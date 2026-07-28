//! Wiki ingest / rebuild / export-and-ingest pipeline + batch builder.
//!
//! Extracted from the pre-split `wiki_cmd.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use std::path::PathBuf;
use std::sync::Arc;

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::{ingest, raw_export, storage};

use super::{
    emit_wiki_progress, ensure_initialized, log_wiki_ingest_warnings, WIKI_PIPELINE_TOTAL_STEPS,
};

/// Run the LLM wiki ingest: build prompt batches from raw sources, dispatch
/// them to the LLM in parallel (bounded by the orchestrator's concurrency
/// limit), write the generated pages, rebuild FTS5, and clear staleness.
#[tauri::command]
pub async fn wiki_ingest(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
) -> Result<ingest::IngestReport, AppError> {
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
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(&mut conn, &root, &config)?
    };
    let sender: Arc<dyn ingest::IngestLlmSender> =
        Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
    let mut report = ingest::run_chunked_ingest(&root, batches, sender, None, (25, 95)).await?;

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
fn build_batches_with_manifest(
    conn: &mut rusqlite::Connection,
    root: &std::path::Path,
    config: &crate::models::llm_config::LlmConfig,
) -> Result<Vec<ingest::IngestBatch>, AppError> {
    // Run the full 8-step bibliometric normalization pipeline so
    // `biblio_authors` (with metrics), `biblio_terms`, `biblio_article_terms`,
    // and the co-author/citation networks are all populated. This is the
    // single source of truth for all three pre-seed layers.
    //
    // Non-fatal: if normalization fails (e.g. empty corpus), we still proceed
    // so the LLM can operate on the raw sources alone. The pre-seeders will
    // simply find no rows and write nothing.
    let _ = crate::db::biblio_repo::run_full_normalization(conn);

    // Phase 1: Pre-seed author pages from `biblio_authors`.
    let manifest = ingest::build_author_manifest(conn)?;
    if !manifest.entries.is_empty() {
        // Errors are non-fatal: the LLM can still produce author pages itself,
        // and the consolidation pass will dedup them.
        let _ = ingest::preseed_authors(root, &manifest);
    }

    // Phase 2: Pre-seed synthesis pages from AI summaries.
    // Each included article with a `full_text_ai_summary` gets a synthesis page
    // whose slug = the article UUID (so [[uuid]] links resolve automatically).
    let _ = ingest::preseed_synthesis_from_ai_summaries(conn, root);

    // Phase 3: Pre-seed concept hubs from `biblio_terms`.
    // Caps at 25 terms so the concept layer stays curated + high-signal.
    let _ = ingest::preseed_concept_hubs(conn, root, 25);

    // Phase 4: Pre-seed method hubs from AI-summary `study_design` (when
    // present) with a `biblio_terms` fallback for abstracts-only corpora.
    // Caps at 25 so the methods layer stays curated + high-signal. Uses a
    // curated study-design lexicon so non-methodological terms are filtered.
    let methods_written = ingest::preseed_methods(conn, root, 25).unwrap_or(0);

    // Layer 1 (External Documents): Pre-seed source pages for user-uploaded
    // documents (Add Documents). Each external doc in `raw/` with a
    // `source_kind: user_*` gets a first-class wiki node at
    // `wiki/sources/{slug}.md` so `[[user-slug]]` wikilinks and
    // `[^art-user-slug]` footnote refs resolve to a navigable page.
    let _ = ingest::preseed_document_source_pages(root);

    // Rebuild batches with the manifest injected (when non-empty). The
    // manifest's `to_prompt_section()` directive tells the LLM NOT to create
    // author pages and to link to the canonical slugs instead.
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
) -> Result<ingest::IngestReport, AppError> {
    let result = wiki_rebuild_inner(&db_state, &orchestrator, &app_handle).await;
    if let Err(ref e) = result {
        // Route through the canonical error logger (action = 'error', in the
        // audit_entries CHECK allowlist). The old `log_wiki_error` used
        // action = 'wiki_ingest_error' which is NOT in the CHECK constraint,
        // so SQLite silently rejected every insert and wiki errors never
        // reached Settings > Diagnostics.
        crate::db::audit_repo::log_error_best_effort(&db_state.conn, &e.to_string());
        emit_wiki_progress(&app_handle, WIKI_PIPELINE_TOTAL_STEPS, &format!("Error: {}", e));
    }
    result
}

/// Inner implementation of wiki_rebuild (without error logging wrapper).
async fn wiki_rebuild_inner(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
) -> Result<ingest::IngestReport, AppError> {
    emit_wiki_progress(app_handle, 0, "Starting wiki rebuild...");

    // Step 0: Scaffold (ensure wiki-root exists) + self-heal AGENTS.md.
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let root = storage::resolve_root(&conn)?;
        storage::scaffold_tree(&root)?;
        let _ = ensure_initialized(&root);
    }
    emit_wiki_progress(app_handle, 10, "Wiki directory ready");

    // Step 1: Export included articles + process user files.
    let (root, config) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let root = storage::resolve_root(&conn)?;
        raw_export::prepare_all(&conn, &root)?;
        let config = crate::db::llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        (root, config)
    };
    emit_wiki_progress(app_handle, 15, "Raw sources prepared");

    // Step 2: Build prompt batches + dispatch them to the LLM in parallel.
    // Each batch carries the full source index, so batches are independent and
    // safe to run concurrently. Progress emits as each batch completes. When
    // the corpus splits into multiple batches, the author manifest + pre-seed
    // optimization is applied to prevent cross-batch duplication.
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(&mut conn, &root, &config)?
    };
    let sender: Arc<dyn ingest::IngestLlmSender> =
        Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
    emit_wiki_progress(app_handle, 25, "Generating wiki pages via LLM...");
    let mut report =
        ingest::run_chunked_ingest(&root, batches, sender, Some(app_handle), (25, 95)).await?;

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
) -> Result<ingest::IngestReport, AppError> {
    let result = wiki_export_and_ingest_inner(&db_state, &orchestrator, &app_handle).await;
    if let Err(ref e) = result {
        // Route through the canonical error logger so wiki errors surface in
        // Settings > Diagnostics. See `wiki_rebuild` for the CHECK-constraint
        // rationale.
        crate::db::audit_repo::log_error_best_effort(&db_state.conn, &e.to_string());
        emit_wiki_progress(&app_handle, WIKI_PIPELINE_TOTAL_STEPS, &format!("Error: {}", e));
    }
    result
}

/// Inner implementation of wiki_export_and_ingest (without error logging wrapper).
async fn wiki_export_and_ingest_inner(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
) -> Result<ingest::IngestReport, AppError> {
    emit_wiki_progress(app_handle, 0, "Preparing raw sources...");

    let (root, config) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let root = storage::resolve_root(&conn)?;
        raw_export::prepare_all(&conn, &root)?;
        let config = crate::db::llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        // Self-heal: ensure AGENTS.md exists so the wiki-view UI does not gate
        // the generated pages behind the "Initialize" empty-state.
        let _ = ensure_initialized(&root);
        (root, config)
    };
    emit_wiki_progress(app_handle, 15, "Raw sources prepared");

    // Build prompt batches + dispatch them to the LLM in parallel. When the
    // corpus splits into multiple batches, the author manifest + pre-seed
    // optimization is applied to prevent cross-batch duplication.
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(&mut conn, &root, &config)?
    };
    let sender: Arc<dyn ingest::IngestLlmSender> =
        Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
    emit_wiki_progress(app_handle, 25, "Generating wiki pages via LLM...");
    let mut report =
        ingest::run_chunked_ingest(&root, batches, sender, Some(app_handle), (25, 95)).await?;

    // Finalize (FTS5 rebuild + log + clear staleness).
    emit_wiki_progress(app_handle, 95, "Indexing pages...");
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    ingest::finalize_ingest(&conn, &root, &mut report)?;
    // Surface non-fatal warnings in Diagnostics.
    log_wiki_ingest_warnings(&conn, &report);

    emit_wiki_progress(app_handle, 100, &format!("Done: {} pages written", report.pages_written));
    Ok(report)
}

/// Helper used by tests and (later) other commands to resolve the root without
/// going through Tauri state. Not a `#[tauri::command]`.
#[allow(dead_code)]
pub(crate) fn root_for_conn(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    storage::resolve_root(conn)
}
