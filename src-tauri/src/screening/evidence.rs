//! Tier 4.1: AI summary as screening evidence (complementarity mode).
//!
//! Pure (no I/O, no DB). Given an article's AI-summary blob (Option) and its
//! criteria-ranked chunks (`ScoredChunk` slice), pick the best evidence source
//! and format it for the screening prompt's `## Supporting Evidence` block.
//!
//! **Complementarity (Q1 = B):** when BOTH an AI summary and chunks exist, send
//! the summary's `structured_extraction` facts PLUS the single highest-ranked
//! verbatim chunk as a grounding citation (NOT strict priority). The system
//! prompt's explicit "_cross-check any summary fact against the verbatim chunk_"
//! instruction is the hallucination-propagation mitigation.
//!
//! Evidence hierarchy:
//! 1. AI summary present AND >= 1 chunk survived ranking -> `AiSummaryWithChunk`
//!    (summary's `structured_extraction` facts + the single highest-ranked chunk
//!    as a verbatim grounding citation).
//! 2. AI summary present AND no chunks -> `AiSummaryAlone`.
//! 3. No AI summary AND chunks present -> `Chunks` (Tier 3 behavior).
//! 4. Neither -> `None`.
//!
//! The chunks-only path is byte-identical to the Tier 3 `format_chunks_as_evidence`
//! output so abstract-mode + chunks-only-mode prompts stay stable.

use crate::db::chunk_repo;
use crate::screening::chunk_retrieval::{
    rank_chunks_by_criteria, ScoredChunk, DEFAULT_MAX_CHUNK_WORDS,
};
use crate::screening::engine::ScreeningConfig;
use rusqlite::Connection;

/// The resolved evidence for one article, in complementarity order.
#[derive(Debug, Clone)]
pub struct ScreeningEvidence {
    /// Which source path was taken.
    pub source_type: EvidenceSource,
    /// The formatted evidence block body (without the `## Supporting Evidence`
    /// header; the caller adds that). Empty string when `source_type == None`.
    pub text: String,
    /// Deduped section labels for the audit trail (e.g. `"AI Summary + §Methods"`).
    /// Stable order: AI Summary marker first (when used), then chunk section
    /// labels in ranked order. `"§Full text"` when chunks have no section labels.
    pub sections_label: String,
}

/// The evidence source path chosen by `resolve_evidence`.
#[derive(Debug, Clone, PartialEq)]
pub enum EvidenceSource {
    /// Summary's structured_extraction facts + top-1 verbatim chunk (complementarity).
    /// ~400 tokens. The recommended path when both candidates exist.
    AiSummaryWithChunk,
    /// AI summary alone, when no chunks survived ranking. ~250 tokens.
    AiSummaryAlone,
    /// Chunks alone, when no AI summary exists (Tier 3 behavior). ~600 tokens.
    Chunks,
    /// Neither - abstract-only fallback. No evidence block is emitted.
    None,
}

/// Pick the best evidence source and format it for the screening prompt.
///
/// **Complementarity (Q1 = B):** when BOTH an AI summary and chunks exist, send
/// the summary's `structured_extraction` facts PLUS the single highest-ranked
/// verbatim chunk. NOT strict priority. See module docs.
///
/// Per CLAUDE.md line 89 (`_never deserialize untrusted input as serde_json::Value
/// without validation_`), the AI-summary JSON is validated to be an object before
/// any field access. Malformed/hand-crafted blobs fall back to `Chunks` (no panic).
///
/// Pure function: callers pass in the candidate data. No DB, no I/O.
#[must_use]
pub fn resolve_evidence(
    ai_summary_json: Option<&str>,
    scored_chunks: &[ScoredChunk],
) -> ScreeningEvidence {
    let summary = ai_summary_json.and_then(parse_summary_blob);
    let has_chunks = !scored_chunks.is_empty();

    match (summary, has_chunks) {
        (Some(summary), true) => {
            // Q1 = B complementarity: summary facts + top-1 verbatim chunk.
            let top_chunk = &scored_chunks[0];
            let text = format_ai_summary_with_chunk(&summary, top_chunk);
            let chunk_section = top_chunk.section.as_deref().unwrap_or("Full text");
            let sections_label = format!("AI Summary + §{chunk_section}");
            ScreeningEvidence {
                source_type: EvidenceSource::AiSummaryWithChunk,
                text,
                sections_label,
            }
        }
        (Some(summary), false) => {
            // Summary alone, no chunks survived ranking.
            let text = format_ai_summary_alone(&summary);
            ScreeningEvidence {
                source_type: EvidenceSource::AiSummaryAlone,
                text,
                sections_label: "AI Summary".to_string(),
            }
        }
        (None, true) => {
            // Tier 3 chunks-only behavior: byte-identical to format_chunks_as_evidence.
            let text = format_chunks_as_evidence(scored_chunks);
            let sections_label = build_chunks_sections_label(scored_chunks);
            ScreeningEvidence { source_type: EvidenceSource::Chunks, text, sections_label }
        }
        (None, false) => {
            // Abstract-only fallback.
            ScreeningEvidence {
                source_type: EvidenceSource::None,
                text: String::new(),
                sections_label: String::new(),
            }
        }
    }
}

// ── Internal: summary parsing + formatting ──────────────────────────────────

/// A parsed AI-summary blob, reduced to the fields the evidence block needs.
#[derive(Debug, Clone)]
struct ParsedSummary {
    field: Option<String>,
    structured_extraction: serde_json::Map<String, serde_json::Value>,
    digest: Option<String>,
}

/// Parse and validate the AI-summary JSON blob.
///
/// Per CLAUDE.md line 89, validate the top-level is an object before reaching
/// into fields. Returns `None` on malformed JSON or non-object top-level so the
/// caller falls back to `Chunks` (no panic).
fn parse_summary_blob(raw: &str) -> Option<ParsedSummary> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let obj = value.as_object()?;
    let field = obj.get("field").and_then(|v| v.as_str()).map(|s| s.to_string());
    let structured_extraction =
        obj.get("structured_extraction").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    let digest = obj.get("summary_150_250_words").and_then(|v| v.as_str()).map(|s| s.to_string());
    // Require at least a digest OR a non-empty structured_extraction to use the
    // summary path. An empty `field` + empty `structured_extraction` + no digest
    // is a degenerate blob that should fall back to chunks.
    let has_facts = !structured_extraction.is_empty();
    if digest.is_none() && !has_facts {
        return None;
    }
    Some(ParsedSummary { field, structured_extraction, digest })
}

/// Format the complementarity block: summary facts + top-1 verbatim chunk.
fn format_ai_summary_with_chunk(summary: &ParsedSummary, top_chunk: &ScoredChunk) -> String {
    let mut block = String::new();
    block.push_str("[Source: AI Summary - structured extraction]\n");
    if let Some(field) = &summary.field {
        block.push_str(&format!("field: {field}\n"));
    }
    // Emit all non-empty structured_extraction facts as `key: value` lines.
    // Known keys are emitted first (in a stable order), then any unknown keys
    // (forward-compatible with future fields). Empty string values are skipped.
    let known_order = [
        "study_type",
        "population",
        "intervention_exposure",
        "comparator",
        "outcomes",
        "statistical_results",
        "clinical_area",
    ];
    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for key in known_order {
        if let Some(value) = summary.structured_extraction.get(key).and_then(|v| v.as_str()) {
            if !value.is_empty() {
                block.push_str(&format!("{key}: {value}\n"));
                emitted.insert(key);
            }
        }
    }
    for (key, value) in &summary.structured_extraction {
        if emitted.contains(key.as_str()) {
            continue;
        }
        if let Some(s) = value.as_str() {
            if !s.is_empty() {
                block.push_str(&format!("{key}: {s}\n"));
            }
        }
    }

    if let Some(digest) = &summary.digest {
        block.push_str("\n[Source: AI Summary - digest]\n");
        block.push_str(digest);
        block.push('\n');
    }

    // Top-1 verbatim chunk (complementarity grounding citation).
    let chunk_section = top_chunk.section.as_deref().unwrap_or("Full text");
    block.push_str(&format!(
        "\n[Source: Full Text - verbatim, §{chunk_section}]\n{}",
        top_chunk.content
    ));
    block
}

/// Format the summary-alone block: facts + digest, no chunk.
fn format_ai_summary_alone(summary: &ParsedSummary) -> String {
    let mut block = String::new();
    block.push_str("[Source: AI Summary - structured extraction]\n");
    if let Some(field) = &summary.field {
        block.push_str(&format!("field: {field}\n"));
    }
    for (key, value) in &summary.structured_extraction {
        if let Some(s) = value.as_str() {
            if !s.is_empty() {
                block.push_str(&format!("{key}: {s}\n"));
            }
        }
    }
    if let Some(digest) = &summary.digest {
        block.push_str("\n[Source: AI Summary - digest]\n");
        block.push_str(digest);
    }
    block
}

// ── Internal: chunks formatting (byte-identical to Tier 3) ──────────────────

/// Format scored chunks as the `[§Methods] ...` body for the chunks-only path.
///
/// **Delegate.** The canonical implementation lives in
/// `chunk_retrieval::format_chunks_as_evidence` (the lowest-level module both
/// `engine` and this module depend on). That function returns `Option<String>`
/// (`None` on empty); the chunks-only path is only reached when at least one
/// chunk survived ranking, so we unwrap to `String` here.
///
/// Byte-identical to the Tier 3 `engine::format_chunks_as_evidence` output so
/// the chunks-only path produces stable prompts (covered by
/// `resolve_evidence_chunks_path_unchanged_from_tier3`).
fn format_chunks_as_evidence(chunks: &[ScoredChunk]) -> String {
    crate::screening::chunk_retrieval::format_chunks_as_evidence(chunks).unwrap_or_default()
}

/// Build the deduped section-label string for the audit trail, in ranked order.
/// Mirrors `rank_and_format_evidence`'s label logic so the chunks-only audit
/// entry matches the pre-T4.1 label byte-for-byte.
fn build_chunks_sections_label(chunks: &[ScoredChunk]) -> String {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let labels: Vec<String> = chunks
        .iter()
        .filter_map(|c| c.section.as_deref())
        .filter(|s| seen.insert(*s))
        .map(|s| format!("§{s}"))
        .collect();
    if labels.is_empty() {
        "§Full text".to_string()
    } else {
        labels.join(", ")
    }
}

// Note: the pure-module tests for `resolve_evidence` live in the standalone
// integration test file `src-tauri/tests/evidence_test.rs` (extracted per
// CLAUDE.md lines 147-148: "Avoid large inline unit tests in library source
// files... instead, move them into standalone integration test files").

/// The evidence body string plus the deduped section labels that survived
/// ranking (e.g. `"§Methods, §Results"`), for the audit trail.
#[derive(Debug, Clone)]
pub(crate) struct ArticleEvidence {
    /// The formatted `[§Methods] ...` block (the `full_text_evidence` value).
    pub text: String,
    /// Section labels actually present in the retrieved chunks, joined for the
    /// audit detail (e.g. `"§Methods, §Results"`). Stable order: deduped,
    /// preserved in retrieval (highest-ranked-first) order.
    pub sections_label: String,
}

/// Rank the given chunks against the criteria text, returning the top-K
/// scored chunks (no DB, no formatting). Pure helper.
pub(crate) fn rank_evidence_chunks(
    chunks: Vec<crate::utils::chunking::Chunk>,
    inclusion_texts: &[String],
    exclusion_texts: &[String],
    config: &ScreeningConfig,
) -> Vec<ScoredChunk> {
    let allow = &config.enhanced_sections;
    let filtered: Vec<_> = chunks
        .into_iter()
        .filter(|c| match c.section.as_deref() {
            Some(s) => allow.iter().any(|a| a.eq_ignore_ascii_case(s)),
            None => true,
        })
        .collect();
    if filtered.is_empty() {
        return Vec::new();
    }
    rank_chunks_by_criteria(
        &filtered,
        inclusion_texts,
        exclusion_texts,
        config.enhanced_top_k,
        DEFAULT_MAX_CHUNK_WORDS,
        config.chunk_budget_per_article,
    )
}

/// Tier 3 + Tier 4.1: retrieve + rank + resolve the supporting evidence for one
/// article. Reads the AI-summary blob AND chunks from `article_chunks`,
/// ranks the chunks, then delegates to `resolve_evidence`.
pub(crate) fn retrieve_evidence_for_article(
    conn: &Connection,
    article_id: &str,
    inclusion_texts: &[String],
    exclusion_texts: &[String],
    config: &ScreeningConfig,
) -> Option<ArticleEvidence> {
    let ai_summary_json: Option<String> = conn
        .query_row(
            "SELECT full_text_ai_summary FROM articles WHERE id = ?1",
            rusqlite::params![article_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    let chunks = chunk_repo::list_chunks_for_article(conn, article_id).ok()?;
    let scored = rank_evidence_chunks(chunks, inclusion_texts, exclusion_texts, config);
    let evidence = resolve_evidence(ai_summary_json.as_deref(), &scored);
    if evidence.source_type == EvidenceSource::None {
        return None;
    }
    Some(ArticleEvidence { text: evidence.text, sections_label: evidence.sections_label })
}
