#[derive(Clone)]
pub struct ArticleSummary {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub abstract_text: String,
    pub keywords: Vec<String>,
    /// Optional supporting evidence distilled from the article's
    /// `full_text_ai_summary` blob (Shape A). When `Some`, the prompt assembler
    /// appends an `Evidence:` block after the abstract so the literature-review
    /// LLM gains access to structured study facts (study design, sample size,
    /// effect sizes). `None` preserves the legacy abstract-only prompt
    /// byte-for-byte (backward compat).
    pub evidence: Option<String>,
}

#[derive(Clone)]
pub struct ScreeningData {
    pub records_identified: usize,
    pub duplicates_removed: usize,
    pub records_screened: usize,
    pub records_excluded: usize,
    pub records_excluded_with_reasons: usize,
    pub records_assessed: usize,
    pub records_in_progress: usize,
    pub studies_included: usize,
    pub ai_screened: usize,
    pub manual_reviewed: usize,
    pub exclusion_reasons: Vec<(String, usize)>,
}

pub struct SummaryPromptInput {
    pub aims: Vec<String>,
    pub screening_data: ScreeningData,
    pub citation_style: String,
    pub articles: Vec<ArticleSummary>,
    /// Full inclusion criterion definitions (Shape 0). Rendered into the
    /// Methodology context so the LLM can name the actual eligibility rules
    /// instead of inferring them from aggregate exclusion counts. Empty when
    /// no inclusion criteria are defined.
    pub inclusion_criteria: Vec<String>,
    /// Full exclusion criterion definitions (Shape 0). Same role as above.
    pub exclusion_criteria: Vec<String>,
}

pub const SYSTEM_PROMPT: &str = "You are an expert academic literature review writer. You produce well-structured, scholarly literature reviews with proper in-text citations and a complete references section. You only cite sources that are explicitly provided. You never fabricate references. You write in formal academic English with natural variation in sentence length. You never use em dashes.";

/// System prompt for the single-article AI summary (full-text analysis).
/// Produces a structured JSON-only response matching the `AiSummaryData`
/// schema consumed by the frontend (`src/composables/use-ai-summary.ts`).
pub const ARTICLE_SUMMARY_SYSTEM_PROMPT: &str = include_str!("ai_article_summary_prompt.md");

/// System prompt variant that also requests per-section summaries.
///
/// Used by `generate_article_ai_summary` when `include_section_summaries` is
/// true AND `classify_sections` detected at least one high-value section
/// (Methods/Results/Discussion). The model returns the standard
/// `AiSummaryData` fields PLUS a `section_summaries` array. The frontend
/// `parseAiSummary` treats `section_summaries` as optional, so v1 callers
/// keep working unchanged.
pub const ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT: &str =
    include_str!("ai_article_summary_with_sections_prompt.md");

/// System prompt for the batched figure/table caption description (Tier 2
/// Phase 4). Grounded "caption parser": the model summarizes what each caption
/// *states* and reproduces quantitative values mentioned in the caption text;
/// it must not invent visual details not present in the caption.
pub const FIGURE_DESCRIPTION_SYSTEM_PROMPT: &str = include_str!("figure_description_prompt.md");

/// Tier 1 fallback: simple markdown-structured prompt for models that struggle
/// with complex JSON schemas (e.g., reasoning models that consume their output
/// budget on thinking tokens). Parsed by `parse_markdown_summary`.
pub const ARTICLE_SUMMARY_MARKDOWN_FALLBACK_PROMPT: &str =
    include_str!("ai_article_summary_markdown_fallback_prompt.md");

use crate::error::AppError;
// `strip_code_fences`, `escape_control_chars_in_json`, and the combined
// `prepare_llm_json` live in `utils::json_repair` so the orchestrator's
// `send_json` can use them without taking a summary-module dependency. They are
// re-exported here for backward compatibility with existing callers that import
// them from `summary::prompt`.
pub use crate::utils::json_repair::{
    escape_control_chars_in_json, prepare_llm_json, strip_code_fences,
};

/// One LLM-described figure/table caption (Tier 2 Phase 4).
///
/// Stored in the `full_text_ai_summary` JSON blob under `figures`/`tables`.
/// `caption` is the verbatim extracted caption text; `description` is the
/// grounded LLM summary of what the caption states.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FigureDescription {
    /// The figure or table number as a string: "1", "2a".
    pub number: String,
    /// The verbatim caption text extracted by `extract_captions`.
    #[serde(default)]
    pub caption: String,
    /// The grounded LLM summary of what the caption states.
    #[serde(default)]
    pub description: String,
}

/// Tier 4.2/4.3: one LLM-described table caption with the optional `markdown`
/// GFM column for tables-as-GFM rendering. Mirrors the TS `TableDescription`
/// interface in `src/composables/use-ai-summary.ts`.
///
/// Stored in the `full_text_ai_summary` JSON blob under `tables`. The
/// `markdown` field carries the preserved GFM rows extracted by
/// `detect_markdown_tables` (T2.2), so the frontend can render the table
/// natively instead of showing the caption + description text only. Old blobs
/// without `markdown` render text-only (the field is optional / `#[serde(default)]`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TableDescription {
    /// The table number as a string: "1", "2a".
    pub number: String,
    /// The verbatim extracted caption text.
    #[serde(default)]
    pub caption: String,
    /// GFM markdown rows extracted from the full text (T2.2). Empty when no
    /// `detect_markdown_tables` match was found for this table's number.
    #[serde(default)]
    pub markdown: String,
    /// The grounded LLM summary of what the caption states.
    #[serde(default)]
    pub description: String,
}

/// Render the user prompt for the batched figure/table description call.
///
/// The prompt includes the paper title and one numbered block per caption
/// (`[N] <caption text>`). Pure function: no I/O.
#[must_use]
pub fn build_figure_description_prompt(
    title: &str,
    captions: &[crate::utils::sections::Caption],
) -> String {
    let mut blocks = Vec::with_capacity(captions.len());
    for c in captions {
        let kind = c.kind.label();
        blocks.push(format!(
            "[{} {}] {}",
            kind,
            c.number,
            if c.caption.trim().is_empty() { "(no caption text)" } else { c.caption.trim() }
        ));
    }
    let captions_block = blocks.join("\n");
    format!(
        "## Paper Title\n{title}\n\n## Captions\n\n{captions_block}\n\n\
         For each caption above, return a JSON array of objects with `number` and `description` keys."
    )
}

/// Parse the batched figure/table description LLM response into a list.
///
/// Tolerates markdown code fences and trailing/leading whitespace. Returns
/// an error (no panic) on malformed JSON. Each element must have a `number`;
/// `description` defaults to an empty string when absent.
pub fn parse_figure_descriptions_response(
    response: &str,
) -> Result<Vec<FigureDescription>, AppError> {
    // `prepare_llm_json` chains strip_code_fences + escape_control_chars_in_json.
    // Not `screening_engine::extract_json` (that helper corrupts object-shaped
    // responses — see `utils::json_repair::strip_code_fences` docs).
    let prepared = prepare_llm_json(response);
    let value: serde_json::Value = serde_json::from_str(&prepared)
        .map_err(|e| AppError::Import(format!("Invalid JSON for figure descriptions: {e}")))?;
    let arr = value.as_array().ok_or_else(|| {
        AppError::Import("Figure descriptions response is not a JSON array".to_string())
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for elem in arr {
        let obj = elem.as_object().ok_or_else(|| {
            AppError::Import("Figure description element is not a JSON object".to_string())
        })?;
        let number = obj
            .get("number")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Import("Figure description missing `number`".to_string()))?
            .to_string();
        let description =
            obj.get("description").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        out.push(FigureDescription { number, caption: String::new(), description });
    }
    Ok(out)
}

/// Merge figure/table descriptions into the `full_text_ai_summary` blob.
///
/// - `existing_blob`: the current JSON string (may be empty or malformed;
///   malformed blobs are treated as empty so the merge never panics).
/// - `figures` / `tables`: the descriptions keyed by number. Only entries whose
///   `number` matches an extracted caption get the `caption` text attached.
/// - Preserves all existing top-level fields (including `section_summaries`),
///   adds/replaces `figures` + `tables`, and stamps `schema_version: 2`.
///
/// Returns the serialized JSON string ready for `article_repo::set_ai_summary`.
#[must_use]
pub fn merge_figure_descriptions_into_blob(
    existing_blob: Option<&str>,
    figures: Vec<FigureDescription>,
    tables: Vec<FigureDescription>,
) -> String {
    let mut value: serde_json::Value = existing_blob
        .and_then(|raw| serde_json::from_str(raw).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert("figures".to_string(), serde_json::to_value(&figures).unwrap_or_default());
        obj.insert("tables".to_string(), serde_json::to_value(&tables).unwrap_or_default());
        obj.insert("schema_version".to_string(), serde_json::Value::from(2));
    }
    value.to_string()
}

/// Merge a freshly-generated summary blob into the existing blob, preserving
/// `figures`/`tables` (and any other keys the summary path does not produce).
///
/// This closes the `set_ai_summary` overwrite footgun (Tier 4 Phase 0): without
/// it, `generate_article_ai_summary` wipes `figures`/`tables` on regen because
/// the freshly-generated summary blob does not include them. With this helper,
/// the summary path mirrors `merge_figure_descriptions_into_blob`'s
/// preserve-on-write contract, so the two commands compose safely regardless
/// of ordering.
///
/// - `existing_blob`: the current JSON string (may be empty or malformed;
///   malformed blobs are treated as empty so the merge never panics).
/// - `fresh_summary_json`: the freshly-generated summary blob (must be a JSON
///   object; if it is malformed, the existing blob is returned unchanged with
///   `schema_version` bumped per `force_v2`).
/// - `force_v2`: when true, stamps `schema_version: 2` on the merged blob
///   (passed as `used_section_path` from the caller so the section-aware path
///   guarantees v2 per the T1.3 contract). When false, preserves the existing
///   `schema_version` if present (no downgrade).
///
/// Mirrors `merge_figure_descriptions_into_blob`'s pure, no-panic contract.
/// Returns the serialized JSON string ready for `article_repo::set_ai_summary`.
#[must_use]
pub fn merge_summary_into_blob(
    existing_blob: Option<&str>,
    fresh_summary_json: &str,
    force_v2: bool,
) -> String {
    let mut merged: serde_json::Value = existing_blob
        .and_then(|raw| serde_json::from_str(raw).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let fresh: serde_json::Value = serde_json::from_str(fresh_summary_json).unwrap_or_else(|_| {
        // Malformed fresh summary: return the existing blob (with v2 stamp per
        // force_v2). Never panic; the caller's own validation should have caught
        // this, but the helper stays defensive.
        if force_v2 {
            ensure_schema_version_v2(&mut merged);
        }
        merged.clone()
    });
    // Overlay the fresh summary's keys onto the existing object. Keys the
    // summary produces (summary_150_250_words, section_summaries, etc.) are
    // overwritten; keys it does NOT produce (figures, tables) are preserved.
    if let (Some(existing_obj), Some(fresh_obj)) = (merged.as_object_mut(), fresh.as_object()) {
        for (k, v) in fresh_obj {
            existing_obj.insert(k.clone(), v.clone());
        }
    } else if fresh.is_object() {
        // Existing was malformed/non-object; the fresh summary wins outright.
        merged = fresh;
    }
    if force_v2 {
        ensure_schema_version_v2(&mut merged);
    }
    merged.to_string()
}

/// Tier 4.2: Build the synthesis user prompt that asks the LLM to synthesize a
/// unified 150-250 word digest FROM the per-section summaries (so the digest is
/// consistent with, not contradictory to, the section data).
///
/// The prompt receives the paper title + field + the per-section summaries as
/// input context, and asks for a single `summary_150_250_words` digest plus
/// `key_insights` + `keywords` that incorporate the specific facts from the
/// sections. Pure function: no I/O.
#[must_use]
pub fn build_synthesis_prompt(title: &str, field: &str, section_summaries_json: &str) -> String {
    format!(
        "## Paper Title\n{title}\n\n## Field\n{field}\n\n\
         ## Per-Section Summaries (synthesize the digest FROM these)\n\
         {section_summaries_json}\n\n\
         Synthesize a unified 150-250 word digest (`summary_150_250_words`) that \
         incorporates the specific facts from the sections above. Also return \
         `key_insights` (3-5 bullets) and `keywords` (5-10 terms) consistent with \
         the section data. Return ONLY a JSON object with keys: \
         `summary_150_250_words`, `key_insights`, `keywords`."
    )
}

/// Tier 4.2: Merge the per-section summaries, figure/table descriptions, and
/// synthesis digest into one unified blob. Single-write composition: the caller
/// produces all parts, then this helper merges them into one `set_ai_summary`
/// write so there is no intermediate state where some keys are missing.
///
/// - `existing_blob`: the current JSON string (preserves unknown keys; malformed
///   blobs are treated as empty so the merge never panics).
/// - `section_summaries_json`: the `section_summaries` array as a JSON string
///   (from T1.3's per-section calls).
/// - `figures` / `tables`: the LLM-described figure/table captions (T2.1).
/// - `synthesis_digest_json`: the synthesis call's `{summary_150_250_words,
///   key_insights, keywords}` blob (T4.2 step 4).
///
/// Stamps `schema_version: 2`. Returns the serialized JSON string ready for
/// `article_repo::set_ai_summary`.
#[must_use]
pub fn merge_unified_blob(
    existing_blob: Option<&str>,
    section_summaries_json: &str,
    figures: Vec<FigureDescription>,
    tables: Vec<TableDescription>,
    synthesis_digest_json: &str,
) -> String {
    // Start from the existing blob (preserves unknown keys like `structured_extraction`).
    let mut value: serde_json::Value = existing_blob
        .and_then(|raw| serde_json::from_str(raw).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));

    // Overlay the section_summaries array (parse the JSON string; on malformed
    // input, skip the key rather than panicking).
    if let Ok(arr) = serde_json::from_str::<serde_json::Value>(section_summaries_json) {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("section_summaries".to_string(), arr);
        }
    }

    // Overlay figures + tables.
    if let Some(obj) = value.as_object_mut() {
        obj.insert("figures".to_string(), serde_json::to_value(&figures).unwrap_or_default());
        obj.insert("tables".to_string(), serde_json::to_value(&tables).unwrap_or_default());
    }

    // Overlay the synthesis digest keys (summary_150_250_words, key_insights, keywords).
    if let Ok(digest) = serde_json::from_str::<serde_json::Value>(synthesis_digest_json) {
        if let (Some(obj), Some(digest_obj)) = (value.as_object_mut(), digest.as_object()) {
            for (k, v) in digest_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    // Stamp schema_version: 2.
    if let Some(obj) = value.as_object_mut() {
        obj.insert("schema_version".to_string(), serde_json::Value::from(2));
    }

    value.to_string()
}

/// Tier 1 fallback: Parse a markdown-structured summary response into the same
/// JSON blob shape the primary JSON path produces. This is the robust fallback
/// for models that struggle with complex JSON schemas (e.g., reasoning models
/// that consume their output budget on thinking tokens).
///
/// Expected markdown format (headings + body):
/// ```text
/// ## Field
/// medicine / public_health
///
/// ## Summary
/// <150-250 word digest>
///
/// ## Key Insights
/// - Insight 1
/// - Insight 2
///
/// ## Keywords
/// sugar, tax, SSB, obesity
///
/// ## Structured Extraction
/// study_type: RCT
/// population: N=1000 children
/// ```
///
/// Unknown headings are ignored. Missing headings produce empty defaults.
/// `schema_version` is stamped to `2` so the frontend renders the enriched view.
///
/// Pure function: no I/O. Returns a JSON string ready for `set_ai_summary`.
#[must_use]
pub fn parse_markdown_summary(markdown: &str) -> String {
    let mut field = String::new();
    let mut subfield = String::new();
    let mut summary = String::new();
    let mut key_insights: Vec<String> = Vec::new();
    let mut keywords: Vec<String> = Vec::new();
    let mut structured_extraction: serde_json::Map<String, serde_json::Value> =
        serde_json::Map::new();

    // Split into heading-delimited sections.
    let mut current_heading: Option<&str> = None;
    let mut current_body = String::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            // Flush the previous section.
            flush_section(
                current_heading,
                &current_body,
                &mut field,
                &mut subfield,
                &mut summary,
                &mut key_insights,
                &mut keywords,
                &mut structured_extraction,
            );
            current_heading = Some(trimmed.trim_start_matches("## ").trim());
            current_body.clear();
        } else {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }
    // Flush the last section.
    flush_section(
        current_heading,
        &current_body,
        &mut field,
        &mut subfield,
        &mut summary,
        &mut key_insights,
        &mut keywords,
        &mut structured_extraction,
    );

    // Build the blob with safe defaults.
    let blob = serde_json::json!({
        "schema_version": 2,
        "field": field,
        "subfield": subfield,
        "structured_extraction": structured_extraction,
        "summary_150_250_words": summary,
        "key_insights": key_insights,
        "keywords": keywords,
    });
    blob.to_string()
}

/// Helper: parse a heading section's body into the appropriate fields.
///
/// `flush_section` has 6 distinct mutable output targets (field, subfield,
/// summary, key_insights, keywords, structured_extraction) plus 2 inputs
/// (heading, body). Each is a separate semantic destination, so the signature
/// is clearer than introducing a builder/accumulator struct for an internal
/// helper.
#[allow(clippy::too_many_arguments)]
fn flush_section(
    heading: Option<&str>,
    body: &str,
    field: &mut String,
    subfield: &mut String,
    summary: &mut String,
    key_insights: &mut Vec<String>,
    keywords: &mut Vec<String>,
    structured_extraction: &mut serde_json::Map<String, serde_json::Value>,
) {
    let Some(h) = heading else { return };
    let trimmed_body = body.trim();
    if trimmed_body.is_empty() {
        return;
    }
    match h.to_lowercase().as_str() {
        "field" => {
            // "medicine / public_health" -> field="medicine", subfield="public_health"
            let parts: Vec<&str> = trimmed_body.split('/').map(|s| s.trim()).collect();
            if !parts.is_empty() {
                *field = parts[0].to_string();
                if parts.len() > 1 {
                    *subfield = parts[1..].join(" / ");
                }
            }
        }
        "summary" => {
            *summary = trimmed_body.to_string();
        }
        "key insights" | "insights" => {
            // Bullet lines starting with `-` or `*`.
            *key_insights = trimmed_body
                .lines()
                .filter_map(|l| {
                    let l = l.trim();
                    l.strip_prefix('-')
                        .or_else(|| l.strip_prefix('*'))
                        .map(|s| s.trim().to_string())
                })
                .filter(|s| !s.is_empty())
                .collect();
        }
        "keywords" => {
            // Comma-separated or bullet-separated.
            if trimmed_body.contains(',') {
                *keywords = trimmed_body
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else {
                *keywords = trimmed_body
                    .lines()
                    .filter_map(|l| {
                        let l = l.trim();
                        l.strip_prefix('-')
                            .or_else(|| l.strip_prefix('*'))
                            .map(|s| s.trim().to_string())
                    })
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        "structured extraction" | "extraction" => {
            // `key: value` lines.
            for line in trimmed_body.lines() {
                let line = line.trim();
                if let Some((k, v)) = line.split_once(':') {
                    let k = k.trim().to_string();
                    let v = v.trim().to_string();
                    if !k.is_empty() && !v.is_empty() {
                        structured_extraction.insert(k, serde_json::Value::String(v));
                    }
                }
            }
        }
        _ => {} // Ignore unknown headings.
    }
}

/// High-value section kinds that the section-aware summary extracts.
///
/// `Introduction` / `Conclusion` / `Abstract` are deliberately excluded:
/// they are either already covered by the whole-paper summary or are low
/// value as standalone section summaries for systematic-review work.
const HIGH_VALUE_SECTION_KINDS: &[crate::utils::sections::SectionKind] = &[
    crate::utils::sections::SectionKind::Methods,
    crate::utils::sections::SectionKind::Results,
    crate::utils::sections::SectionKind::Discussion,
];

/// Filter `classify_sections` output to the high-value subset (Methods /
/// Results / Discussion).
///
/// Returns the input unchanged if it is empty or contains only `Text` /
/// `References` (the degenerate "no real sections detected" case). The
/// caller uses an empty result to skip the section-aware prompt branch.
#[must_use]
pub fn filter_high_value_sections(
    sections: &[crate::utils::sections::Section],
) -> Vec<crate::utils::sections::Section> {
    sections.iter().filter(|s| HIGH_VALUE_SECTION_KINDS.contains(&s.kind)).cloned().collect()
}

/// Ensure the parsed AI-summary JSON blob carries `schema_version: 2`.
///
/// Per the T1.3 contract (`chunkingplan.md` §T1.3), the backend MUST guarantee
/// `schema_version: 2` when the section-aware summary path runs, regardless of
/// whether the model emitted the field. This keeps frontend `parseAiSummary`
/// gating reliable: a blob with `schema_version >= 2` is rendered via the
/// enriched view, a blob without it (or `1`) renders via the legacy view.
///
/// - If `value` is an object and `schema_version` is missing or < 2, it is set
///   to `2`.
/// - If `value` is not an object, the function is a no-op (defensive: a
///   malformed top-level array/string is left untouched; the caller's validation
///   already confirmed it is a JSON object in practice).
///
/// Pure function: no I/O. Tested directly.
pub fn ensure_schema_version_v2(value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let needs_bump = match obj.get("schema_version").and_then(|v| v.as_i64()) {
        Some(existing) => existing < 2,
        None => true,
    };
    if needs_bump {
        obj.insert("schema_version".to_string(), serde_json::Value::from(2));
    }
}

/// Render detected high-value sections into a structured block for the LLM
/// user prompt.
///
/// Each section is rendered as:
///
/// ```text
/// === SECTION: Methods ===
/// <section body>
/// ```
///
/// Sections with empty bodies are skipped. Returns an empty string when no
/// sections are provided (the caller falls back to the standard prompt).
#[must_use]
pub fn build_section_context(sections: &[crate::utils::sections::Section]) -> String {
    let mut blocks = Vec::with_capacity(sections.len());
    for s in sections {
        let body = s.body.trim();
        if body.is_empty() {
            continue;
        }
        let label = s.kind.label();
        blocks.push(format!("=== SECTION: {label} ===\n{body}"));
    }
    blocks.join("\n\n")
}

/// Distill a per-article `full_text_ai_summary` JSON blob into a compact
/// evidence string for the project-wide literature-review prompt (Shape A).
///
/// Extracts the highest-signal fields the literature-review LLM can directly
/// use to synthesize patterns across studies:
/// - `field` / `subfield` (research domain)
/// - `structured_extraction` facts (`study_type`, `population`,
///   `intervention_exposure`, `outcomes`, `effect_size`, etc.) as `key: value`
///   lines
/// - `summary_150_250_words` digest (truncated to keep the prompt bounded)
///
/// Returns `None` when the blob is missing, malformed, or carries no usable
/// facts (so the caller leaves `ArticleSummary.evidence = None` and the prompt
/// stays byte-identical to the legacy abstract-only path). Never panics:
/// malformed JSON falls back to `None` (per CLAUDE.md line 89 - validate before
/// field access).
///
/// Pure function: no I/O. Tested directly.
#[must_use]
pub fn format_ai_summary_as_evidence(blob: Option<&str>) -> Option<String> {
    let raw = blob?;
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let obj = value.as_object()?;
    let mut lines: Vec<String> = Vec::new();

    // Field / subfield (research domain).
    if let Some(field) = obj.get("field").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        lines.push(format!("field: {field}"));
    }
    if let Some(sub) = obj.get("subfield").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        lines.push(format!("subfield: {sub}"));
    }

    // Structured extraction facts (the highest-value fields for pattern
    // synthesis). Emit known keys in a stable order first, then any unknown
    // string-valued keys (forward-compatible). Skip empty values.
    if let Some(extraction) = obj.get("structured_extraction").and_then(|v| v.as_object()) {
        let known_order = [
            "study_type",
            "study_design",
            "population",
            "sample_size",
            "intervention_exposure",
            "comparator",
            "outcomes",
            "effect_size",
            "confidence_interval",
            "statistical_results",
            "clinical_area",
        ];
        let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for key in known_order {
            if let Some(value) =
                extraction.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
            {
                lines.push(format!("{key}: {value}"));
                emitted.insert(key);
            }
        }
        for (key, value) in extraction {
            if emitted.contains(key.as_str()) {
                continue;
            }
            if let Some(s) = value.as_str().filter(|s| !s.is_empty()) {
                lines.push(format!("{key}: {s}"));
            }
        }
    }

    // Digest (truncated to keep the prompt bounded; 600 chars ~ 150 tokens).
    if let Some(digest) =
        obj.get("summary_150_250_words").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
    {
        let truncated = if digest.len() > 600 { &digest[..600] } else { digest };
        lines.push(format!("digest: {truncated}"));
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("; "))
    }
}

/// Format screening statistics into a human-readable summary for the prompt.
///
/// When `inclusion_criteria` / `exclusion_criteria` are non-empty (Shape 0),
/// the full criterion definitions are rendered so the LLM can name the actual
/// eligibility rules in the Methodology section instead of inferring them from
/// aggregate exclusion counts alone. Empty lists produce no criteria lines
/// (backward compatible with callers that pass `&[]`).
#[must_use]
pub fn format_screening_summary(
    data: &ScreeningData,
    inclusion_criteria: &[String],
    exclusion_criteria: &[String],
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Total records identified: {}", data.records_identified));

    if data.duplicates_removed > 0 {
        lines.push(format!(
            "Duplicates removed: {} ({} unique records after deduplication)",
            data.duplicates_removed, data.records_screened
        ));
    }

    lines.push(format!("Records screened: {}", data.records_screened));

    if data.ai_screened > 0 || data.manual_reviewed > 0 {
        lines.push(format!(
            "Screening method: {} articles were screened using AI-assisted review, {} underwent manual review by the researcher.",
            data.ai_screened, data.manual_reviewed
        ));
    }

    if data.records_excluded > 0 {
        lines.push(format!("Records excluded: {}", data.records_excluded));
        if data.records_excluded_with_reasons > 0 {
            lines.push(format!(
                "Excluded with specific criteria: {}",
                data.records_excluded_with_reasons
            ));
        }
    }

    lines.push(format!("Records assessed for eligibility: {}", data.records_assessed));

    if data.records_in_progress > 0 {
        lines.push(format!("Records still in progress: {}", data.records_in_progress));
    }

    lines.push(format!("Studies included in final review: {}", data.studies_included));

    if !data.exclusion_reasons.is_empty() {
        lines.push("Top exclusion reasons:".to_string());
        for (text, count) in &data.exclusion_reasons {
            lines.push(format!("  - {} ({} articles)", text, count));
        }
    }

    // Shape 0: full criteria definitions so the Methodology narrative can name
    // the actual eligibility rules (not just aggregate exclusion counts).
    if !inclusion_criteria.is_empty() {
        lines.push("Inclusion criteria:".to_string());
        for c in inclusion_criteria {
            lines.push(format!("  - {c}"));
        }
    }
    if !exclusion_criteria.is_empty() {
        lines.push("Exclusion criteria:".to_string());
        for c in exclusion_criteria {
            lines.push(format!("  - {c}"));
        }
    }

    lines.join("\n")
}

#[must_use]
pub fn build_summary_prompt(input: &SummaryPromptInput) -> String {
    let aims_list = if input.aims.is_empty() {
        "None defined.".to_string()
    } else {
        input
            .aims
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{}. {}", i + 1, a))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let screening_summary = format_screening_summary(
        &input.screening_data,
        &input.inclusion_criteria,
        &input.exclusion_criteria,
    );

    let articles_text = input
        .articles
        .iter()
        .map(|a| {
            let year_str = a.year.map(|y| y.to_string()).unwrap_or_else(|| "Unknown".to_string());
            let keywords = if a.keywords.is_empty() {
                String::new()
            } else {
                format!("\nKeywords: {}", a.keywords.join(", "))
            };
            // Shape A: append the distilled evidence block when present so the
            // LLM can cite structured facts (study design, sample size, effect
            // sizes) in the Results/Discussion synthesis. Empty when `None`
            // (abstract-only mode) - preserves the legacy prompt byte-for-byte.
            let evidence = if let Some(ev) = &a.evidence {
                format!("\nEvidence: {ev}")
            } else {
                String::new()
            };
            format!(
                "---\nTitle: {}\nAuthors: {}\nYear: {}\nAbstract: {}{}{}\n---",
                a.title,
                a.authors.join("; "),
                year_str,
                a.abstract_text,
                keywords,
                evidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"## Task
Write a comprehensive, well-structured literature review based on the included articles below.
Focus on the research aims and synthesize findings across studies.

## Research Aims
{aims}

## Search and Screening Methodology (use this data to write the Methodology section)
{screening}

## Citation Style
Use **{citation_style}** citation style for all in-text citations and the references section.

## Included Articles
{articles}

## Instructions
Write a literature review with the following sections. Each section should be substantive and scholarly.
Use **Markdown formatting** with proper headings as shown below:

1. **Title** - An H1 heading (`# Title`) with a descriptive title for the literature review.
2. **Abstract** - An H2 heading (`## Abstract`) followed by a concise summary (150 to 250 words) of the review's scope, key findings, and conclusions.
3. **Introduction** - An H2 heading (`## Introduction`) introducing the research context, stating the research aims, and outlining the review's scope and structure.
4. **Methodology** - An H2 heading (`## Methodology`) describing the search and screening process using the data provided above. Explain how records were identified, how many were screened, the screening approach (AI-assisted and/or manual review), exclusion criteria applied, and how many studies were ultimately included. Write this as a narrative description of the systematic process.
5. **Results** - An H2 heading (`## Results`) presenting the key findings from the included studies, organized by themes. Use H3 subheadings (`### Theme`) for each thematic group. Cite relevant studies using {citation_style} style.
6. **Discussion** - An H2 heading (`## Discussion`) interpreting the findings, identifying patterns and contradictions across studies, and relating them back to the research aims.
7. **Conclusion** - An H2 heading (`## Conclusion`) summarizing the main takeaways, stating implications, and suggesting directions for future research.
8. **References** - An H2 heading (`## References`) followed by a numbered list of all cited works in proper {citation_style} format. Only include references from the provided articles. Do NOT invent references.

## Writing Style Rules
- Do NOT use em dashes (the long dash character) anywhere in the text. Use commas, parentheses, colons, or split into separate sentences instead.
- Write in formal academic prose with natural variation in sentence length. Mix shorter declarative sentences with longer, more complex ones to reflect human academic writing.
- Avoid repetitive sentence structures. Vary how you open paragraphs and transition between ideas.
- Only cite articles that are listed above. Never fabricate or invent references.
- Use proper {citation_style} in-text citations throughout (e.g., author-date, numeric, etc. as appropriate for the style).
- Synthesize findings across studies rather than summarizing each study individually.
- Use Markdown formatting: headings (`#`, `##`, `###`), **bold** for emphasis where appropriate, and proper list formatting for the References section.
- Return only the Markdown text. Do not wrap it in code fences (no ``` markers) or JSON."#,
        aims = aims_list,
        screening = screening_summary,
        citation_style = input.citation_style,
        articles = articles_text,
    )
}
