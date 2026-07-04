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
/// (article UUID) and `part_id` (chunk index; 0 for metadata-only) before each
/// delegated orchestrator call, and routes through
/// `send_with_type(LlmRequestType::Translation)`. The `LlmClient` trait and
/// `send_with_type` default method are not widened (per plan §LLM Orchestrator
/// and Translation Client).
pub struct TranslationLlmClient {
    pub config: LlmConfig,
    pub orchestrator: Arc<LlmOrchestrator>,
    pub job_id: String,
}

#[async_trait::async_trait]
impl LlmClient for TranslationLlmClient {
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError> {
        eprintln!(
            "[translation] job_id={} part_id=0 (metadata-only) delegating via orchestrator",
            self.job_id
        );
        self.orchestrator.send(&self.config, system, user, LlmRequestType::Translation).await
    }

    async fn send_with_type(
        &self,
        system: &str,
        user: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        eprintln!(
            "[translation] job_id={} part_id=0 (metadata-only) delegating via orchestrator ({:?})",
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
    let Ok(conn) = lock_db(db) else {
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
        let conn = lock_db(db)?;
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
        let conn = lock_db(db)?;
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

/// System prompt for per-chunk translation. Asks for a clean English
/// translation with no commentary so the stitched output is directly
/// re-chunkable.
const CHUNK_TRANSLATION_SYSTEM_PROMPT: &str = "\
You are a professional academic translator. Translate the given text into \
clear, accurate English. Preserve technical terminology, citations, and any \
markdown structure. Output ONLY the translated English text with no \
commentary, headings, or formatting markers.";

/// Translate the full text (metadata + chunks) of a non-English article to
/// English and write the result back in a single transaction.
///
/// Implements plan §F steps F.1-F.11 for the full-text case.
///
/// Pipeline:
/// 1. Read article + language; gate on English/absent and `is_translated`.
/// 2. Mark `running`; persist originals (title, abstract, full_text, chunks).
/// 3. Translate title + abstract as one metadata LLM call (reuses the
///    `METADATA_TRANSLATION_SYSTEM_PROMPT` + `parse_metadata_translation`).
/// 4. Translate each original chunk via a per-chunk LLM call
///    (`CHUNK_TRANSLATION_SYSTEM_PROMPT`). Each call logs `job_id`/`part_id`.
/// 5. Stitch translated chunks into a single English `full_text`.
/// 6. Re-run `classify_sections` + `chunk_sections` over the English text.
/// 7. Single-transaction write-back: `DELETE` + `INSERT` English chunks,
///    `UPDATE articles` (title/abstract/full_text/is_translated/status/audit.
///
/// On any LLM error mid-translation: discard the in-memory partial, write
/// `failed` + `translation_error` audit, return Err. No partial rows.
pub async fn translate_full_text(
    db: &Mutex<rusqlite::Connection>,
    article_id: &str,
    client: &dyn LlmClient,
) -> Result<(), AppError> {
    use crate::db::chunk_repo;
    use crate::utils::chunking::{chunk_sections, Chunk, DEFAULT_CHUNK_WORDS};
    use crate::utils::sections::classify_sections;

    // ── Burst 1: read + mark running + persist originals ──
    let (article, source_language, original_chunks) = {
        let conn = lock_db(db)?;
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

    // ── Burst 2 (no lock): metadata + per-chunk LLM calls ──
    // Metadata call (title + abstract).
    let translated_metadata =
        translate_metadata_text(client, &article.title, &article.abstract_text, db, article_id)
            .await?;

    // Per-chunk translation. Accumulate English chunks in memory; discard on
    // any error so no partial state is written.
    let mut translated_chunks: Vec<String> = Vec::with_capacity(original_chunks.len());
    for (idx, chunk) in original_chunks.iter().enumerate() {
        eprintln!(
            "[translation] job_id={article_id} part_id={idx} translating chunk ({} words)",
            chunk.word_count
        );
        let (resp, _tokens) = match client.send(CHUNK_TRANSLATION_SYSTEM_PROMPT, &chunk.text).await
        {
            Ok(v) => v,
            Err(e) => {
                let err_msg = e.to_string();
                let detail = format!("Full-text chunk {idx} translation failed: {err_msg}");
                mark_translation_failed_with_detail(db, article_id, &err_msg, &detail);
                return Err(e);
            }
        };
        let trimmed = resp.trim();
        if trimmed.is_empty() {
            // Skip empty translations rather than failing the whole job.
            continue;
        }
        translated_chunks.push(trimmed.to_string());
    }

    // Stitch + re-chunk the English text.
    let english_full_text = translated_chunks.join("\n\n");
    let english_sections = classify_sections(&english_full_text);
    let rechunked: Vec<Chunk> = chunk_sections(&english_sections, DEFAULT_CHUNK_WORDS);

    // ── Burst 3: single-transaction write-back ──
    let now = chrono::Utc::now().to_rfc3339();
    {
        let conn = lock_db(db)?;
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
    let Ok(conn) = lock_db(db) else {
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

/// Lock the DB mutex. Maps the poison error to an `AppError::Database` so the
/// engine surfaces lock failures uniformly. The guard is released when it
/// drops; the engine never holds it across an `.await`.
fn lock_db(
    db: &Mutex<rusqlite::Connection>,
) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, AppError> {
    db.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))
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
}
