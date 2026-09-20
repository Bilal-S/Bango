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

/// True when the effective generation config is the on-device Bango AI
/// backend. Local runs skip the heaviest optional LLM work (per-article
/// full-text summaries, per-framework polish) and size wiki batches for the
/// local output cap.
fn is_local_config(config: &crate::models::llm_config::LlmConfig) -> bool {
    config.provider == crate::models::llm_config::LlmProvider::BangoAi
}

/// Progress callback for the `build_batches_with_manifest` pre-seed phases.
///
/// Mirrors `ChunkProgressCb` in `commands/full_text.rs`. The callback receives
/// `(step_pct, message)` so the caller can emit a `wiki:progress` event in the
/// 15-25% range (the gap between "Raw sources prepared" and "Generating wiki
/// pages via LLM..."). The callback is `Option` so tests can pass `None`.
/// Prep-progress callback slot. `Send + Sync` bound required because the
/// callback stays alive across `.await` points inside the async pre-seed
/// pipeline (tauri command futures must be `Send`).
pub(super) type WikiPrepProgressCb<'a> = Option<&'a (dyn Fn(usize, &str) + Send + Sync)>;

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
        let has_llm = crate::llm::readiness::has_usable_llm(&conn)?;
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
        let config = crate::llm::effective_config::resolve(&conn)?.ok_or_else(|| {
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

    // Frameworks pre-phase: backfill + canonicalize blob framework names so
    // the deterministic framework pre-seed has grounded input.
    if !skip_llm {
        ensure_wiki_frameworks(db_state, orchestrator, app_handle, cancel).await?;
    }

    // Build batches (with author manifest if multi-batch) inside a DB scope.
    let prep_cb: WikiPrepProgressCb<'_> = Some(&|step, msg| {
        emit_wiki_progress(app_handle, step, msg);
    });
    let mut pre_seed_pages = 0usize;
    let mut framework_rows: Vec<ingest::FrameworkRow> = Vec::new();
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(
            &mut conn,
            &root,
            &config,
            cancel,
            prep_cb,
            &mut pre_seed_pages,
            &mut framework_rows,
        )?
    };
    // Framework polish pass: one LLM call per framework, outside the DB lock.
    // Skipped on Bango AI: the deterministic framework skeletons stay, and the
    // serialized CPU calls are not worth the minutes.
    if !skip_llm && !framework_rows.is_empty() {
        if is_local_config(&config) {
            eprintln!("[wiki:diag] framework polish skipped (Bango AI)");
        } else {
            let polished = ingest::polish_framework_pages(
                framework_rows,
                &root,
                orchestrator.inner(),
                &config,
            )
            .await?;
            eprintln!("[wiki:diag] framework pages polished: {polished}");
        }
    }
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
        let call_timeout = crate::llm::orchestrator::resolve_timeout(
            &crate::llm::orchestrator::LlmRequestType::WikiIngest,
            is_local_config(&config),
        );
        let sender: Arc<dyn ingest::IngestLlmSender> =
            Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
        ingest::run_chunked_ingest_with_progress(
            &root,
            batches,
            sender,
            Some(app_handle),
            (25, 95),
            cancel,
            ingest::IngestProgressConfig::production(call_timeout),
        )
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
/// Wiki ensure-summaries query (wikifix-final Change 1): included articles
/// with full text but a missing/empty AI-summary blob. Pub for integration
/// tests (`wiki_full_text_refresh_test.rs`).
pub fn wiki_articles_missing_summary(conn: &rusqlite::Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM articles \
         WHERE status = 'included' \
           AND full_text IS NOT NULL AND TRIM(full_text) <> '' \
           AND (full_text_ai_summary IS NULL OR TRIM(full_text_ai_summary) = '')",
    )?;
    let ids = stmt.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Record a per-article ensure-summaries failure (non-fatal): the article
/// falls back to its abstract for this wiki export. Pub for tests.
pub fn record_wiki_summary_failure(
    conn: &rusqlite::Connection,
    article_id: &str,
    err: &str,
) -> Result<(), AppError> {
    crate::db::audit_repo::create_entry(
        conn,
        article_id,
        "ai_summary",
        None,
        None,
        Some(&format!("Wiki ensure-summaries failed ({err}); abstract used for wiki export")),
        "ai",
    )?;
    Ok(())
}

/// Ensure-summaries pre-phase: generate the missing blob for every target so
/// the wiki export carries summary-scale content (full text is never sent).
/// Per-article failures are non-fatal (audit entry + abstract fallback).
/// Emits `wiki:progress` in the 1-9% slice; cancel-checked between articles.
///
/// Returns the number of full-text articles whose summary generation was
/// skipped because the backend is local (`is_local`), so the caller can warn
/// that the wiki export uses abstracts for them.
async fn ensure_wiki_summaries(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
    cancel: Option<&Arc<AtomicBool>>,
    is_local: bool,
) -> Result<usize, AppError> {
    /* Bango AI: skip the per-article full-text summary pass. Each summary is a
    long prompt plus a long generation on serialized CPU inference; the raw
    export falls back to the article abstract, which is the documented
    degraded-but-correct input. The missing-summary count is returned so the
    ingest report discloses the abstract-only fallback. */
    if is_local {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let skipped = wiki_articles_missing_summary(&conn)?.len();
        eprintln!("[wiki:diag] ensure-summaries skipped (Bango AI): {skipped} article(s)");
        return Ok(skipped);
    }
    let targets = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        wiki_articles_missing_summary(&conn)?
    };
    let total = targets.len();
    for (i, article_id) in targets.into_iter().enumerate() {
        if is_cancelled(cancel) {
            return Ok(0);
        }
        emit_wiki_progress(
            app_handle,
            1 + (i.saturating_mul(8) / total.max(1)),
            &format!("Generating AI summary {} of {}", i + 1, total),
        );
        if let Err(e) = crate::commands::summary::generate_article_ai_summary_inner(
            db_state,
            app_handle,
            orchestrator,
            &article_id,
            true,
        )
        .await
        {
            eprintln!("[wiki:diag] ensure-summaries failed for {article_id}: {e}");
            let conn = crate::db::connection::lock_conn(&db_state.conn)?;
            let _ = record_wiki_summary_failure(&conn, &article_id, &e.to_string());
        }
    }
    Ok(0)
}

/// Wiki ensure-frameworks query: included full-text articles whose blob
/// exists but lacks the `theoretical_frameworks` key. Pub for tests.
pub fn wiki_articles_missing_frameworks(
    conn: &rusqlite::Connection,
) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM articles \
         WHERE status = 'included' \
           AND full_text IS NOT NULL AND TRIM(full_text) <> '' \
           AND full_text_ai_summary IS NOT NULL AND full_text_ai_summary != '' \
           AND full_text_ai_summary NOT LIKE '%\"theoretical_frameworks\"%'",
    )?;
    let ids = stmt.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// System prompt for the one-shot corpus-level framework-alias merge.
const FRAMEWORK_ALIAS_MERGE_SYSTEM_PROMPT: &str = "You merge duplicate theoretical-framework \
     names. You receive distinct framework names found across a review's articles; some are \
     spelling or acronym variants of the same framework. Respond with JSON ONLY: \
     {\"merges\": [{\"canonical\": \"<chosen canonical name>\", \"absorb\": [\"<names to merge \
     into it>\"]}]}. Merge ONLY true variants/acronyms of the same framework (e.g. \"DSM-5\" \
     and \"Diagnostic and Statistical Manual of Mental Disorders\"); keep genuinely different \
     frameworks separate. Empty merges list when nothing should merge. No code fences.";

/// Ensure-frameworks pre-phase (frameworks plan Stage 1-2): backfill the
/// `theoretical_frameworks` blob field for included full-text articles, then
/// canonicalize names across the corpus (deterministic clustering + one LLM
/// alias-merge call) so every consumer sees identical names. Non-fatal per
/// article; a no-op without LLM config.
async fn ensure_wiki_frameworks(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<(), AppError> {
    let config = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        crate::llm::effective_config::resolve(&conn)?
    };
    let Some(config) = config else { return Ok(()) };

    // Phase 1: backfill the missing `theoretical_frameworks` fields.
    let targets = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        wiki_articles_missing_frameworks(&conn)?
    };
    let total = targets.len();
    for (i, article_id) in targets.into_iter().enumerate() {
        if is_cancelled(cancel) {
            return Ok(());
        }
        emit_wiki_progress(
            app_handle,
            2 + (i.saturating_mul(3) / total.max(1)),
            &format!("Extracting frameworks {} of {}", i + 1, total),
        );
        let (title, full_text) = {
            let conn = crate::db::connection::lock_conn(&db_state.conn)?;
            crate::db::article_repo::get_full_text_for_summary(&conn, &article_id)?
        };
        let max_chars = ((config.context_window_tokens as usize).saturating_sub(2000)) * 4;
        let truncated =
            if full_text.len() > max_chars { &full_text[..max_chars] } else { &full_text };
        let user_prompt = format!("## Article Title\n{title}\n\n## Full Text\n{truncated}");
        match orchestrator
            .send_json(
                &config,
                ingest::FRAMEWORK_EXTRACTION_SYSTEM_PROMPT,
                &user_prompt,
                crate::llm::orchestrator::LlmRequestType::ArticleSummary,
            )
            .await
        {
            Ok((json, _tokens)) => {
                let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                let existing: Option<String> = conn
                    .query_row(
                        "SELECT full_text_ai_summary FROM articles WHERE id = ?1",
                        rusqlite::params![&article_id],
                        |r| r.get(0),
                    )
                    .ok();
                let merged = existing.as_deref().and_then(|blob| {
                    crate::commands::summary::merge_frameworks_into_blob(blob, &json)
                });
                if let Some(merged) = merged {
                    crate::db::article_repo::set_ai_summary(&conn, &article_id, &merged)?;
                    crate::db::audit_repo::create_entry(
                        &conn,
                        &article_id,
                        "ai_summary",
                        None,
                        None,
                        Some("Wiki frameworks extracted from full text"),
                        "ai",
                    )?;
                }
            }
            Err(e) => {
                eprintln!("[wiki:diag] framework extraction failed for {article_id}: {e}");
            }
        }
    }

    // Phase 2: canonicalize names across blobs.
    canonicalize_framework_names(db_state, orchestrator, &config).await?;
    Ok(())
}

/// Stage 2 canonicalization: deterministic slug clustering, one LLM
/// alias-merge call for acronym variants, then blob rewrites so every
/// downstream consumer (export, pre-seed, article pages) sees identical names.
async fn canonicalize_framework_names(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    config: &crate::models::llm_config::LlmConfig,
) -> Result<(), AppError> {
    let articles: Vec<(String, String)> = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let mut stmt = conn.prepare(
            "SELECT id, full_text_ai_summary FROM articles \
             WHERE status = 'included' AND full_text_ai_summary IS NOT NULL \
               AND full_text_ai_summary LIKE '%\"theoretical_frameworks\"%'",
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .filter_map(Result::ok)
            .collect();
        rows
    };
    if articles.is_empty() {
        return Ok(());
    }
    let mut all_names: Vec<String> = Vec::new();
    for (_, blob) in &articles {
        if let Some(parsed) = crate::wiki::ingest::synthesis::parse_ai_summary(blob) {
            for fw in &parsed.theoretical_frameworks {
                all_names.push(fw.name.clone());
            }
        }
    }
    let mut map = ingest::canonical_name_map(&all_names);
    let mut distinct: Vec<String> =
        map.values().cloned().collect::<std::collections::HashSet<_>>().into_iter().collect();
    distinct.sort();
    // One LLM alias-merge call when more than one distinct name exists.
    if distinct.len() > 1 {
        let list = distinct.iter().map(|n| format!("- {n}")).collect::<Vec<_>>().join("\n");
        let prompt = format!("Distinct framework names found in the review:\n\n{list}\n");
        if let Ok((json, _)) = orchestrator
            .send_json(
                config,
                FRAMEWORK_ALIAS_MERGE_SYSTEM_PROMPT,
                &prompt,
                crate::llm::orchestrator::LlmRequestType::ArticleSummary,
            )
            .await
        {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
                let mut merges: Vec<(String, Vec<String>)> = Vec::new();
                if let Some(arr) = value.get("merges").and_then(|v| v.as_array()) {
                    for entry in arr {
                        let Some(canonical) =
                            entry.get("canonical").and_then(|v| v.as_str()).map(str::to_string)
                        else {
                            continue;
                        };
                        let absorb: Vec<String> = entry
                            .get("absorb")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter().filter_map(|x| x.as_str()).map(str::to_string).collect()
                            })
                            .unwrap_or_default();
                        if !canonical.is_empty() && !absorb.is_empty() {
                            merges.push((canonical, absorb));
                        }
                    }
                }
                if !merges.is_empty() {
                    map = ingest::apply_alias_merges(&map, &merges);
                }
            }
        }
    }
    // Rewrite blobs whose raw names map to a different canonical name.
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let mut rewritten = 0usize;
    for (article_id, blob) in &articles {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(blob) else { continue };
        let Some(arr) = value.get_mut("theoretical_frameworks").and_then(|v| v.as_array_mut())
        else {
            continue;
        };
        let mut changed = false;
        for entry in arr.iter_mut() {
            let raw = entry.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if let Some(canonical) = map.get(&raw) {
                if *canonical != raw {
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert(
                            "name".to_string(),
                            serde_json::Value::String(canonical.clone()),
                        );
                        changed = true;
                    }
                }
            }
        }
        if changed {
            let merged = serde_json::to_string(&value).unwrap_or_default();
            crate::db::article_repo::set_ai_summary(&conn, article_id, &merged)?;
            crate::db::audit_repo::create_entry(
                &conn,
                article_id,
                "ai_summary",
                None,
                None,
                Some("Wiki framework names canonicalized"),
                "ai",
            )?;
            rewritten += 1;
        }
    }
    if rewritten > 0 {
        eprintln!("[wiki:diag] canonicalized framework names in {rewritten} blob(s)");
    }
    Ok(())
}

fn build_batches_with_manifest(
    conn: &mut rusqlite::Connection,
    root: &std::path::Path,
    config: &crate::models::llm_config::LlmConfig,
    cancel: Option<&Arc<AtomicBool>>,
    prep_cb: WikiPrepProgressCb<'_>,
    pre_seed_pages: &mut usize,
    framework_rows: &mut Vec<ingest::FrameworkRow>,
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
        if let Err(e) = crate::db::biblio_repo::run_full_normalization(conn, None) {
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

    // Phase 5: Pre-seed framework SKELETON pages from the canonicalized blob
    // `theoretical_frameworks` field (sync, inside the DB lock). The LLM
    // polish pass (one call per framework, sees every naming article) runs
    // after this function returns, outside the DB lock, at the call sites.
    eprintln!("[wiki:diag] phase=preparing:frameworks");
    if let Some(cb) = prep_cb {
        cb(20, "Preparing framework pages...");
    }
    *framework_rows = ingest::fetch_framework_rows(conn)?;
    *pre_seed_pages += ingest::preseed_framework_pages(framework_rows, root)?;

    // Layer 1 (External Documents): Pre-seed source pages for user-uploaded
    // documents (Add Documents). Each external doc in `raw/` with a
    // `source_kind: user_*` gets a first-class wiki node at
    // `wiki/sources/{slug}.md` so `[[user-slug]]` wikilinks and
    // `[^art-user-slug]` footnote refs resolve to a navigable page.
    eprintln!("[wiki:diag] phase=preparing:sources");
    if let Some(cb) = prep_cb {
        cb(21, "Preparing source pages...");
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
        cb(22, "Building LLM batches...");
    }
    let methods_pre_seeded = methods_written > 0;
    // Two-sided sizing (wikifix-final Change 4.4): planning-only estimate,
    // nothing is sent to the provider. Local (Bango AI) runs floor it to the
    // local output cap so batches stay bounded.
    let output_budget = ingest::wiki_batch_output_budget(config);
    if manifest.entries.is_empty() {
        ingest::build_ingest_prompt_batches_with_budgets(
            root,
            config.context_window_tokens,
            None,
            methods_pre_seeded,
            output_budget,
        )
    } else {
        ingest::build_ingest_prompt_batches_with_budgets(
            root,
            config.context_window_tokens,
            Some(&manifest),
            methods_pre_seeded,
            output_budget,
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
        let has_llm = crate::llm::readiness::has_usable_llm(&conn)?;
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
        let config = crate::llm::effective_config::resolve(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        (root, articles, config)
    };

    // Step 0.5 (wikifix-final Change 1): ensure every included full-text
    // article has an AI-summary blob before export; the wiki LLM never sees
    // full text. Skipped when the LLM is not configured (abstract fallback).
    // Under Bango AI the whole pass is skipped and the returned count becomes
    // an abstract-only warning on the report.
    let mut summary_skips = 0usize;
    if !skip_llm {
        summary_skips = ensure_wiki_summaries(
            db_state,
            orchestrator,
            app_handle,
            cancel,
            is_local_config(&config),
        )
        .await?;
        ensure_wiki_frameworks(db_state, orchestrator, app_handle, cancel).await?;
    }
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
    let mut framework_rows: Vec<ingest::FrameworkRow> = Vec::new();
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(
            &mut conn,
            &root,
            &config,
            cancel,
            prep_cb,
            &mut pre_seed_pages,
            &mut framework_rows,
        )?
    };
    // Framework polish pass: one LLM call per framework, outside the DB lock.
    // Skipped on Bango AI: the deterministic framework skeletons stay, and the
    // serialized CPU calls are not worth the minutes.
    if !skip_llm && !framework_rows.is_empty() {
        if is_local_config(&config) {
            eprintln!("[wiki:diag] framework polish skipped (Bango AI)");
        } else {
            let polished = ingest::polish_framework_pages(
                framework_rows,
                &root,
                orchestrator.inner(),
                &config,
            )
            .await?;
            eprintln!("[wiki:diag] framework pages polished: {polished}");
        }
    }
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
        let call_timeout = crate::llm::orchestrator::resolve_timeout(
            &crate::llm::orchestrator::LlmRequestType::WikiIngest,
            is_local_config(&config),
        );
        let sender: Arc<dyn ingest::IngestLlmSender> =
            Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
        emit_wiki_progress(app_handle, 25, "Generating wiki pages via LLM...");
        ingest::run_chunked_ingest_with_progress(
            &root,
            batches,
            sender,
            Some(app_handle),
            (25, 95),
            cancel,
            ingest::IngestProgressConfig::production(call_timeout),
        )
        .await?
    };

    if summary_skips > 0 {
        report.warnings.push(format!(
            "{summary_skips} full-text article(s) have no AI summary; Bango AI uses their \
             abstracts for the wiki export."
        ));
    }

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
        let has_llm = crate::llm::readiness::has_usable_llm(&conn)?;
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
        let config = crate::llm::effective_config::resolve(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        (root, articles, config)
    };

    // Ensure-summaries pre-phase (wikifix-final Change 1): generate missing
    // blobs before the export below so exports carry summary-scale content.
    // Skipped when the LLM is not configured (abstract fallback). Under Bango
    // AI the whole pass is skipped and the returned count becomes an
    // abstract-only warning on the report.
    let mut summary_skips = 0usize;
    if !skip_llm {
        summary_skips = ensure_wiki_summaries(
            db_state,
            orchestrator,
            app_handle,
            cancel,
            is_local_config(&config),
        )
        .await?;
        ensure_wiki_frameworks(db_state, orchestrator, app_handle, cancel).await?;
    }

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
    let mut framework_rows: Vec<ingest::FrameworkRow> = Vec::new();
    let batches = {
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
        build_batches_with_manifest(
            &mut conn,
            &root,
            &config,
            cancel,
            prep_cb,
            &mut pre_seed_pages,
            &mut framework_rows,
        )?
    };
    // Framework polish pass: one LLM call per framework, outside the DB lock.
    // Skipped on Bango AI: the deterministic framework skeletons stay, and the
    // serialized CPU calls are not worth the minutes.
    if !skip_llm && !framework_rows.is_empty() {
        if is_local_config(&config) {
            eprintln!("[wiki:diag] framework polish skipped (Bango AI)");
        } else {
            let polished = ingest::polish_framework_pages(
                framework_rows,
                &root,
                orchestrator.inner(),
                &config,
            )
            .await?;
            eprintln!("[wiki:diag] framework pages polished: {polished}");
        }
    }
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
        let call_timeout = crate::llm::orchestrator::resolve_timeout(
            &crate::llm::orchestrator::LlmRequestType::WikiIngest,
            is_local_config(&config),
        );
        let sender: Arc<dyn ingest::IngestLlmSender> =
            Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
        emit_wiki_progress(app_handle, 25, "Generating wiki pages via LLM...");
        ingest::run_chunked_ingest_with_progress(
            &root,
            batches,
            sender,
            Some(app_handle),
            (25, 95),
            cancel,
            ingest::IngestProgressConfig::production(call_timeout),
        )
        .await?
    };

    if summary_skips > 0 {
        report.warnings.push(format!(
            "{summary_skips} full-text article(s) have no AI summary; Bango AI uses their \
             abstracts for the wiki export."
        ));
    }

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
