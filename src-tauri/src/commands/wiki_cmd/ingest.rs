//! Wiki ingest / rebuild / export-and-ingest pipeline + batch builder.
//!
//! Extracted from the pre-split `wiki_cmd.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.
//!
//! ## Cancel-token contract (v2)
//!
//! All three entry points (`wiki_ingest`, `wiki_rebuild`, `wiki_export_and_ingest`)
//! snapshot a fresh `Arc<AtomicBool>` into the managed [`super::WikiIngestState`]
//! at start and clear it on return (success, error, or cancel). The frontend's
//! [`cancel_wiki_ingest`] command signals the active token; the pipeline checks
//! [`super::is_cancelled`] between each pre-seed step (in
//! `build_batches_with_manifest`) and between LLM batch completions (in
//! `run_chunked_ingest`). On cancel, the pipeline returns `Ok(report)` with
//! `report.errors.push("Cancelled")` - there is no `Cancelled` error variant
//! (mirrors the screening engine's `Ok(true)`/`Ok(false)` convention).

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::{ingest, raw_export, storage};

use super::{
    emit_wiki_progress, ensure_initialized, is_cancelled, log_wiki_ingest_warnings,
    WikiIngestState, WIKI_PIPELINE_TOTAL_STEPS,
};

/// Progress callback for the `build_batches_with_manifest` pre-seed phases.
///
/// Mirrors `ChunkProgressCb` in `commands/full_text.rs`. The callback receives
/// `(step_pct, message)` so the caller can emit a `wiki:progress` event in the
/// 15-25% range (the gap between "Raw sources prepared" and "Generating wiki
/// pages via LLM..."). The callback is `Option` so tests can pass `None`.
pub(super) type WikiPrepProgressCb<'a> = Option<&'a dyn Fn(usize, &str)>;

/// Run the LLM wiki ingest: build prompt batches from raw sources, dispatch
/// them to the LLM in parallel (bounded by the orchestrator's concurrency
/// limit), write the generated pages, rebuild FTS5, and clear staleness.
#[tauri::command]
pub async fn wiki_ingest(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: tauri::AppHandle,
    wiki_state: tauri::State<'_, WikiIngestState>,
) -> Result<ingest::IngestReport, AppError> {
    let cancel = Arc::new(AtomicBool::new(false));
    wiki_state.set_active(Arc::clone(&cancel));
    let result = wiki_ingest_inner(&db_state, &orchestrator, &app_handle, Some(&cancel)).await;
    wiki_state.clear_active();
    result
}

/// Inner implementation of `wiki_ingest` (without the managed-state wrapper).
async fn wiki_ingest_inner(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<ingest::IngestReport, AppError> {
    // Pre-flight: check whether the LLM is configured. When it is not, the
    // deterministic pre-seed layers (author pages, synthesis, concept hubs,
    // method hubs, source pages) still run and are indexed - only the LLM
    // batch dispatch is skipped. This prevents misleading 401 Unauthorized
    // errors from the orchestrator while still giving the user a functional
    // wiki backbone.
    let skip_llm = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let has_llm = crate::db::llm_config_repo::has_config(&conn)?;
        if !has_llm {
            let _ = crate::db::audit_repo::log_error(
                &conn,
                "Wiki ingest: LLM not configured - deterministic pre-seed \
                 pages will be written, LLM synthesis skipped. Configure an \
                 LLM provider in Settings.",
            );
        }
        !has_llm
    };

    let (root, config) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let root = storage::resolve_root(&conn)?;
        let config = crate::db::llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        raw_export::process_user_files(&root)?;
        // Self-heal: ensure AGENTS.md exists so the wiki-view UI does not gate
        // the generated pages behind the "Initialize" empty-state.
        let _ = ensure_initialized(&root);
        (root, config)
    };

    // Build batches (with author manifest if multi-batch) inside a DB scope.
    let prep_cb: WikiPrepProgressCb<'_> = Some(&|step, msg| {
        emit_wiki_progress(app_handle, step, msg);
    });
    let mut pre_seed_pages = 0usize;
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(
            &mut conn,
            &root,
            &config,
            cancel,
            prep_cb,
            &mut pre_seed_pages,
        )?
    };
    if is_cancelled(cancel) {
        let mut report = ingest::IngestReport::default();
        report.errors.push("Cancelled".to_string());
        emit_wiki_progress(app_handle, WIKI_PIPELINE_TOTAL_STEPS, "Cancelled");
        return Ok(report);
    }

    // LLM batch dispatch - skipped when unconfigured. The pre-seed pages are
    // already on disk; finalize_ingest will FTS5-index them regardless.
    let mut report = if skip_llm {
        let mut r = ingest::IngestReport { pages_written: pre_seed_pages, ..Default::default() };
        r.errors.push(
            "LLM not configured: deterministic pre-seed pages written, \
             LLM synthesis skipped."
                .to_string(),
        );
        emit_wiki_progress(app_handle, 50, "LLM not configured, skipping synthesis");
        r
    } else {
        let sender: Arc<dyn ingest::IngestLlmSender> =
            Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
        ingest::run_chunked_ingest(&root, batches, sender, Some(app_handle), (25, 95), cancel)
            .await?
    };

    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    ingest::finalize_ingest(&conn, &root, &mut report)?;
    // Surface non-fatal warnings (ungrounded pages, batch failures) in
    // Settings > Diagnostics so the user can see what went wrong.
    log_wiki_ingest_warnings(&conn, &report);
    Ok(report)
}

/// Build ingest batches with the deterministic pre-seed foundation:
///
/// 1. Run the full 8-step bibliometric normalization so `biblio_authors`
///    (with metrics), `biblio_terms`, and `biblio_article_terms` are populated.
/// 2. **Pre-seed author pages** (`wiki/authors/`) from `biblio_authors`.
/// 3. **Pre-seed synthesis pages** (`wiki/synthesis/`) from each included
///    article's `full_text_ai_summary` JSON - no LLM call needed.
/// 4. **Pre-seed concept hubs** (`wiki/concepts/`) from the top-N terms in
///    `biblio_terms` - no LLM call needed.
/// 5. Inject the author manifest into every batch prompt (so the LLM links to
///    canonical author slugs instead of inventing its own).
///
/// This deterministic foundation runs unconditionally (both single-batch and
/// multi-batch runs), guaranteeing author + synthesis + concept pages exist
/// regardless of which LLM model is used or how many batches the corpus splits
/// into. The LLM's role becomes cross-cutting thematic synthesis only.
///
/// Reviewed (user-edited) pages of each type are preserved by the pre-seeders.
///
/// ## Cancel-token + progress contract (v2)
///
/// `cancel` is checked between each of the 7 pre-seed steps; on cancel the
/// function returns `Ok(Vec::new())` (empty batches = no LLM calls) so the
/// caller can emit a "Cancelled" progress event. `prep_cb` fires at each
/// step with a `(step_pct, message)` tuple in the 15-25% range so the
/// frontend progress bar advances past 15% with a meaningful phase label
/// instead of freezing silently.
fn build_batches_with_manifest(
    conn: &mut rusqlite::Connection,
    root: &std::path::Path,
    config: &crate::models::llm_config::LlmConfig,
    cancel: Option<&Arc<AtomicBool>>,
    prep_cb: WikiPrepProgressCb<'_>,
    pre_seed_pages: &mut usize,
) -> Result<Vec<ingest::IngestBatch>, AppError> {
    // Run the full 8-step bibliometric normalization pipeline so
    // `biblio_authors` (with metrics), `biblio_terms`, `biblio_article_terms`,
    // and the co-author/citation networks are all populated. This is the
    // single source of truth for all three pre-seed layers.
    //
    // Skip when the biblio tables are already fresh: `run_full_normalization`
    // is O(n^2) on article count (keyword co-occurrence, citation networks) and
    // already runs on the Bibliometrics dashboard entry via
    // `useBibliometrics.runNormalization`. Re-running it here when the tables
    // are fresh is the single biggest freeze source in the 15-25% gap.
    //
    // Non-fatal: if normalization fails (e.g. empty corpus), we still proceed
    // so the LLM can operate on the raw sources alone. The pre-seeders will
    // simply find no rows and write nothing. The error is logged to stderr via
    // the always-on `[wiki:diag]` channel (mirrors `[screening:diag]`).
    if crate::db::app_settings_repo::get_biblio_needs_refresh(conn)? {
        eprintln!("[wiki:diag] phase=preparing:normalization (running)");
        if let Some(cb) = prep_cb {
            cb(15, "Normalizing bibliometrics...");
        }
        if let Err(e) = crate::db::biblio_repo::run_full_normalization(conn) {
            eprintln!("[wiki:diag] normalization error (non-fatal): {e}");
        }
    } else {
        eprintln!("[wiki:diag] phase=preparing:normalization (skipped: biblio fresh)");
        if let Some(cb) = prep_cb {
            cb(15, "Bibliometrics already fresh");
        }
    }
    if is_cancelled(cancel) {
        eprintln!("[wiki:diag] cancel detected after normalization");
        return Ok(Vec::new());
    }

    // Phase 1: Pre-seed author pages from `biblio_authors`.
    eprintln!("[wiki:diag] phase=preparing:authors");
    if let Some(cb) = prep_cb {
        cb(16, "Preparing author pages...");
    }
    let manifest = ingest::build_author_manifest(conn)?;
    if !manifest.entries.is_empty() {
        // Errors are non-fatal: the LLM can still produce author pages itself,
        // and the consolidation pass will dedup them.
        *pre_seed_pages += ingest::preseed_authors(root, &manifest).unwrap_or(0);
    }
    if is_cancelled(cancel) {
        eprintln!("[wiki:diag] cancel detected after authors");
        return Ok(Vec::new());
    }

    // Phase 2: Pre-seed synthesis pages from AI summaries.
    // Each included article with a `full_text_ai_summary` gets a synthesis page
    // whose slug = the article UUID (so [[uuid]] links resolve automatically).
    eprintln!("[wiki:diag] phase=preparing:synthesis");
    if let Some(cb) = prep_cb {
        cb(17, "Preparing synthesis pages...");
    }
    *pre_seed_pages += ingest::preseed_synthesis_from_ai_summaries(conn, root).unwrap_or(0);
    if is_cancelled(cancel) {
        eprintln!("[wiki:diag] cancel detected after synthesis");
        return Ok(Vec::new());
    }

    // Phase 3: Pre-seed concept hubs from `biblio_terms`.
    // Caps at 25 terms so the concept layer stays curated + high-signal.
    eprintln!("[wiki:diag] phase=preparing:concepts");
    if let Some(cb) = prep_cb {
        cb(18, "Preparing concept hubs...");
    }
    *pre_seed_pages += ingest::preseed_concept_hubs(conn, root, 25).unwrap_or(0);
    if is_cancelled(cancel) {
        eprintln!("[wiki:diag] cancel detected after concepts");
        return Ok(Vec::new());
    }

    // Phase 4: Pre-seed method hubs from AI-summary `study_design` (when
    // present) with a `biblio_terms` fallback for abstracts-only corpora.
    // Caps at 25 so the methods layer stays curated + high-signal. Uses a
    // curated study-design lexicon so non-methodological terms are filtered.
    eprintln!("[wiki:diag] phase=preparing:methods");
    if let Some(cb) = prep_cb {
        cb(19, "Preparing method hubs...");
    }
    let methods_written = ingest::preseed_methods(conn, root, 25).unwrap_or(0);
    *pre_seed_pages += methods_written;
    if is_cancelled(cancel) {
        eprintln!("[wiki:diag] cancel detected after methods");
        return Ok(Vec::new());
    }

    // Layer 1 (External Documents): Pre-seed source pages for user-uploaded
    // documents (Add Documents). Each external doc in `raw/` with a
    // `source_kind: user_*` gets a first-class wiki node at
    // `wiki/sources/{slug}.md` so `[[user-slug]]` wikilinks and
    // `[^art-user-slug]` footnote refs resolve to a navigable page.
    eprintln!("[wiki:diag] phase=preparing:sources");
    if let Some(cb) = prep_cb {
        cb(20, "Preparing source pages...");
    }
    *pre_seed_pages += ingest::preseed_document_source_pages(root).unwrap_or(0);
    if is_cancelled(cancel) {
        eprintln!("[wiki:diag] cancel detected after sources");
        return Ok(Vec::new());
    }

    // Rebuild batches with the manifest injected (when non-empty). The
    // manifest's `to_prompt_section()` directive tells the LLM NOT to create
    // author pages and to link to the canonical slugs instead.
    eprintln!("[wiki:diag] phase=preparing:batches");
    if let Some(cb) = prep_cb {
        cb(21, "Building LLM batches...");
    }
    let methods_pre_seeded = methods_written > 0;
    if manifest.entries.is_empty() {
        ingest::build_ingest_prompt_batches(
            root,
            config.context_window_tokens,
            None,
            methods_pre_seeded,
        )
    } else {
        ingest::build_ingest_prompt_batches(
            root,
            config.context_window_tokens,
            Some(&manifest),
            methods_pre_seeded,
        )
    }
}

/// Full rebuild: scaffold (if needed) + export included articles + process user files
/// + LLM ingest + FTS5 rebuild. Emits `wiki:progress` at each step.
/// This is the one-click "Re-scaffold" action.
#[tauri::command]
pub async fn wiki_rebuild(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: tauri::AppHandle,
    wiki_state: tauri::State<'_, WikiIngestState>,
) -> Result<ingest::IngestReport, AppError> {
    let cancel = Arc::new(AtomicBool::new(false));
    wiki_state.set_active(Arc::clone(&cancel));
    let result = wiki_rebuild_inner(&db_state, &orchestrator, &app_handle, Some(&cancel)).await;
    if let Err(ref e) = result {
        // Route through the canonical error logger (action = 'error', in the
        // audit_entries CHECK allowlist). The old `log_wiki_error` used
        // action = 'wiki_ingest_error' which is NOT in the CHECK constraint,
        // so SQLite silently rejected every insert and wiki errors never
        // reached Settings > Diagnostics.
        crate::db::audit_repo::log_error_best_effort(&db_state.conn, &e.to_string());
        emit_wiki_progress(&app_handle, WIKI_PIPELINE_TOTAL_STEPS, &format!("Error: {}", e));
    }
    wiki_state.clear_active();
    result
}

/// Inner implementation of wiki_rebuild (without error logging wrapper).
async fn wiki_rebuild_inner(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<ingest::IngestReport, AppError> {
    // Pre-flight: check whether the LLM is configured (mirrors wiki_ingest_inner).
    // When not, deterministic pre-seed still runs; only LLM batches are skipped.
    let skip_llm = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let has_llm = crate::db::llm_config_repo::has_config(&conn)?;
        if !has_llm {
            let _ = crate::db::audit_repo::log_error(
                &conn,
                "Wiki rebuild: LLM not configured - deterministic pre-seed \
                 pages will be written, LLM synthesis skipped. Configure an \
                 LLM provider in Settings.",
            );
        }
        !has_llm
    };

    emit_wiki_progress(app_handle, 0, "Starting wiki rebuild...");

    // Step 0: Scaffold (ensure wiki-root exists) + self-heal AGENTS.md.
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let root = storage::resolve_root(&conn)?;
        storage::scaffold_tree(&root)?;
        let _ = ensure_initialized(&root);
    }

    // Step 1: Lock briefly to load articles + config, then release so the
    // CPU-bound extraction runs lock-free. Per-article progress events fire
    // in the 10-15% range so the user sees "Exporting article N of M..." instead
    // of a silent 0% freeze. Cancel is checked before each article.
    let (root, articles, config) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let root = storage::resolve_root(&conn)?;
        let articles = raw_export::load_included_articles(&conn)?;
        let config = crate::db::llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        (root, articles, config)
    };
    emit_wiki_progress(app_handle, 10, "Wiki directory ready");

    {
        let total = articles.len();
        let article_report = raw_export::write_article_exports(
            &root,
            &articles,
            Some(&|i, _total, _article_id| {
                let step = 10usize.saturating_add((i + 1).saturating_mul(5) / total.max(1));
                emit_wiki_progress(
                    app_handle,
                    step,
                    &format!("Exporting article {} of {}", i + 1, total),
                );
            }),
            cancel,
        )?;
        if article_report.cancelled {
            let mut report = ingest::IngestReport::default();
            report.errors.push("Cancelled".to_string());
            emit_wiki_progress(app_handle, WIKI_PIPELINE_TOTAL_STEPS, "Cancelled");
            return Ok(report);
        }
        raw_export::process_user_files(&root)?;
    }
    emit_wiki_progress(app_handle, 15, "Raw sources prepared");

    // Step 2: Build prompt batches + dispatch them to the LLM in parallel.
    // Each batch carries the full source index, so batches are independent and
    // safe to run concurrently. Progress emits as each batch completes. When
    // the corpus splits into multiple batches, the author manifest + pre-seed
    // optimization is applied to prevent cross-batch duplication.
    let prep_cb: WikiPrepProgressCb<'_> = Some(&|step, msg| {
        emit_wiki_progress(app_handle, step, msg);
    });
    let mut pre_seed_pages = 0usize;
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(
            &mut conn,
            &root,
            &config,
            cancel,
            prep_cb,
            &mut pre_seed_pages,
        )?
    };
    if is_cancelled(cancel) {
        let mut report = ingest::IngestReport::default();
        report.errors.push("Cancelled".to_string());
        emit_wiki_progress(app_handle, WIKI_PIPELINE_TOTAL_STEPS, "Cancelled");
        return Ok(report);
    }
    // LLM batch dispatch - skipped when unconfigured. The pre-seed pages are
    // already on disk; finalize_ingest will FTS5-index them regardless.
    let mut report = if skip_llm {
        let mut r = ingest::IngestReport { pages_written: pre_seed_pages, ..Default::default() };
        r.errors.push(
            "LLM not configured: deterministic pre-seed pages written, \
             LLM synthesis skipped."
                .to_string(),
        );
        emit_wiki_progress(app_handle, 50, "LLM not configured, skipping synthesis");
        r
    } else {
        let sender: Arc<dyn ingest::IngestLlmSender> =
            Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
        emit_wiki_progress(app_handle, 25, "Generating wiki pages via LLM...");
        ingest::run_chunked_ingest(&root, batches, sender, Some(app_handle), (25, 95), cancel)
            .await?
    };

    // Step 3: Finalize (FTS5 rebuild + log + clear staleness).
    emit_wiki_progress(app_handle, 95, "Indexing pages...");
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    ingest::finalize_ingest(&conn, &root, &mut report)?;
    // Surface non-fatal warnings (ungrounded pages, batch failures) in
    // Settings > Diagnostics so the user can see what went wrong.
    log_wiki_ingest_warnings(&conn, &report);

    emit_wiki_progress(app_handle, 100, &format!("Done: {} pages written", report.pages_written));
    Ok(report)
}

/// Export raw + ingest in one call (used after "Add Documents").
/// Emits `wiki:progress` at each step.
#[tauri::command]
pub async fn wiki_export_and_ingest(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: tauri::AppHandle,
    wiki_state: tauri::State<'_, WikiIngestState>,
) -> Result<ingest::IngestReport, AppError> {
    let cancel = Arc::new(AtomicBool::new(false));
    wiki_state.set_active(Arc::clone(&cancel));
    let result =
        wiki_export_and_ingest_inner(&db_state, &orchestrator, &app_handle, Some(&cancel)).await;
    if let Err(ref e) = result {
        // Route through the canonical error logger so wiki errors surface in
        // Settings > Diagnostics. See `wiki_rebuild` for the CHECK-constraint
        // rationale.
        crate::db::audit_repo::log_error_best_effort(&db_state.conn, &e.to_string());
        emit_wiki_progress(&app_handle, WIKI_PIPELINE_TOTAL_STEPS, &format!("Error: {}", e));
    }
    wiki_state.clear_active();
    result
}

/// Inner implementation of wiki_export_and_ingest (without error logging wrapper).
async fn wiki_export_and_ingest_inner(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<ingest::IngestReport, AppError> {
    // Pre-flight: check whether the LLM is configured (mirrors wiki_ingest_inner).
    // When not, deterministic pre-seed still runs; only LLM batches are skipped.
    let skip_llm = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let has_llm = crate::db::llm_config_repo::has_config(&conn)?;
        if !has_llm {
            let _ = crate::db::audit_repo::log_error(
                &conn,
                "Wiki export-and-ingest: LLM not configured - deterministic \
                 pre-seed pages will be written, LLM synthesis skipped. \
                 Configure an LLM provider in Settings.",
            );
        }
        !has_llm
    };

    emit_wiki_progress(app_handle, 0, "Preparing raw sources...");

    // Lock briefly to load articles + config, then release so the CPU-bound
    // extraction runs lock-free. Per-article progress events fire in the
    // 10-15% range. Cancel is checked before each article.
    let (root, articles, config) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let root = storage::resolve_root(&conn)?;
        let articles = raw_export::load_included_articles(&conn)?;
        let config = crate::db::llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        (root, articles, config)
    };

    // Self-heal: ensure AGENTS.md exists so the wiki-view UI does not gate
    // the generated pages behind the "Initialize" empty-state.
    // No DB connection needed (AGENTS.md is a file write).
    let _ = ensure_initialized(&root);

    {
        let total = articles.len();
        let article_report = raw_export::write_article_exports(
            &root,
            &articles,
            Some(&|i, _total, _article_id| {
                let step = 10usize.saturating_add((i + 1).saturating_mul(5) / total.max(1));
                emit_wiki_progress(
                    app_handle,
                    step,
                    &format!("Exporting article {} of {}", i + 1, total),
                );
            }),
            cancel,
        )?;
        if article_report.cancelled {
            let mut report = ingest::IngestReport::default();
            report.errors.push("Cancelled".to_string());
            emit_wiki_progress(app_handle, WIKI_PIPELINE_TOTAL_STEPS, "Cancelled");
            return Ok(report);
        }
        raw_export::process_user_files(&root)?;
    }
    emit_wiki_progress(app_handle, 15, "Raw sources prepared");

    // Build prompt batches + dispatch them to the LLM in parallel. When the
    // corpus splits into multiple batches, the author manifest + pre-seed
    // optimization is applied to prevent cross-batch duplication.
    let prep_cb: WikiPrepProgressCb<'_> = Some(&|step, msg| {
        emit_wiki_progress(app_handle, step, msg);
    });
    let mut pre_seed_pages = 0usize;
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(
            &mut conn,
            &root,
            &config,
            cancel,
            prep_cb,
            &mut pre_seed_pages,
        )?
    };
    if is_cancelled(cancel) {
        let mut report = ingest::IngestReport::default();
        report.errors.push("Cancelled".to_string());
        emit_wiki_progress(app_handle, WIKI_PIPELINE_TOTAL_STEPS, "Cancelled");
        return Ok(report);
    }
    // LLM batch dispatch - skipped when unconfigured. The pre-seed pages are
    // already on disk; finalize_ingest will FTS5-index them regardless.
    let mut report = if skip_llm {
        let mut r = ingest::IngestReport { pages_written: pre_seed_pages, ..Default::default() };
        r.errors.push(
            "LLM not configured: deterministic pre-seed pages written, \
             LLM synthesis skipped."
                .to_string(),
        );
        emit_wiki_progress(app_handle, 50, "LLM not configured, skipping synthesis");
        r
    } else {
        let sender: Arc<dyn ingest::IngestLlmSender> =
            Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
        emit_wiki_progress(app_handle, 25, "Generating wiki pages via LLM...");
        ingest::run_chunked_ingest(&root, batches, sender, Some(app_handle), (25, 95), cancel)
            .await?
    };

    // Finalize (FTS5 rebuild + log + clear staleness).
    emit_wiki_progress(app_handle, 95, "Indexing pages...");
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    ingest::finalize_ingest(&conn, &root, &mut report)?;
    // Surface non-fatal warnings in Diagnostics.
    log_wiki_ingest_warnings(&conn, &report);

    emit_wiki_progress(app_handle, 100, &format!("Done: {} pages written", report.pages_written));
    Ok(report)
}

/// Cancel any in-flight wiki ingest.
///
/// Signals the active cancel token (if any) so the pipeline aborts between
/// pre-seed steps or between LLM batch completions. Safe to call when no
/// ingest is running (no-op). Mirrors `cancel_scraping`.
#[tauri::command]
pub fn cancel_wiki_ingest(state: tauri::State<'_, WikiIngestState>) -> Result<(), AppError> {
    state.cancel_active();
    Ok(())
}
