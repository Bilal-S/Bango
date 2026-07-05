//! Translation engine: owns the `TranslationLlmClient` and the per-article
//! translation pipeline.
//!
//! Two paths:
//! - `translate_metadata_only`: title + abstract translation (for articles
//!   without full text attached).
//! - `translate_full_text`: title + abstract + per-chunk full-text translation
//!   followed by re-chunking of the English text.
//!
//! Per the translation pipeline design and the module `AGENTS.md`:
//! - Originals are persisted to `article_original_content` (and
//!   `article_original_chunks` for the full-text path) BEFORE the working row
//!   is rewritten.
//! - Write-back is a single `rusqlite::Transaction`: update `articles`, write
//!   `translation` audit entry, commit. On any error the transaction rolls
//!   back; no partial rows reach `articles` or `article_chunks`.
//!
//! The engine takes a `&Mutex<Connection>` (mirroring
//! `commands::summary::generate_article_ai_summary_inner`) so the worker can
//! release the lock across the async LLM call - the `MutexGuard` is `!Send`
//! and cannot be held across `.await` inside a `tokio::spawn`ed task.

use std::sync::{Arc, Mutex};

use crate::db::article_original_repo;
use crate::db::article_repo;
use crate::db::audit_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::llm_config::LlmConfig;
use crate::screening::llm_client::LlmClient;
use crate::translation::language::{is_english_abstract, should_skip_translation};

/// System prompt instructing the model to translate title + abstract to
/// English using the deterministic `TITLE:` / `ABSTRACT:` marker format.
const METADATA_TRANSLATION_SYSTEM_PROMPT: &str = "\
You are a professional academic translator. Translate the given article title \
and abstract into clear, accurate English. Preserve technical terminology and \
citations. Output EXACTLY two sections using these markers on their own line:\n\
TITLE:\n<translated English title>\n\nABSTRACT:\n<translated English abstract>\n\
Do not add any commentary before or after these sections.";

/// The diagnostic `TranslationLlmClient`.
///
/// Mirrors `screening::llm_client::HttpLlmClient` but logs the `job_id`
/// (article UUID) before each delegated orchestrator call, and routes through
/// `send_with_type(LlmRequestType::Translation)`. The `LlmClient` trait and
/// `send_with_type` default method are not widened (per plan §LLM Orchestrator
/// and Translation Client). Per-chunk identification (`part_id={idx}`) lives
/// in the `translate_full_text` dispatch log line, not here - the trait
/// signature carries no part_id, so hardcoding one would be wrong for chunk
/// calls (see the note in `send`).
pub struct TranslationLlmClient {
    pub config: LlmConfig,
    pub orchestrator: Arc<LlmOrchestrator>,
    pub job_id: String,
}

#[async_trait::async_trait]
impl LlmClient for TranslationLlmClient {
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError> {
        // NOTE: this method is called for BOTH the metadata (title + abstract)
        // call and each per-chunk call. The per-chunk dispatch log line in
        // `translate_full_text` (`part_id={idx} translating chunk`) is the
        // source of truth for identifying which chunk is in flight; this line
        // only confirms the orchestrator delegation. Do NOT hardcode a part_id
        // or a "(metadata-only)" label here - it would be wrong for chunk calls
        // and floods the log under parallel dispatch.
        eprintln!("[translation] job_id={} delegating via orchestrator", self.job_id);
        self.orchestrator.send(&self.config, system, user, LlmRequestType::Translation).await
    }

    async fn send_with_type(
        &self,
        system: &str,
        user: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        // See the note in `send`: do not hardcode part_id or a metadata-only
        // label here - this method serves both metadata and per-chunk calls.
        eprintln!(
            "[translation] job_id={} delegating via orchestrator ({:?})",
            self.job_id, request_type
        );
        self.orchestrator.send(&self.config, system, user, request_type).await
    }
}

/// Parsed metadata translation response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslatedMetadata {
    pub title: String,
    pub abstract_text: String,
}

/// Find a marker (e.g. `"TITLE:"`) case-insensitively in `response`, returning
/// the byte index of the match in the **original** string. This avoids the
/// Unicode-expansion index-shift bug that `response.to_uppercase().find(...)`
/// would introduce: `to_uppercase()` can change byte lengths (e.g. ligatures
/// like `ﬁ` → `FI`), so an index found in the uppercased copy may not land on a
/// char boundary in the original, causing `str::get()` to return `None` and the
/// parse to fail silently. Searching the original string keeps indices
/// byte-stable.
fn find_marker_ci(haystack: &str, needle: &str) -> Option<usize> {
    let needle_lower = needle.to_ascii_lowercase();
    haystack.to_ascii_lowercase().find(&needle_lower).filter(|&idx| haystack.is_char_boundary(idx))
}

/// Parse the `TITLE:` / `ABSTRACT:` marker format from the LLM response.
///
/// Tolerant of leading/trailing whitespace and model preamble: scans for the
/// markers case-insensitively and splits on the first occurrence of each. If
/// either marker is missing, returns `None` (the caller treats this as a
/// translation failure).
///
/// A response where either the title OR the abstract section is empty is also
/// treated as a parse failure (`None`), so a malformed response cannot
/// overwrite the article's title or abstract with an empty string.
#[must_use]
pub fn parse_metadata_translation(response: &str) -> Option<TranslatedMetadata> {
    let title_idx = find_marker_ci(response, "TITLE:")?;
    let abstract_idx = find_marker_ci(response, "ABSTRACT:")?;
    if abstract_idx <= title_idx {
        return None;
    }
    let title_start = title_idx + "TITLE:".len();
    let title_raw = response.get(title_start..abstract_idx)?.trim();
    let abstract_start = abstract_idx + "ABSTRACT:".len();
    let abstract_raw = response.get(abstract_start..).unwrap_or("").trim();
    // Strict: either field empty is a parse failure. Prevents overwriting the
    // working title/abstract with an empty string when the model misbehaves.
    if title_raw.is_empty() || abstract_raw.is_empty() {
        return None;
    }
    Some(TranslatedMetadata {
        title: title_raw.to_string(),
        abstract_text: abstract_raw.to_string(),
    })
}

/// Mark a translation job failed: write `translation_status='failed'` +
/// `translation_error` + a `'translation_error'` audit entry. Used by both
/// engine paths (metadata-only + full-text) so the error-handling boilerplate
/// lives in one place. Non-fatal - errors are ignored (the caller already has
/// an error to propagate and the DB write is best-effort bookkeeping).
fn mark_translation_failed(db: &Mutex<rusqlite::Connection>, article_id: &str, err_msg: &str) {
    let Ok(conn) = crate::db::connection::lock_conn(db) else {
        return;
    };
    let _ = article_repo::update_translation_status_failed(&conn, article_id, err_msg);
    let _ = audit_repo::create_entry(
        &conn,
        article_id,
        "translation_error",
        None,
        None,
        Some(&format!("Metadata translation failed: {err_msg}")),
        "ai",
    );
}

/// Translate the metadata (title + abstract) of a non-English article to
/// English and write the result back in a single transaction.
///
/// Implements plan §F steps F.1-F.6 + F.10-F.11 for the metadata-only case.
///
/// The `db: &Mutex<Connection>` is locked in three bursts so the lock is never
/// held across the async LLM `.await` (the `MutexGuard` is `!Send`):
/// 1. Read article + language; mark `running`; persist originals.
/// 2. (lock released) Make the LLM call if the abstract needs translation.
/// 3. Single-transaction write-back: `UPDATE articles` + `translation` audit.
///
/// On LLM error or parse failure: write `translation_status='failed'` +
/// `translation_error` + `translation_error` audit entry, then return Err.
pub async fn translate_metadata_only(
    db: &Mutex<rusqlite::Connection>,
    article_id: &str,
    client: &dyn LlmClient,
) -> Result<(), AppError> {
    // ── Burst 1: read + mark running + persist originals ──
    let (article, source_language, needs_llm_call) = {
        let conn = crate::db::connection::lock_conn(db)?;
        // F.1: read article + language.
        let article = article_repo::get_article_by_id(&conn, article_id)?;
        if article.is_translated {
            // Idempotent: already translated, nothing to do.
            return Ok(());
        }
        // Skip-policy gate: English OR absent/blank language → skip (plan §F.2/§G).
        if should_skip_translation(article.language.as_deref()) {
            return Ok(());
        }
        let source_language = article.language.clone();

        // F.3: mark running.
        article_repo::update_translation_status(&conn, article_id, "running")?;

        // F.4 + F.5: persist originals (metadata-only path: no full text / chunks).
        article_original_repo::insert_original_content(
            &conn,
            article_id,
            Some(&article.title),
            Some(&article.abstract_text),
            None,
            source_language.as_deref(),
        )?;

        // F.6: decide whether an LLM call is needed.
        let needs_llm_call =
            !article.abstract_text.is_empty() && !is_english_abstract(&article.abstract_text);
        (article, source_language, needs_llm_call)
    }; // lock released before the async LLM call.

    // ── Burst 2 (no lock held): the LLM call ──
    let translated = if needs_llm_call {
        match translate_metadata_text(
            client,
            &article.title,
            &article.abstract_text,
            db,
            article_id,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                // mark_translation_failed already ran inside the helper.
                return Err(e);
            }
        }
    } else {
        // No LLM call needed: keep title/abstract as-is (they are either empty
        // or already English). The originals row still records the source text.
        TranslatedMetadata {
            title: article.title.clone(),
            abstract_text: article.abstract_text.clone(),
        }
    };

    // ── Burst 3: single-transaction write-back ──
    let now = chrono::Utc::now().to_rfc3339();
    {
        let conn = crate::db::connection::lock_conn(db)?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE articles SET title = ?1, abstract_text = ?2, is_translated = 1, \
             translation_status = 'succeeded', translation_error = NULL, translated_at = ?3, \
             changed_at = datetime('now') \
          WHERE id = ?4",
            rusqlite::params![translated.title, translated.abstract_text, now, article_id],
        )?;
        let detail = if needs_llm_call {
            format!(
                "Translated metadata from {} to English",
                source_language.as_deref().unwrap_or("unknown")
            )
        } else {
            "Metadata-only translation: no abstract translation required (empty or already English)"
                .to_string()
        };
        // Audit entry inside the same transaction so it rolls back atomically.
        let audit_id = uuid::Uuid::new_v4().to_string();
        let audit_ts = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO audit_entries (id, article_id, timestamp, action, details, source) \
          VALUES (?1, ?2, ?3, 'translation', ?4, 'ai')",
            rusqlite::params![audit_id, article_id, audit_ts, detail],
        )?;
        tx.commit()?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Batched chunk translation (translation-3-plan.md)
// ---------------------------------------------------------------------------
//
// Full-text translation packs multiple chunks into a single LLM call sized to
// the configured context window, then parses the JSON-map response and resends
// any chunks the model skipped or truncated. This mirrors the proven
// `wiki/ingest.rs::build_ingest_prompt_batches` pattern and reduces a 46-chunk
// article from 46 per-chunk calls to ~2-3 batched calls.

/// Fraction of the context window reserved for the input side (system prompt +
/// batched chunks + JSON wrapper). Mirrors the Wiki ingest's conservative
/// split so the model has ample room for output + overhead. The 20% remainder
/// covers the system prompt, JSON keys/escaping, and a safety buffer for
/// response drift.
const INPUT_BUDGET_FRACTION: f64 = 0.4;

/// Hard floor on the input character budget per batch. Ensures tiny context
/// windows (e.g. 4k local models) still make progress.
const MIN_BATCH_INPUT_CHARS: usize = 4_000;

/// Hard cap on the input character budget per batch. Protects against
/// pathological single-call payloads regardless of the configured window.
const MAX_BATCH_INPUT_CHARS: usize = 80_000;

/// Fallback input character budget when the configured context window is
/// unusable (zero / negative). Matches the Wiki ingest's `MAX_SOURCE_CHARS`.
const FALLBACK_BATCH_INPUT_CHARS: usize = 48_000;

/// Maximum resend iterations for missing chunks before the job fails. After
/// this many rounds, any still-missing chunks cause a job failure with a clear
/// audit entry. Prevents infinite loops on a model that persistently ignores
/// the JSON contract.
const MAX_RESEND_ITERATIONS: usize = 2;

/// System prompt for batched chunk translation. Asks for a JSON object mapping
/// each chunk_id to its translated English text. The JSON-lines user prompt
/// (one single-key object per chunk) provides unambiguous delimiters so the
/// model cannot confuse chunk boundaries.
const CHUNK_BATCH_SYSTEM_PROMPT: &str = "\
You are a professional academic translator. Translate each chunk into clear, \
accurate English. Preserve technical terminology, citations, and any markdown \
structure. Return a JSON object mapping each chunk_id to its translated \
English text. Output ONLY the JSON object - no commentary, no markdown fences.";

/// Approximate token count for a chunk of text (1 token ~= 4 chars for Latin
/// scripts). Mirrors the `wiki/ingest.rs::estimate_tokens` heuristic.
#[must_use]
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Compute the input character budget for one batch from the configured context
/// window. Falls back to `FALLBACK_BATCH_INPUT_CHARS` when the configured
/// window is unusable (zero / negative), then clamps to
/// `[MIN_BATCH_INPUT_CHARS, MAX_BATCH_INPUT_CHARS]`.
#[must_use]
fn batch_input_char_budget(context_window_tokens: i32) -> usize {
    if context_window_tokens <= 0 {
        return FALLBACK_BATCH_INPUT_CHARS.clamp(MIN_BATCH_INPUT_CHARS, MAX_BATCH_INPUT_CHARS);
    }
    let tokens = (f64::from(context_window_tokens) * INPUT_BUDGET_FRACTION) as usize;
    let chars = tokens.saturating_mul(4);
    chars.clamp(MIN_BATCH_INPUT_CHARS, MAX_BATCH_INPUT_CHARS)
}

/// A compiled chunk-translation batch: the chunk indices it contains plus the
/// user prompt to send to the LLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkBatch {
    /// 0-based chunk indices (into the source `original_chunks` slice) included
    /// in this batch, in ascending order.
    pub chunk_indices: Vec<usize>,
    /// The full user prompt (JSON-lines of `{"<idx>": "<text>"}`).
    pub prompt: String,
}

/// Greedily pack `chunks` into batches sized to the configured context window.
///
/// Sequential fill in `chunk_index` order: keep adding chunks to the current
/// batch until the next chunk would exceed the input budget, then flush.
/// Guarantees every chunk lands in exactly one batch and the stitched output is
/// in input order. Mirrors `wiki/ingest.rs::build_ingest_prompt_batches`.
///
/// Returns an empty `Vec` when `chunks` is empty.
#[must_use]
pub fn build_chunk_batches(
    chunks: &[crate::utils::chunking::Chunk],
    context_window_tokens: i32,
) -> Vec<ChunkBatch> {
    let all_indices: Vec<usize> = (0..chunks.len()).collect();
    build_chunk_batches_for_indices(chunks, &all_indices, context_window_tokens)
}

/// Greedily pack a SUBSET of chunks (identified by `indices` into the `chunks`
/// slice) into batches, using the ORIGINAL indices in both the prompt keys and
/// the returned `ChunkBatch.chunk_indices`. This is the resend-round helper:
/// the model responds with the original chunk IDs so `translated_slots` fills
/// without remapping.
///
/// Returns an empty `Vec` when `indices` is empty.
#[must_use]
pub fn build_chunk_batches_for_indices(
    chunks: &[crate::utils::chunking::Chunk],
    indices: &[usize],
    context_window_tokens: i32,
) -> Vec<ChunkBatch> {
    if indices.is_empty() {
        return Vec::new();
    }
    let budget = batch_input_char_budget(context_window_tokens);
    // Reserve room for the system prompt + JSON wrapper + per-chunk JSON
    // overhead (keys, braces, escaping). 600 tokens ~ 2400 chars of overhead.
    let overhead_chars = (estimate_tokens(CHUNK_BATCH_SYSTEM_PROMPT) + 600).saturating_mul(4);
    let usable_budget = budget.saturating_sub(overhead_chars).max(MIN_BATCH_INPUT_CHARS);

    let mut batches: Vec<ChunkBatch> = Vec::new();
    let mut current_indices: Vec<usize> = Vec::new();
    let mut current_len: usize = 0;

    for &idx in indices {
        let chunk = &chunks[idx];
        // One JSON line: `{"<idx>": "<escaped-text>"}\n`. Escape quotes +
        // backslashes conservatively (the model parses this back to a string).
        let escaped = escape_json_string(&chunk.text);
        let line_len = escaped.len() + idx.to_string().len() + 8; // `{"": ""}\n` overhead
        if !current_indices.is_empty() && current_len + line_len > usable_budget {
            // Flush current batch.
            let prompt = build_batch_user_prompt(&current_indices, chunks);
            batches
                .push(ChunkBatch { chunk_indices: std::mem::take(&mut current_indices), prompt });
            current_len = 0;
        }
        current_indices.push(idx);
        current_len += line_len;
    }
    if !current_indices.is_empty() {
        let prompt = build_batch_user_prompt(&current_indices, chunks);
        batches.push(ChunkBatch { chunk_indices: current_indices, prompt });
    }
    batches
}

/// Build the JSON-lines user prompt for one batch. Each chunk is a single-key
/// JSON object on its own line; the response is one combined JSON object keyed
/// by chunk_id. Pure helper (no I/O).
fn build_batch_user_prompt(indices: &[usize], chunks: &[crate::utils::chunking::Chunk]) -> String {
    let mut out = String::new();
    out.push_str("Translate the following chunks. Return JSON: {\"<chunk_id>\": \"<english>\", ...}\n\nChunks:\n");
    for &idx in indices {
        let escaped = escape_json_string(&chunks[idx].text);
        out.push_str(&format!("{{\"{idx}\": \"{escaped}\"}}\n"));
    }
    out
}

/// Escape a string for embedding inside a JSON string literal. Escapes
/// backslash, double-quote, and the control chars that most JSON parsers
/// reject (newline, tab, carriage return). Other bytes (including UTF-8) are
/// passed through unchanged - `serde_json` handles UTF-8 fine on the parse
/// side, and we only need the output to be unambiguous to the model.
#[must_use]
fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Parsed batch-translation response: the chunk_ids that were translated,
/// mapped to their English text, plus the chunk_ids the model skipped or
/// returned empty (candidates for the resend round).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchParseResult {
    /// chunk_id -> translated English text (non-empty).
    pub translated: std::collections::HashMap<usize, String>,
    /// chunk_ids that were expected but missing or empty in the response, in
    /// ascending order.
    pub missing: Vec<usize>,
}

/// Parse a batch-translation LLM response into a `BatchParseResult`.
///
/// Tolerant of markdown fences (```` ```json ... ``` ````) and leading/trailing
/// preamble. If the whole-string JSON parse fails, attempts to extract the
/// outermost `{...}` block via regex before giving up. For each expected
/// `chunk_id`, a missing key or an empty-string value is recorded as `missing`
/// so the caller can resend just those chunks.
///
/// A completely malformed response records ALL expected ids as missing (the
/// caller treats this as a full-batch resend, bounded by `MAX_RESEND_ITERATIONS`).
#[must_use]
pub fn parse_batch_translation_response(
    response: &str,
    expected_ids: &[usize],
) -> BatchParseResult {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return BatchParseResult {
            translated: std::collections::HashMap::new(),
            missing: expected_ids.to_vec(),
        };
    }

    // Strip markdown fences if present (models sometimes wrap JSON in
    // ```json ... ```).
    let fence_stripped = strip_markdown_fences(trimmed);

    // Try a direct parse first.
    let parsed_map: Option<std::collections::HashMap<String, String>> =
        serde_json::from_str(&fence_stripped).ok();

    // Fallback: extract the outermost {...} block via regex and retry.
    let parsed_map = parsed_map.or_else(|| {
        let re = regex::Regex::new(r"(?s)\{.*\}").ok()?;
        let captured = re.find(&fence_stripped)?;
        serde_json::from_str::<std::collections::HashMap<String, String>>(captured.as_str()).ok()
    });

    let Some(map) = parsed_map else {
        // Malformed: treat every expected id as missing so the caller resends
        // the whole batch (bounded by MAX_RESEND_ITERATIONS).
        return BatchParseResult {
            translated: std::collections::HashMap::new(),
            missing: expected_ids.to_vec(),
        };
    };

    let mut translated: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let mut missing: Vec<usize> = Vec::new();
    for &id in expected_ids {
        match map.get(&id.to_string()) {
            Some(v) if !v.trim().is_empty() => {
                translated.insert(id, v.trim().to_string());
            }
            _ => missing.push(id),
        }
    }
    BatchParseResult { translated, missing }
}

/// Strip a single pair of markdown code fences (` ```json ... ``` ` or
/// ` ``` ... ``` `) from around a JSON body. Returns the input unchanged when
/// no fences are present. Pure helper.
#[must_use]
fn strip_markdown_fences(s: &str) -> String {
    let trimmed = s.trim();
    if !trimmed.starts_with("```") {
        return s.to_string();
    }
    // Drop the opening fence line (including an optional `json` tag).
    let after_open = match trimmed.find('\n') {
        Some(idx) => &trimmed[idx + 1..],
        None => return s.to_string(),
    };
    // Drop the closing fence if present.
    if let Some(close) = after_open.rfind("```") {
        after_open[..close].to_string()
    } else {
        after_open.to_string()
    }
}

/// Translate the full text (metadata + chunks) of a non-English article to
/// English and write the result back in a single transaction.
///
/// Implements plan §F steps F.1-F.11 for the full-text case + the
/// batched-chunk optimization from translation-3-plan.md.
///
/// Pipeline:
/// 1. Read article + language; gate on English/absent and `is_translated`.
/// 2. Mark `running`; persist originals (title, abstract, full_text, chunks).
/// 3. Translate title + abstract as one metadata LLM call.
/// 4. Pack original chunks into context-window-sized batches and dispatch them
///    concurrently via `futures::future::join_all`. Each batch is one LLM call
///    carrying a JSON-lines payload; the model returns a single JSON object
///    mapping each chunk_id to its English translation.
/// 5. Parse each batch response, fill a slot per chunk_id, and collect any
///    missing ids into a resend round (reusing the same packing/dispatch/parse
///    flow, bounded by `MAX_RESEND_ITERATIONS`). After the cap, any
///    still-missing chunk fails the job with a clear audit detail.
/// 6. Stitch translated chunks into a single English `full_text`, then re-run
///    `classify_sections` + `chunk_sections` over the English text.
/// 7. Single-transaction write-back: `DELETE` + `INSERT` English chunks,
///    `UPDATE articles`, write `translation` audit entry, commit.
///
/// On any LLM error mid-translation: discard the in-memory partial, write
/// `failed` + `translation_error` audit, return Err. No partial rows.
///
/// `context_window_tokens` is plumbed from the caller (the worker reads
/// `LlmConfig.context_window_tokens`). It cannot be read from `client` because
/// the `LlmClient` trait has no config accessor, and widening the trait would
/// pollute `screening::llm_client`.
pub async fn translate_full_text(
    db: &Mutex<rusqlite::Connection>,
    article_id: &str,
    client: &dyn LlmClient,
    context_window_tokens: i32,
) -> Result<(), AppError> {
    use crate::db::chunk_repo;
    use crate::utils::chunking::{chunk_sections, Chunk, DEFAULT_CHUNK_WORDS};
    use crate::utils::sections::classify_sections;

    // ── Burst 1: read + mark running + persist originals ──
    let (article, source_language, original_chunks) = {
        let conn = crate::db::connection::lock_conn(db)?;
        let article = article_repo::get_article_by_id(&conn, article_id)?;
        if article.is_translated {
            return Ok(());
        }
        // Skip-policy gate: English OR absent/blank language → skip (plan §F.2/§G).
        if should_skip_translation(article.language.as_deref()) {
            return Ok(());
        }
        let source_language = article.language.clone();

        article_repo::update_translation_status(&conn, article_id, "running")?;

        // Persist originals: title, abstract, full_text, source_language.
        article_original_repo::insert_original_content(
            &conn,
            article_id,
            Some(&article.title),
            Some(&article.abstract_text),
            article.full_text.as_deref(),
            source_language.as_deref(),
        )?;

        // Persist original chunks. If none are stored, derive them from the
        // original full text so per-chunk translation has input to work with.
        let mut chunks = chunk_repo::list_chunks_for_article(&conn, article_id)?;
        if chunks.is_empty() {
            if let Some(ft) = article.full_text.as_deref() {
                let sections = classify_sections(ft);
                chunks = chunk_sections(&sections, DEFAULT_CHUNK_WORDS);
            }
        }
        if !chunks.is_empty() {
            article_original_repo::replace_original_chunks(&conn, article_id, &chunks)?;
        }
        (article, source_language, chunks)
    }; // lock released.

    // ── Burst 2 (no lock): metadata + batched chunk LLM calls ──
    // Metadata call (title + abstract).
    let translated_metadata =
        translate_metadata_text(client, &article.title, &article.abstract_text, db, article_id)
            .await?;

    // Per-chunk translation, now BATCHED. Slots are pre-sized by index so a
    // truncated batch response can be detected and resent without losing the
    // chunk's place in the stitched output.
    let total_chunks = original_chunks.len();
    let mut translated_slots: Vec<Option<String>> = vec![None; total_chunks];

    // Initial round: pack ALL chunks into batches, dispatch concurrently.
    let mut pending_indices: Vec<usize> = (0..total_chunks).collect();
    let mut iterations = 0usize;
    let mut first_error: Option<AppError> = None;

    while !pending_indices.is_empty() {
        if iterations > MAX_RESEND_ITERATIONS {
            // Cap reached: any still-missing chunks fail the job so no silent
            // gaps reach the stitched `full_text`.
            let missing_count = pending_indices.len();
            let sample: Vec<String> =
                pending_indices.iter().take(5).map(ToString::to_string).collect();
            let err_msg = format!(
                "Full-text translation failed: {missing_count} chunk(s) still missing after \
                 {MAX_RESEND_ITERATIONS} resend rounds (chunk ids: {})",
                sample.join(", ")
            );
            mark_translation_failed_with_detail(
                db,
                article_id,
                &err_msg,
                &format!("{err_msg}. The model may have truncated the batch response."),
            );
            return Err(AppError::Import(err_msg));
        }

        let batches = build_chunk_batches_for_indices(
            &original_chunks,
            &pending_indices,
            context_window_tokens,
        );
        // No batches means no pending chunks (empty-pending short-circuit
        // above). Defensive: avoid an infinite loop if packing produced
        // nothing usable.
        if batches.is_empty() {
            break;
        }

        eprintln!(
            "[translation] job_id={article_id} iteration={iterations} dispatching {} batch(es) \
             for {} chunk(s)",
            batches.len(),
            pending_indices.len()
        );

        // Dispatch all batches in this round concurrently. Each `client.send()`
        // routes through the LlmOrchestrator's semaphore, so real parallelism
        // is bounded by `max_concurrent_requests` from LlmConfig.
        let batch_futures = batches.iter().map(|batch| {
            let batch_indices = batch.chunk_indices.clone();
            async move {
                client
                    .send(CHUNK_BATCH_SYSTEM_PROMPT, &batch.prompt)
                    .await
                    .map(|(resp, _tokens)| (batch_indices, resp))
            }
        });
        let batch_results = futures::future::join_all(batch_futures).await;

        // Parse each batch response, fill slots, collect the next round's
        // missing indices.
        let mut still_missing: Vec<usize> = Vec::new();
        for result in batch_results {
            match result {
                Err(e) => {
                    // An LLM error on a batch fails the whole job (mirrors the
                    // previous fail-on-first-error semantics). Record the
                    // first error; remaining batches in this round complete
                    // harmlessly (bounded by the orchestrator).
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                    continue;
                }
                Ok((batch_indices, resp)) => {
                    let parsed = parse_batch_translation_response(&resp, &batch_indices);
                    for (id, text) in &parsed.translated {
                        if let Some(slot) = translated_slots.get_mut(*id) {
                            *slot = Some(text.clone());
                        }
                    }
                    still_missing.extend(parsed.missing.iter().copied().filter(|id| {
                        // Only keep missing ids that belong to this batch (a
                        // malformed-response fallback reports ALL expected ids
                        // as missing; parse_batch_translation_response already
                        // scoped them to `batch_indices`).
                        batch_indices.contains(id)
                    }));
                }
            }
        }

        if let Some(e) = first_error.take() {
            let err_msg = e.to_string();
            let detail = format!("Full-text batch translation failed: {err_msg}");
            mark_translation_failed_with_detail(db, article_id, &err_msg, &detail);
            return Err(e);
        }

        // Next round retranslates ONLY the still-missing chunks, using their
        // original chunk_ids so the slots fill correctly.
        pending_indices = still_missing;
        iterations += 1;
    }

    // Flatten slots into ordered strings. Any leftover `None` is a bug (the
    // loop above either fills every slot or returns Err on cap exhaustion),
    // but defensively fail rather than write a partial stitched text.
    let mut translated_chunks: Vec<String> = Vec::with_capacity(total_chunks);
    for (idx, slot) in translated_slots.iter().enumerate() {
        match slot {
            Some(t) => translated_chunks.push(t.clone()),
            None => {
                let err_msg = format!(
                    "Full-text translation failed: chunk {idx} is unexpectedly untranslated"
                );
                mark_translation_failed_with_detail(db, article_id, &err_msg, &err_msg);
                return Err(AppError::Import(err_msg));
            }
        }
    }

    // Stitch + re-chunk the English text.
    let english_full_text = translated_chunks.join("\n\n");
    let english_sections = classify_sections(&english_full_text);
    let rechunked: Vec<Chunk> = chunk_sections(&english_sections, DEFAULT_CHUNK_WORDS);

    // ── Burst 3: single-transaction write-back ──
    let now = chrono::Utc::now().to_rfc3339();
    {
        let conn = crate::db::connection::lock_conn(db)?;
        let tx = conn.unchecked_transaction()?;
        // Delete + insert English chunks inside the transaction.
        tx.execute(
            "DELETE FROM article_chunks WHERE article_id = ?1",
            rusqlite::params![article_id],
        )?;
        for chunk in &rechunked {
            tx.execute(
                "INSERT INTO article_chunks (article_id, chunk_index, section, content, word_count) \
                  VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    article_id,
                    chunk.chunk_index as i64,
                    chunk.section,
                    chunk.text,
                    chunk.word_count as i64,
                ],
            )?;
        }
        tx.execute(
            "UPDATE articles SET title = ?1, abstract_text = ?2, full_text = ?3, is_translated = 1, \
             translation_status = 'succeeded', translation_error = NULL, translated_at = ?4, \
             changed_at = datetime('now') \
          WHERE id = ?5",
            rusqlite::params![
                translated_metadata.title,
                translated_metadata.abstract_text,
                english_full_text,
                now,
                article_id
            ],
        )?;
        let detail = format!(
            "Translated full text from {} to English ({} chunks re-chunked)",
            source_language.as_deref().unwrap_or("unknown"),
            rechunked.len()
        );
        let audit_id = uuid::Uuid::new_v4().to_string();
        let audit_ts = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO audit_entries (id, article_id, timestamp, action, details, source) \
          VALUES (?1, ?2, ?3, 'translation', ?4, 'ai')",
            rusqlite::params![audit_id, article_id, audit_ts, detail],
        )?;
        tx.commit()?;
    }

    Ok(())
}

/// Mark a translation job failed with a custom audit detail message (used by
/// the full-text per-chunk error path so the audit entry identifies which
/// chunk failed). Same DB writes as [`mark_translation_failed`].
fn mark_translation_failed_with_detail(
    db: &Mutex<rusqlite::Connection>,
    article_id: &str,
    err_msg: &str,
    detail: &str,
) {
    let Ok(conn) = crate::db::connection::lock_conn(db) else {
        return;
    };
    let _ = article_repo::update_translation_status_failed(&conn, article_id, err_msg);
    let _ = audit_repo::create_entry(
        &conn,
        article_id,
        "translation_error",
        None,
        None,
        Some(detail),
        "ai",
    );
}

/// Helper: translate the metadata (title + abstract) text via a single LLM
/// call. Returns the parsed `TranslatedMetadata`, or marks the job failed and
/// returns `Err` on LLM error / parse failure.
///
/// Shared by both `translate_metadata_only` (metadata-only path) and
/// `translate_full_text` (full-text path) so the LLM-call + parse + error
/// handling logic lives in exactly one place.
async fn translate_metadata_text(
    client: &dyn LlmClient,
    title: &str,
    abstract_text: &str,
    db: &Mutex<rusqlite::Connection>,
    article_id: &str,
) -> Result<TranslatedMetadata, AppError> {
    let needs_llm_call = !abstract_text.is_empty() && !is_english_abstract(abstract_text);
    if !needs_llm_call {
        return Ok(TranslatedMetadata {
            title: title.to_string(),
            abstract_text: abstract_text.to_string(),
        });
    }
    let user_prompt = format!("TITLE:\n{title}\n\nABSTRACT:\n{abstract_text}");
    let response = match client.send(METADATA_TRANSLATION_SYSTEM_PROMPT, &user_prompt).await {
        Ok((text, _tokens)) => text,
        Err(e) => {
            let err_msg = e.to_string();
            mark_translation_failed(db, article_id, &err_msg);
            return Err(e);
        }
    };
    match parse_metadata_translation(&response) {
        Some(t) => Ok(t),
        None => {
            let err_msg = "Failed to parse translation response (missing TITLE:/ABSTRACT: markers)";
            mark_translation_failed(db, article_id, err_msg);
            Err(AppError::Import(err_msg.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_response() {
        let resp = "TITLE:\nOn the Origin of Species\n\nABSTRACT:\nThis paper discusses evolution.";
        let parsed = parse_metadata_translation(resp).expect("parses");
        assert_eq!(parsed.title, "On the Origin of Species");
        assert_eq!(parsed.abstract_text, "This paper discusses evolution.");
    }

    #[test]
    fn parse_tolerates_whitespace_and_preamble() {
        let resp = "Here is the translation:\n\nTITLE:\n  A Title  \n\nABSTRACT:\n  An abstract.  ";
        let parsed = parse_metadata_translation(resp).expect("parses");
        assert_eq!(parsed.title, "A Title");
        assert_eq!(parsed.abstract_text, "An abstract.");
    }

    #[test]
    fn parse_lowercase_markers() {
        let resp = "title:\nFoo\n\nabstract:\nBar";
        let parsed = parse_metadata_translation(resp).expect("parses");
        assert_eq!(parsed.title, "Foo");
        assert_eq!(parsed.abstract_text, "Bar");
    }

    #[test]
    fn parse_returns_none_when_markers_missing() {
        assert!(parse_metadata_translation("just some text").is_none());
    }

    #[test]
    fn parse_returns_none_when_abstract_before_title() {
        let resp = "ABSTRACT:\nfoo\n\nTITLE:\nbar";
        assert!(parse_metadata_translation(resp).is_none());
    }

    #[test]
    fn parse_returns_none_when_title_empty() {
        // Strict: an empty title must be a parse failure, not an empty-string
        // overwrite of the working article title.
        let resp = "TITLE:\n\nABSTRACT:\nSome text";
        assert!(parse_metadata_translation(resp).is_none());
    }

    #[test]
    fn parse_returns_none_when_abstract_empty() {
        // Strict: an empty abstract must be a parse failure, not an empty-string
        // overwrite of the working article abstract.
        let resp = "TITLE:\nSome title\n\nABSTRACT:\n";
        assert!(parse_metadata_translation(resp).is_none());
    }

    #[test]
    fn parse_handles_unicode_preamble_before_markers() {
        // Regression: a preamble containing characters whose `to_uppercase()`
        // form has a different byte length (e.g. the `ﬁ` ligature, U+FB01,
        // which expands to `FI`) must NOT shift the marker index and break the
        // slice. The case-insensitive search runs on the original string so
        // indices stay byte-stable.
        let resp = "Voici la traduction ﬁnale:\n\nTITLE:\nA Title\n\nABSTRACT:\nAn abstract.";
        let parsed = parse_metadata_translation(resp).expect("parses despite Unicode preamble");
        assert_eq!(parsed.title, "A Title");
        assert_eq!(parsed.abstract_text, "An abstract.");
    }

    // ── Batched chunk translation helpers (translation-3-plan.md) ──

    fn make_chunk(idx: usize, text: &str) -> crate::utils::chunking::Chunk {
        crate::utils::chunking::Chunk {
            section: Some("Methods".to_string()),
            chunk_index: idx,
            text: text.to_string(),
            word_count: text.split_whitespace().count(),
        }
    }

    #[test]
    fn build_chunk_batches_single_batch_when_small() {
        // Three tiny chunks + a generous context window → one batch containing
        // all three, in order.
        let chunks = vec![make_chunk(0, "alpha"), make_chunk(1, "beta"), make_chunk(2, "gamma")];
        let batches = build_chunk_batches(&chunks, 50_000);
        assert_eq!(batches.len(), 1, "small input packs into one batch");
        assert_eq!(batches[0].chunk_indices, vec![0, 1, 2]);
        // The prompt must reference each chunk by its id.
        assert!(batches[0].prompt.contains("\"0\""));
        assert!(batches[0].prompt.contains("\"1\""));
        assert!(batches[0].prompt.contains("\"2\""));
    }

    #[test]
    fn build_chunk_batches_splits_when_large() {
        // A tiny context window forces a split once the budget is exceeded.
        let chunks: Vec<_> =
            (0..10).map(|i| make_chunk(i, &"chunk text padding ".repeat(200))).collect();
        let batches = build_chunk_batches(&chunks, 4_000);
        assert!(
            batches.len() > 1,
            "expected multiple batches for a large input with a tiny window, got {}",
            batches.len()
        );
    }

    #[test]
    fn build_chunk_batches_preserves_input_order() {
        // Chunks must appear in ascending-index order across all batches, and
        // within each batch.
        let chunks: Vec<_> = (0..6).map(|i| make_chunk(i, &"chunk ".repeat(150))).collect();
        let batches = build_chunk_batches(&chunks, 4_000);
        let mut all_indices: Vec<usize> = Vec::new();
        for batch in &batches {
            // Within-batch ascending order.
            for w in batch.chunk_indices.windows(2) {
                assert!(w[0] < w[1], "batch indices must be ascending: {:?}", batch.chunk_indices);
            }
            all_indices.extend(batch.chunk_indices.iter().copied());
        }
        assert_eq!(all_indices, vec![0, 1, 2, 3, 4, 5], "global order must be input order");
    }

    #[test]
    fn build_chunk_batches_every_chunk_exactly_once() {
        // Every chunk index must land in exactly one batch (no skips, no dups).
        let chunks: Vec<_> =
            (0..8).map(|i| make_chunk(i, &format!("chunk {i} {}", " ".repeat(120)))).collect();
        let batches = build_chunk_batches(&chunks, 4_000);
        let mut all_indices: Vec<usize> =
            batches.iter().flat_map(|b| b.chunk_indices.iter().copied()).collect();
        all_indices.sort_unstable();
        assert_eq!(all_indices, vec![0, 1, 2, 3, 4, 5, 6, 7], "every chunk exactly once");
    }

    #[test]
    fn build_chunk_batches_respects_floor_and_cap() {
        // Non-positive window → fallback (clamped to [MIN, MAX]).
        let budget = batch_input_char_budget(0);
        assert!(
            (MIN_BATCH_INPUT_CHARS..=MAX_BATCH_INPUT_CHARS).contains(&budget),
            "fallback budget must be clamped, got {budget}"
        );
        // Negative window → same fallback.
        let budget_neg = batch_input_char_budget(-1);
        assert_eq!(budget, budget_neg, "negative window matches zero window fallback");
        // Huge window → clamped to MAX_BATCH_INPUT_CHARS.
        let budget_huge = batch_input_char_budget(10_000_000);
        assert_eq!(budget_huge, MAX_BATCH_INPUT_CHARS, "huge window is clamped to the cap");
        // Tiny positive window → clamped to MIN_BATCH_INPUT_CHARS.
        let budget_tiny = batch_input_char_budget(1);
        assert_eq!(budget_tiny, MIN_BATCH_INPUT_CHARS, "tiny window is clamped to the floor");
    }

    #[test]
    fn build_chunk_batches_for_indices_uses_original_ids() {
        // Resend-round helper: a subset must keep the ORIGINAL chunk ids in the
        // prompt keys + the returned `chunk_indices`.
        let chunks = vec![
            make_chunk(0, "zero"),
            make_chunk(1, "one"),
            make_chunk(2, "two"),
            make_chunk(3, "three"),
            make_chunk(4, "four"),
        ];
        let batches = build_chunk_batches_for_indices(&chunks, &[1, 3], 50_000);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].chunk_indices, vec![1, 3]);
        assert!(batches[0].prompt.contains("\"1\""));
        assert!(batches[0].prompt.contains("\"3\""));
        // The other chunk ids must NOT appear (resend only includes the missing subset).
        assert!(!batches[0].prompt.contains("\"0\""));
        assert!(!batches[0].prompt.contains("\"2\""));
    }

    #[test]
    fn parse_batch_translation_response_happy_path() {
        let resp = r#"{"0": "Hello", "1": "World"}"#;
        let parsed = parse_batch_translation_response(resp, &[0, 1]);
        assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Hello"));
        assert_eq!(parsed.translated.get(&1).map(String::as_str), Some("World"));
        assert!(parsed.missing.is_empty());
    }

    #[test]
    fn parse_batch_translation_response_missing_keys() {
        // The model returned only chunk 0; chunk 1 is missing.
        let resp = r#"{"0": "Hello"}"#;
        let parsed = parse_batch_translation_response(resp, &[0, 1]);
        assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Hello"));
        assert_eq!(parsed.missing, vec![1]);
    }

    #[test]
    fn parse_batch_translation_response_empty_values_marked_missing() {
        // Empty-string values are treated as missing so the caller resends them.
        let resp = r#"{"0": "Hello", "1": "   "}"#;
        let parsed = parse_batch_translation_response(resp, &[0, 1]);
        assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Hello"));
        assert_eq!(parsed.missing, vec![1], "whitespace-only values are missing");
    }

    #[test]
    fn parse_batch_translation_response_strips_markdown_fences() {
        let resp = "```json\n{\"0\": \"Fenced\"}\n```";
        let parsed = parse_batch_translation_response(resp, &[0]);
        assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Fenced"));
        assert!(parsed.missing.is_empty());
    }

    #[test]
    fn parse_batch_translation_response_malformed_falls_back_to_all_missing() {
        // Completely unparseable response → every expected id is missing.
        let parsed = parse_batch_translation_response("not json at all", &[0, 1, 2]);
        assert!(parsed.translated.is_empty());
        assert_eq!(parsed.missing, vec![0, 1, 2]);
    }

    #[test]
    fn parse_batch_translation_response_regex_fallback_extracts_embedded_json() {
        // Model wraps JSON in preamble + postamble; the regex fallback should
        // still extract the {...} block.
        let resp = "Here is the translation:\n{\"0\": \"Extracted\"}\nDone.";
        let parsed = parse_batch_translation_response(resp, &[0]);
        assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Extracted"));
    }
}
