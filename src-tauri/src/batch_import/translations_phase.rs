//! Phase 3: Enqueue `FullText` translation jobs for non-English
//! newly-attached articles and wait per article. Non-fatal: errors recorded,
//! Phase 4 proceeds with whatever text exists.

use std::time::Duration;

use tauri::{Manager, State};

use crate::commands::translation::enqueue_article_translation_inner;
use crate::db::article_repo;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::translation::language::should_skip_translation;
use crate::translation::worker::TranslationJobKind;

use super::BatchImportPhaseResult;

/// Per-article translation-wait timeout (5 min).
const TRANSLATION_WAIT_TIMEOUT_SECS: u64 = 5 * 60;

/// Poll interval (2s).
const TRANSLATION_POLL_INTERVAL_MS: u64 = 2_000;

/// Skip message when LLM not configured. Mirrors Phase 4.
pub const LLM_NOT_CONFIGURED_SKIP_MSG: &str = "Skipped: LLM not configured";

/// Pre-flight LLM config check for Phase 3. Pure I/O over `&Connection` for
/// unit-testability. Returns `Some(skip_result)` (with audit record) when
/// LLM not configured, `None` when it is.
pub fn check_llm_configured_or_skip(
    conn: &rusqlite::Connection,
    total: usize,
) -> Option<BatchImportPhaseResult> {
    if llm_config_repo::has_config(conn).unwrap_or(false) {
        return None;
    }
    // Write a system-level audit record so the skip is visible in Diagnostics
    // and the Notification History, with an actionable explanation.
    let audit_detail = "Batch import Phase 3 (Translations) skipped: LLM not \
         configured. Configure an LLM provider in Settings to use auto-translate.";
    let _ = audit_repo::log_error(conn, audit_detail);
    Some(BatchImportPhaseResult {
        total,
        processed: 0,
        succeeded: 0,
        failed: 0,
        errors: vec![LLM_NOT_CONFIGURED_SKIP_MSG.to_string()],
    })
}

/// Run Phase 3: enqueue translations for non-English newly-attached articles
/// and wait for each. Filters by `articles.language`, `is_translated`, and
/// `auto_translate`.
pub async fn run_translations_phase<F, Fut, P>(
    app_handle: &tauri::AppHandle,
    db_state: &State<'_, DbState>,
    article_ids: Vec<String>,
    on_progress: &mut P,
    is_cancelled: F,
) -> BatchImportPhaseResult
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
    P: FnMut(usize, usize, &str),
{
    let total = article_ids.len();
    let mut processed = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<String> = Vec::new();

    /* Pre-flight: skip when LLM not configured (avoids N per-article
    failures). Must run before worker-handle resolution so CI/test
    environments short-circuit cleanly. Mirrors Phase 4. */
    {
        let conn = match db_state.conn.lock() {
            Ok(c) => c,
            Err(e) => {
                return BatchImportPhaseResult {
                    total,
                    processed: 0,
                    succeeded: 0,
                    failed: total,
                    errors: vec![format!("Failed to acquire DB lock to check LLM config: {e}")],
                };
            }
        };
        if let Some(skip) = check_llm_configured_or_skip(&conn, total) {
            return skip;
        }
    }

    // Resolve the translation worker handle (needed for enqueue). If it's not
    // registered (e.g. in a test harness), skip the phase gracefully.
    let worker = match app_handle.try_state::<crate::translation::worker::TranslationWorkerHandle>()
    {
        Some(w) => w,
        None => {
            return BatchImportPhaseResult {
                total,
                processed: 0,
                succeeded: 0,
                failed: 0,
                errors: vec!["Translation worker not available".to_string()],
            };
        }
    };

    for article_id in &article_ids {
        if is_cancelled().await {
            break;
        }

        on_progress(
            processed,
            total,
            &format!(
                "Phase 3 - Translations - checking {total} articles - translating {} of {total}",
                processed + 1
            ),
        );

        processed += 1;

        // Pre-enqueue status read. Not passed to `wait_for_translation`:
        // enqueuing mutates DB status. `wait_for_translation` always polls live.
        let (language, is_translated) = {
            let conn = match db_state.conn.lock() {
                Ok(c) => c,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("DB lock error for {article_id}: {e}"));
                    continue;
                }
            };
            let article = match article_repo::get_article_by_id(&conn, article_id) {
                Ok(a) => a,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("Failed to read article {article_id}: {e}"));
                    continue;
                }
            };
            (article.language, article.is_translated)
        };

        // Skip-policy gate: English OR absent/blank language → no translation
        // needed (plan §F.2/§G).
        if should_skip_translation(language.as_deref()) {
            succeeded += 1; // counts as "nothing to do = success"
            continue;
        }
        // Skip already-translated articles.
        if is_translated {
            succeeded += 1;
            continue;
        }

        // Enqueue via the standard gate (writes 'queued' + sends). The gate is
        // idempotent - if the article is already queued/running, it skips.
        let enqueued = {
            let conn = match db_state.conn.lock() {
                Ok(c) => c,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("DB lock error for {article_id}: {e}"));
                    continue;
                }
            };
            match enqueue_article_translation_inner(
                &conn,
                worker.inner(),
                article_id,
                TranslationJobKind::FullText,
                false,
            ) {
                Ok(b) => b,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("Failed to enqueue translation for {article_id}: {e}"));
                    continue;
                }
            }
        };

        if !enqueued {
            // Already queued/running/succeeded - fall through to the wait loop
            // so we still block until it completes.
        }

        // Wait per article until translation_status leaves 'running'/'queued'.
        // Always polls the live DB status (no stale entry-status shortcut).
        let wait_result = wait_for_translation(
            db_state,
            article_id,
            Duration::from_secs(TRANSLATION_WAIT_TIMEOUT_SECS),
            Duration::from_millis(TRANSLATION_POLL_INTERVAL_MS),
        )
        .await;

        match wait_result {
            WaitOutcome::Succeeded => succeeded += 1,
            WaitOutcome::Timeout => {
                failed += 1;
                errors.push(format!(
                    "Translation timed out for {article_id} after {TRANSLATION_WAIT_TIMEOUT_SECS}s; proceeding with current text"
                ));
            }
            WaitOutcome::Failed(msg) => {
                failed += 1;
                errors.push(format!("Translation failed for {article_id}: {msg}"));
            }
            WaitOutcome::Error(e) => {
                failed += 1;
                errors.push(format!("Error waiting for translation {article_id}: {e}"));
            }
        }
    }

    BatchImportPhaseResult { total, processed, succeeded, failed, errors }
}

/// Outcome of `wait_for_translation`.
#[derive(Debug)]
enum WaitOutcome {
    Succeeded,
    Timeout,
    Failed(String),
    Error(String),
}

/// Poll `translation_status` until it leaves `'running'`/`'queued'`, up to
/// `timeout`. Always reads live DB status.
/// - `Succeeded`: `'succeeded'` or `is_translated`
/// - `Failed`: `'failed'`
/// - `Timeout`: still `'running'`/`'queued'` after timeout
async fn wait_for_translation(
    db_state: &State<'_, DbState>,
    article_id: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> WaitOutcome {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if std::time::Instant::now() >= deadline {
            return WaitOutcome::Timeout;
        }
        tokio::time::sleep(poll_interval).await;

        let (status, is_translated, error_msg) = {
            let conn = match db_state.conn.lock() {
                Ok(c) => c,
                Err(e) => return WaitOutcome::Error(e.to_string()),
            };
            let info = match article_repo::get_translation_status(&conn, article_id) {
                Ok(i) => i,
                Err(e) => return WaitOutcome::Error(e.to_string()),
            };
            (
                info.translation_status,
                info.is_translated,
                info.translation_error.unwrap_or_default(),
            )
        };

        if is_translated || status == "succeeded" {
            return WaitOutcome::Succeeded;
        }
        if status == "failed" {
            return WaitOutcome::Failed(if error_msg.is_empty() {
                "translation failed".to_string()
            } else {
                error_msg
            });
        }
        // status == "running" || "queued" -> keep polling.
    }
}

/// Public per-article wait helper (for summary-phase gating and tests).
/// Polls live status; resolves once `translation_status` leaves `'running'`.
pub async fn wait_for_translation_if_needed(
    db_state: &State<'_, DbState>,
    article_id: &str,
) -> Result<(), String> {
    // Always poll the live status; no stale entry-status shortcut.
    let outcome = wait_for_translation(
        db_state,
        article_id,
        Duration::from_secs(TRANSLATION_WAIT_TIMEOUT_SECS),
        Duration::from_millis(TRANSLATION_POLL_INTERVAL_MS),
    )
    .await;
    match outcome {
        WaitOutcome::Succeeded => Ok(()),
        WaitOutcome::Timeout => Err("translation timed out".to_string()),
        WaitOutcome::Failed(msg) => Err(msg),
        WaitOutcome::Error(e) => Err(e),
    }
}
