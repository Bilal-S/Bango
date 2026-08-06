#![allow(clippy::expect_used)]
//! 5-phase pipeline scanning the Bango Documents directory for files produced
//! by external tools and importing them into the article database by DOI match:
//! 1. Full Text (`fulltext/`): attach `{normalized_doi}.pdf/.txt`. Skips
//!    articles that already have full text.
//! 2. Citations (`ris/`): import `{normalized_doi}_references.ris` /
//!    `_citations.ris` / `.bib` files. Skips articles with existing details.
//! 3. Translations: enqueue `FullText` jobs for non-English articles. Gated on
//!    `app_settings.auto_translate` (default false, opt-in).
//! 4. AI Summaries: generate via `generate_article_ai_summary_inner` for
//!    newly-attached articles without a summary. Gated on
//!    `bango-full-text-summaries` localStorage flag. Runs after translations.
//! 5. Embeddings: embed `included` corpus so semantic search has a candidate
//!    pool. Idempotent via director staleness check. Gated on LLM configured +
//!    embeddings not disabled.
//!
//! All phases run in one spawned background task. Emits
//! `batch-import:progress` per-item; cancel via [`cancel_batch_import`].
//!       In-flight LLM requests complete naturally.
//!
//! # Lock scope (Concern 3)
//!
//! Phases 1-2 are `async` with short DB lock bursts (discovery + per-article
//! write). CPU-bound PDF parse runs on `spawn_blocking` with no lock held via
//! `attach_full_text_split`. See `full_text_phase.rs` for the per-article
//! lock contract.
use crate::db::app_settings_repo;
use crate::db::article_repo::{self, ArticleDoiInfo};
use crate::db::connection::DbState;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

pub mod citations_phase;
pub mod embeddings_phase;
pub mod full_text_phase;
pub mod summary_phase;
pub mod translations_phase;

/// DOI match map shared across phases (avoids circular deps).
pub struct DoiMatchMap(pub HashMap<String, ArticleDoiInfo>);

impl DoiMatchMap {
    /// Look up an article by its cleaned-DOI key.
    pub fn get(&self, key: &str) -> Option<&ArticleDoiInfo> {
        self.0.get(key)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether the map contains a key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
}

/// Per-phase execution result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportPhaseResult {
    /// Total items discovered before cancellation.
    pub total: usize,
    /// Items attempted (may be < total if cancelled).
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    /// Non-fatal error messages.
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Pipeline phases (1-indexed for display).
///
/// Variants 1-5 are work phases. `Complete` (6) is a terminal indicator used
/// only for the final 100% snapshot emitted after all work phases finish; it
/// renders the phase label "Batch Import" so the user sees an unambiguous
/// end state instead of the just-finished phase label.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BatchImportPhase {
    FullText = 1,
    Citations = 2,
    Translations = 3,
    Summaries = 4,
    Embeddings = 5,
    /// Terminal "all phases done" indicator, not a work phase.
    Complete = 6,
}

impl BatchImportPhase {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::FullText => "Full Text",
            Self::Citations => "Citations",
            Self::Translations => "Translations",
            Self::Summaries => "AI Summaries",
            Self::Embeddings => "Embeddings",
            Self::Complete => "Batch Import",
        }
    }
}

/// Progress payload emitted via `batch-import:progress` and returned by
/// `get_batch_import_progress`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportProgress {
    /// 1-based.
    pub phase: usize,
    pub phase_name: String,
    /// Items completed in the current phase.
    pub completed: usize,
    /// Total items in the current phase.
    pub total: usize,
    /// Overall percentage (0-100) across all phases.
    pub overall_percent: usize,
    /// Human-readable status (e.g. filename being processed).
    #[serde(default)]
    pub message: String,
    pub is_running: bool,
    pub is_cancelled: bool,
    /// Per-phase results (populated as each phase completes).
    #[serde(default)]
    pub full_text: Option<BatchImportPhaseResult>,
    #[serde(default)]
    pub citations: Option<BatchImportPhaseResult>,
    #[serde(default)]
    pub translations: Option<BatchImportPhaseResult>,
    #[serde(default)]
    pub summaries: Option<BatchImportPhaseResult>,
}

/// Managed state: cancel token + progress snapshot. Uses `std::sync::Mutex`
/// so per-item progress callbacks fire synchronously without `.await`.
pub struct BatchImportState {
    cancel_token: Arc<Mutex<bool>>,
    progress: Arc<Mutex<BatchImportProgress>>,
}

impl Default for BatchImportState {
    fn default() -> Self {
        Self {
            cancel_token: Arc::new(Mutex::new(false)),
            progress: Arc::new(Mutex::new(BatchImportProgress::default())),
        }
    }
}

impl BatchImportState {
    /// Cloned cancel token for background tasks.
    pub fn cancel_handle(&self) -> Arc<Mutex<bool>> {
        Arc::clone(&self.cancel_token)
    }

    /// Cloned progress handle for background tasks.
    pub fn progress_handle(&self) -> Arc<Mutex<BatchImportProgress>> {
        Arc::clone(&self.progress)
    }
}

/// Compute 0-100% avoiding division by zero.
#[allow(clippy::manual_checked_ops)]
fn percent(processed: usize, total: usize) -> usize {
    if total == 0 {
        100
    } else {
        (processed * 100) / total
    }
}

/// Emit progress via event system + shared struct (sync, so callable from
/// phase loops via on_progress callback). `app_handle.emit()` is non-blocking.
#[allow(clippy::too_many_arguments)]
fn emit_progress(
    app_handle: &tauri::AppHandle,
    progress: &Arc<Mutex<BatchImportProgress>>,
    phase: BatchImportPhase,
    completed: usize,
    total: usize,
    overall_percent: usize,
    message: &str,
    is_running: bool,
    is_cancelled: bool,
    full_text: Option<BatchImportPhaseResult>,
    citations: Option<BatchImportPhaseResult>,
    translations: Option<BatchImportPhaseResult>,
    summaries: Option<BatchImportPhaseResult>,
) {
    let payload = BatchImportProgress {
        phase: phase as usize,
        phase_name: phase.name().to_string(),
        completed,
        total,
        overall_percent,
        message: message.to_string(),
        is_running,
        is_cancelled,
        full_text: full_text.clone(),
        citations: citations.clone(),
        translations: translations.clone(),
        summaries: summaries.clone(),
    };
    {
        let mut guard = progress.lock().expect("batch import mutex");
        guard.phase = payload.phase;
        guard.phase_name = payload.phase_name.clone();
        guard.completed = payload.completed;
        guard.total = payload.total;
        guard.overall_percent = payload.overall_percent;
        guard.message = payload.message.clone();
        guard.is_running = payload.is_running;
        guard.is_cancelled = payload.is_cancelled;
        if full_text.is_some() {
            guard.full_text = full_text;
        }
        if citations.is_some() {
            guard.citations = citations;
        }
        if translations.is_some() {
            guard.translations = translations;
        }
        if summaries.is_some() {
            guard.summaries = summaries;
        }
    }
    let _ = app_handle.emit("batch-import:progress", payload);
}

/// Start the batch import pipeline. Returns immediately after spawning the
/// background task.
///
/// * `auto_summarize` - when true, Phase 4 runs.
/// * `include_section_summaries` - forwarded to summary generator (mirrors
///   `bango-section-summaries` localStorage flag).
#[tauri::command]
pub async fn start_batch_import(
    app_handle: tauri::AppHandle,
    _db_state: State<'_, DbState>,
    batch_state: State<'_, BatchImportState>,
    auto_summarize: Option<bool>,
    include_section_summaries: Option<bool>,
) -> Result<BatchImportProgress, AppError> {
    // ── Concurrent-start guard ──
    {
        let guard = batch_state.progress.lock().expect("batch import mutex");
        if guard.is_running {
            return Ok(guard.clone());
        }
    }

    // Reset cancel token + progress snapshot.
    let cancel_handle = batch_state.cancel_handle();
    {
        let mut token = cancel_handle.lock().expect("batch import mutex");
        *token = false;
        let mut prog = batch_state.progress.lock().expect("batch import mutex");
        *prog = BatchImportProgress::default();
    }

    let auto_sum = auto_summarize.unwrap_or(false);
    let include_sections = include_section_summaries.unwrap_or(false);
    let progress = batch_state.progress_handle();
    let app_handle_clone = app_handle.clone();
    let cancel_for_task = Arc::clone(&cancel_handle);

    // Emit initial state.
    emit_progress(
        &app_handle,
        &progress,
        BatchImportPhase::FullText,
        0,
        0,
        0,
        "Starting batch import...",
        true,
        false,
        None,
        None,
        None,
        None,
    );

    // Spawn the background task.
    tokio::spawn(async move {
        let db = app_handle_clone.state::<DbState>();

        // Read the auto_translate setting once (Phase 3 gate). Default false
        // (opt-in); Phase 3 is skipped unless the user enabled it in Settings.
        let auto_translate = {
            let conn = match db.conn.lock() {
                Ok(c) => c,
                Err(_) => {
                    emit_progress(
                        &app_handle_clone,
                        &progress,
                        BatchImportPhase::FullText,
                        0,
                        0,
                        0,
                        "DB lock error",
                        false,
                        false,
                        None,
                        None,
                        None,
                        None,
                    );
                    return;
                }
            };
            app_settings_repo::get_auto_translate(&conn).unwrap_or(false)
        };

        // ═══════════════════════════════════════════════════════════════════
        //  Phase 1: Full Text (async, short DB lock bursts - Concern 3)
        // ═══════════════════════════════════════════════════════════════════
        /* DB mutex held only for brief initial discovery + per-article
        write burst. CPU-bound PDF parse runs on `spawn_blocking` with
        no lock held via `attach_full_text_split` (Concern 3 fix). */
        let p1_cancel = Arc::clone(&cancel_for_task);
        let p1_progress = Arc::clone(&progress);
        let p1_app = app_handle_clone.clone();
        let is_cancelled = move || {
            let guard = p1_cancel.try_lock();
            matches!(guard, Ok(g) if *g)
        };
        let mut on_progress = move |processed: usize, total: usize, msg: &str| {
            let overall = percent(processed, total) / 4;
            emit_progress(
                &p1_app,
                &p1_progress,
                BatchImportPhase::FullText,
                processed,
                total,
                overall,
                msg,
                true,
                false,
                None,
                None,
                None,
                None,
            );
        };
        let phase1_outcome =
            full_text_phase::run_full_text_phase(&db.conn, &is_cancelled, &mut on_progress).await;
        let (ft_result, newly_attached, ft_lock_err) = match phase1_outcome {
            Ok((result, ids)) => (result, ids, None),
            Err(e) => (
                BatchImportPhaseResult {
                    total: 0,
                    processed: 0,
                    succeeded: 0,
                    failed: 0,
                    errors: vec![format!("Phase 1 error: {e}")],
                },
                vec![],
                Some(format!("Phase 1 error: {e}")),
            ),
        };

        // Handle deferred lock error.
        if let Some(msg) = ft_lock_err {
            emit_progress(
                &app_handle_clone,
                &progress,
                BatchImportPhase::FullText,
                0,
                0,
                0,
                &msg,
                false,
                false,
                None,
                None,
                None,
                None,
            );
            return;
        }

        // Phase 1 complete summary.
        let ft_percent = percent(ft_result.processed, ft_result.total);
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::FullText,
            ft_result.processed,
            ft_result.total,
            ft_percent / 4,
            &format!(
                "Phase 1 (Full Text): {} attached, {} failed",
                ft_result.succeeded, ft_result.failed
            ),
            true,
            false,
            Some(ft_result.clone()),
            None,
            None,
            None,
        );

        // Check cancel after phase 1.
        if let Ok(g) = cancel_for_task.try_lock() {
            if *g {
                emit_progress(
                    &app_handle_clone,
                    &progress,
                    BatchImportPhase::FullText,
                    ft_result.processed,
                    ft_result.total,
                    ft_percent / 4,
                    "Cancelled by user",
                    false,
                    true,
                    Some(ft_result),
                    None,
                    None,
                    None,
                );
                return;
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        //  Phase 2: Citations (async, short DB lock bursts - Concern 3)
        // ═══════════════════════════════════════════════════════════════════
        let p2_cancel = Arc::clone(&cancel_for_task);
        let p2_progress = Arc::clone(&progress);
        let p2_app = app_handle_clone.clone();
        let p2_ft = ft_result.clone();
        let is_cancelled = move || {
            let guard = p2_cancel.try_lock();
            matches!(guard, Ok(g) if *g)
        };
        let mut on_progress = move |processed: usize, total: usize, msg: &str| {
            let overall = 25 + percent(processed, total) / 4;
            emit_progress(
                &p2_app,
                &p2_progress,
                BatchImportPhase::Citations,
                processed,
                total,
                overall,
                msg,
                true,
                false,
                Some(p2_ft.clone()),
                None,
                None,
                None,
            );
        };
        let cit_result =
            match citations_phase::run_citations_phase(&db.conn, &is_cancelled, &mut on_progress)
                .await
            {
                Ok(result) => result,
                Err(e) => BatchImportPhaseResult {
                    total: 0,
                    processed: 0,
                    succeeded: 0,
                    failed: 0,
                    errors: vec![format!("Phase 2 error: {e}")],
                },
            };

        // Phase 2 complete summary.
        let cit_percent = percent(cit_result.processed, cit_result.total);
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::Citations,
            cit_result.processed,
            cit_result.total,
            25 + cit_percent / 4,
            &format!(
                "Phase 2 (Citations): {} imported, {} failed",
                cit_result.succeeded, cit_result.failed
            ),
            true,
            false,
            Some(ft_result.clone()),
            Some(cit_result.clone()),
            None,
            None,
        );

        // Check cancel after phase 2.
        if let Ok(g) = cancel_for_task.try_lock() {
            if *g {
                emit_progress(
                    &app_handle_clone,
                    &progress,
                    BatchImportPhase::Citations,
                    cit_result.processed,
                    cit_result.total,
                    25 + cit_percent / 4,
                    "Cancelled by user",
                    false,
                    true,
                    Some(ft_result),
                    Some(cit_result),
                    None,
                    None,
                );
                return;
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        //  Phase 3: Translations
        // ═══════════════════════════════════════════════════════════════════
        let trn_result = if auto_translate && !newly_attached.is_empty() {
            // Per-item progress callback for Phase 3.
            let prog_snap = Arc::clone(&progress);
            let app_snap = app_handle_clone.clone();
            let ft_snap = ft_result.clone();
            let cit_snap = cit_result.clone();
            let mut on_progress = move |processed: usize, total: usize, msg: &str| {
                let overall = 50 + percent(processed, total) / 4;
                emit_progress(
                    &app_snap,
                    &prog_snap,
                    BatchImportPhase::Translations,
                    processed,
                    total,
                    overall,
                    msg,
                    true,
                    false,
                    Some(ft_snap.clone()),
                    Some(cit_snap.clone()),
                    None,
                    None,
                );
            };

            let cancel_snap = Arc::clone(&cancel_for_task);
            translations_phase::run_translations_phase(
                &app_handle_clone,
                &db,
                newly_attached.clone(),
                &mut on_progress,
                move || {
                    let snap = Arc::clone(&cancel_snap);
                    async move { *snap.lock().expect("batch import mutex") }
                },
            )
            .await
        } else {
            BatchImportPhaseResult {
                errors: vec![
                    "Phase 3 skipped (auto-translate disabled or no new articles)".to_string()
                ],
                ..Default::default()
            }
        };

        // Phase 3 complete summary.
        let trn_percent = percent(trn_result.processed, trn_result.total);
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::Translations,
            trn_result.processed,
            trn_result.total,
            50 + trn_percent / 4,
            &format!(
                "Phase 3 (Translations): {} translated, {} failed",
                trn_result.succeeded, trn_result.failed
            ),
            true,
            false,
            Some(ft_result.clone()),
            Some(cit_result.clone()),
            Some(trn_result.clone()),
            None,
        );

        // Check cancel after phase 3.
        if let Ok(g) = cancel_for_task.try_lock() {
            if *g {
                emit_progress(
                    &app_handle_clone,
                    &progress,
                    BatchImportPhase::Translations,
                    trn_result.processed,
                    trn_result.total,
                    50 + trn_percent / 4,
                    "Cancelled by user",
                    false,
                    true,
                    Some(ft_result),
                    Some(cit_result),
                    Some(trn_result),
                    None,
                );
                return;
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        //  Phase 4: AI Summaries (optional, parallel - Concern 1)
        // ═══════════════════════════════════════════════════════════════════
        let sum_result = if auto_sum {
            let to_summarize: Vec<String> = {
                let conn = match db.conn.lock() {
                    Ok(c) => c,
                    Err(_) => {
                        return;
                    }
                };
                match article_repo::get_articles_with_doi_info(&conn) {
                    Ok(articles) => {
                        let id_set: std::collections::HashSet<&str> =
                            newly_attached.iter().map(String::as_str).collect();
                        articles
                            .into_iter()
                            .filter(|a| id_set.contains(a.id.as_str()) && !a.has_ai_summary)
                            .map(|a| a.id)
                            .collect::<Vec<_>>()
                    }
                    Err(_) => vec![],
                }
            };

            // Per-item progress callback for Phase 4.
            let prog_snap = Arc::clone(&progress);
            let app_snap = app_handle_clone.clone();
            let ft_snap = ft_result.clone();
            let cit_snap = cit_result.clone();
            let trn_snap = trn_result.clone();
            let mut on_progress = move |processed: usize, total: usize, msg: &str| {
                let overall = 75 + percent(processed, total) / 4;
                emit_progress(
                    &app_snap,
                    &prog_snap,
                    BatchImportPhase::Summaries,
                    processed,
                    total,
                    overall,
                    msg,
                    true,
                    false,
                    Some(ft_snap.clone()),
                    Some(cit_snap.clone()),
                    Some(trn_snap.clone()),
                    None,
                );
            };

            let cancel_snap = Arc::clone(&cancel_for_task);
            summary_phase::run_summary_phase(
                &db,
                &app_handle_clone,
                to_summarize,
                include_sections,
                &mut on_progress,
                move || {
                    let snap = Arc::clone(&cancel_snap);
                    async move { *snap.lock().expect("batch import mutex") }
                },
            )
            .await
        } else {
            BatchImportPhaseResult {
                errors: vec!["Phase 4 skipped (auto-summarize disabled)".to_string()],
                ..Default::default()
            }
        };

        // Phase 4 complete summary.
        let sum_percent = percent(sum_result.processed, sum_result.total);
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::Summaries,
            sum_result.processed,
            sum_result.total,
            75 + sum_percent / 4,
            &format!(
                "Phase 4 (AI Summaries): {} summarized, {} failed",
                sum_result.succeeded, sum_result.failed
            ),
            true,
            false,
            Some(ft_result.clone()),
            Some(cit_result.clone()),
            Some(trn_result.clone()),
            Some(sum_result.clone()),
        );

        // ═══════════════════════════════════════════════════════════════════
        //  Phase 5: Embeddings (always runs - idempotent via director staleness)
        // ═══════════════════════════════════════════════════════════════════
        /* Embeds `included` corpus for semantic search. Idempotent via
        director `input_hash` staleness check; gated on LLM configured +
        embeddings not disabled. */
        let emb_result = embeddings_phase::run_embeddings_phase(
            &db,
            &app_handle_clone,
            Arc::clone(&cancel_for_task),
        )
        .await;

        // Phase 5 complete summary.
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::Embeddings,
            emb_result.processed,
            emb_result.total,
            80,
            &format!(
                "Phase 5 (Embeddings): {} generated, {} skipped, {} failed",
                emb_result.succeeded,
                emb_result.processed.saturating_sub(emb_result.succeeded),
                emb_result.failed
            ),
            true,
            false,
            Some(ft_result.clone()),
            Some(cit_result.clone()),
            Some(trn_result.clone()),
            Some(sum_result.clone()),
        );

        // Final state. Uses the `Complete` variant so the phase label renders
        // "Batch Import" (not the just-finished "Embeddings") at 100%.
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::Complete,
            emb_result.processed,
            emb_result.total,
            100,
            "Batch import complete",
            false,
            false,
            Some(ft_result),
            Some(cit_result),
            Some(trn_result),
            Some(sum_result),
        );
    });

    let guard = batch_state.progress.lock().expect("batch import mutex");
    Ok(guard.clone())
}

/// Cancel a running batch import. Checked between items; in-flight LLM
/// requests complete naturally. `expect()` on the cancel-token mutex is a
/// poisoned-mutex panic point.
#[allow(clippy::expect_used)]
#[tauri::command]
pub async fn cancel_batch_import(batch_state: State<'_, BatchImportState>) -> Result<(), AppError> {
    let handle = batch_state.cancel_handle();
    *handle.lock().expect("batch import mutex") = true;
    Ok(())
}

/// Current progress snapshot (for polling or initial load). `expect()` on the
/// progress mutex is a poisoned-mutex panic point.
#[allow(clippy::expect_used)]
#[tauri::command]
pub async fn get_batch_import_progress(
    batch_state: State<'_, BatchImportState>,
) -> Result<BatchImportProgress, AppError> {
    let guard = batch_state.progress.lock().expect("batch import mutex");
    Ok(guard.clone())
}
