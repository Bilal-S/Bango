// Deny `unwrap()`/`expect()`/`panic!()` in production library/application
// code. The `cfg_attr(not(test), ...)` form keeps the rules active for normal
// builds (escalated to errors by `cargo clippy -- -D warnings`) while
// suspending them under `cfg(test)` so:
//   - inline `#[cfg(test)] mod tests` blocks inside `src/*.rs` (same crate),
//     AND
//   - integration test crates in `tests/*.rs` (separate crates that depend on
//     `bango_lib` and are never reached by this attribute anyway)
// both remain idiomatic. The crate-wide `[lints.clippy]` table in `Cargo.toml`
// intentionally does NOT set these lints either, so nothing re-escalates them
// in the test profile.
#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod batch_import;
pub mod biblio;
pub mod bibtex;
pub mod citation_finder;
pub mod commands;
pub mod crypto;
pub mod db;
pub mod dedup;
pub mod embedding;
pub mod error;
pub mod export;
pub mod llm;
pub mod models;
pub mod openalex;
pub mod prisma;
pub mod ris;
pub mod scraping;
pub mod screening;
pub mod summary;
pub mod translation;
pub mod utils;
pub mod wiki;

use commands::citation_finder::CitationFinderState;
use commands::scraping::ScrapingState;
use commands::screening::ScreeningState;
use commands::startup::StartupStatus;
use commands::wiki_cmd::WikiIngestState;
use db::connection::DbState;
use db::schema_check::{check_schema, SchemaStatus};
use llm::orchestrator::LlmOrchestrator;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// Application-level feature flags set at startup.
#[derive(Debug, Clone)]
pub struct AppFlags {
    pub premium: bool,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().unwrap_or_else(|e| {
                eprintln!("fatal: failed to get app data dir: {e}");
                std::process::exit(1);
            });
            if let Err(e) = std::fs::create_dir_all(&app_data_dir) {
                eprintln!("fatal: failed to create app data dir: {e}");
                std::process::exit(1);
            }
            let db_path = app_data_dir.join("bango.db");

            let conn = match crate::db::connection::create_connection_at(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("fatal: failed to create database connection: {e:#}");
                    std::process::exit(1);
                }
            };

            // ── Startup schema detection ──
            // Probe the live schema BEFORE running migrations so we can flag a
            // legacy install (old `article_references` table) that the migration
            // system cannot upgrade by itself (both old and new are user_version=1).
            // The result is published to the frontend via managed state; the
            // frontend triggers `perform_legacy_upgrade` when needed.
            let schema_status = match check_schema(&conn) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("fatal: failed to probe database schema: {e:#}");
                    std::process::exit(1);
                }
            };
            if schema_status == SchemaStatus::Legacy {
                eprintln!("[startup] legacy schema detected; upgrade will be triggered after webview loads");
            }

            if let Err(e) = crate::db::migration::run_migrations(&conn) {
                eprintln!("fatal: failed to run database migrations: {e:#}");
                show_migration_failure_dialog(app, &app_data_dir, &format!("{e:#}"));
                std::process::exit(1);
            }

            app.manage(DbState { conn: std::sync::Mutex::new(conn) });
            app.manage(StartupStatus { schema: std::sync::Mutex::new(schema_status) });

            // ── Premium flag: synchronous (gates a bootstrap feature) ──
            // The frontend reads `isPremium` once during bootstrap and uses it
            // to gate the batch reference scraping feature, so this must be
            // ready before any IPC call. The two queries are tiny (~1ms).
            let args: Vec<String> = std::env::args().collect();
            let premium_from_cli = args.iter().any(|a| a == "--premium");
            let premium_from_env = std::env::var("BANGO_PREMIUM")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let premium_requested = premium_from_cli || premium_from_env;
            let premium = {
                let guard = app.state::<DbState>();
                let conn = db::connection::lock_conn(&guard.conn)?;
                if premium_requested {
                    if let Err(e) =
                        db::app_settings_repo::set_setting(&conn, "flag_premium", Some("true"))
                    {
                        eprintln!("warning: failed to persist flag_premium: {e:#}");
                    }
                }
                db::app_settings_repo::get_setting(&conn, "flag_premium")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false)
            };
            app.manage(AppFlags { premium });

            // ── Defer non-critical init to a background thread ──
            // Journal index auto-load, LLM orchestrator creation, and the
            // translation worker are NOT needed during the initial dashboard
            // render. The frontend `bootstrap()` fetches only touch `DbState`
            // + `AppFlags`, both ready synchronously above. By the time the
            // user clicks anything that needs the orchestrator or worker
            // (screening, chat, translate), this background thread (a few DB
            // reads + a channel spawn, ~50-200ms) has long finished.
            //
            // A dedicated OS thread is used (not `tauri::async_runtime::spawn`)
            // because the work is entirely synchronous blocking I/O. Spawning
            // it on the async runtime would stall an executor worker thread.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                init_background_state(&handle);
            });

            Ok(())
        })
        .manage(ScreeningState { engine: tokio::sync::RwLock::new(None) })
        .manage(batch_import::BatchImportState::default())
        .manage(ScrapingState::default())
        .manage(CitationFinderState::default())
        .manage(WikiIngestState::default())
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::startup::get_startup_status,
            commands::startup::perform_legacy_upgrade,
            commands::import::parse_ris_file,
            commands::import::import_ris_file,
            commands::import::parse_bibtex_file,
            commands::import::import_bibtex_file,
            commands::import::get_articles,
            commands::dedup::check_duplicates,
            commands::dedup::merge_exact_duplicates,
            commands::dedup::run_deduplication,
            commands::dedup::resolve_fuzzy_match,
            commands::criteria::get_research_aims,
            commands::criteria::create_research_aim,
            commands::criteria::update_research_aim,
            commands::criteria::delete_research_aim,
            commands::criteria::get_criteria,
            commands::criteria::create_criterion,
            commands::criteria::update_criterion,
            commands::criteria::delete_criterion,
            commands::criteria::generate_criteria,
            commands::criteria::critique_criteria,
            commands::criteria::check_rules,
            commands::llm_config::get_llm_config,
            commands::llm_config::save_llm_config,
            commands::llm_config::test_llm_connection,
            commands::llm_config::list_llm_models,
            commands::llm_config::has_llm_config,
            commands::chat::send_chat_message,
            commands::tags::get_tags,
            commands::tags::get_tags_with_counts,
            commands::tags::create_tag,
            commands::tags::rename_tag,
            commands::tags::delete_tag,
            commands::tags::update_tag_color,
            commands::tags::suggest_tags,
            commands::tags::merge_tag,
            commands::labels::get_labels,
            commands::labels::get_labels_with_counts,
            commands::labels::create_label,
            commands::labels::rename_label,
            commands::labels::delete_label,
            commands::labels::update_label_color,
            commands::labels::suggest_labels,
            commands::labels::merge_label,
            commands::articles::query_articles,
            commands::articles::get_article_counts,
            commands::articles::get_article,
            commands::articles::delete_article,
            commands::articles::update_article_status,
            commands::articles::get_audit_trail,
            commands::articles::get_recent_audit_entries,
            commands::articles::update_article_notes,
            commands::articles::update_article_tags,
            commands::articles::update_article_labels,
            commands::articles::override_ai_decision,
            commands::articles::clear_ai_reasoning,
            commands::articles::update_article_criteria,
            commands::articles::update_article_metadata,
            commands::articles::get_import_activities,
            commands::articles::get_activity_feed,
            commands::articles::get_generic_audit_entries,
            commands::articles::clear_generic_audit,
            commands::articles::rematch_journals,
            commands::articles::biblio_get_journal_info,
            commands::articles::search_journal_index,
            commands::articles::link_article_to_journal_index,
            commands::articles::get_original_title,
            commands::articles::bulk_update_article_status,
            commands::articles::bulk_add_tag_to_articles,
            commands::articles::bulk_add_label_to_articles,
            commands::articles::bulk_remove_tag_from_articles,
            commands::articles::bulk_remove_label_from_articles,
            commands::app_settings::get_app_flags,
            commands::app_settings::get_storage_root,
            commands::app_settings::set_storage_root,
            commands::app_settings::get_auto_translate,
            commands::app_settings::set_auto_translate,
            commands::app_settings::get_screening_custom_logic,
            commands::app_settings::set_screening_custom_logic,
            commands::app_settings::get_project_name,
            commands::app_settings::set_project_name,
            commands::full_text::attach_full_text,
            commands::full_text::count_articles_with_full_text,
            commands::full_text::delete_full_text,
            commands::full_text::get_full_text_file_path,
            commands::full_text::read_full_text,
            commands::full_text::read_full_text_file_bytes,
            commands::full_text::rebuild_article_chunks,
            commands::screening::get_screening_readiness,
            commands::screening::start_screening,
            commands::screening::screen_article,
            commands::screening::get_screening_progress,
            commands::screening::pause_screening,
            commands::screening::resume_screening,
            commands::screening::stop_screening,
            commands::screening::reset_screening_errors,
            commands::screening::reset_working_list,
            commands::screening::estimate_screening_tokens,
            commands::screening::get_screening_mode,
            commands::screening::set_screening_mode,
            commands::screening::get_full_text_article_count,
            commands::summary::generate_summary,
            commands::summary::get_saved_summary,
            commands::summary::generate_article_ai_summary,
            commands::summary::generate_figure_descriptions,
            commands::summary::generate_unified_summary,
            commands::summary::analyze_research_gaps,
            commands::summary::get_saved_gap_analysis,
            commands::search_strategy::suggest_search_strategy,
            commands::translation::enqueue_article_translation,
            commands::translation::get_translation_status,
            commands::translation::retry_translation_job,
            commands::prisma::get_prisma_data,
            commands::prisma::get_prisma_svg,
            commands::prisma::export_prisma_svg_to_file,
            commands::prisma::export_prisma_png_to_file,
            commands::export_cmd::export_ris,
            commands::export_cmd::export_ris_to_file,
            commands::export_cmd::export_ris_for_tab_to_file,
            commands::export_cmd::export_ris_for_ids_to_file,
            commands::export_cmd::export_project_backup,
            commands::export_cmd::export_project_to_file,
            commands::export_cmd::import_project_backup,
            commands::export_cmd::write_text_to_file,
            commands::export_cmd::write_base64_to_file,
            commands::export_cmd::reset_project,
            commands::references::extract_cr_references,
            commands::references::get_article_references,
            commands::references::link_reference_to_article,
            commands::references::delete_article_references,
            commands::references::upsert_reference_paper,
            commands::references::preview_references_import,
            commands::references::import_references_for_article,
            commands::references::promote_reference_to_article,
            commands::references::query_reference_papers,
            commands::references::get_reference_articles_of_interest,
            commands::references::get_linked_articles_for_paper,
            commands::references::get_reference_paper,
            batch_import::start_batch_import,
            batch_import::cancel_batch_import,
            batch_import::get_batch_import_progress,
            commands::scraping::scrape_citation_chaser_cmd,
            commands::scraping::cancel_scraping,
            commands::biblio_cmd::biblio_normalize,
            commands::biblio_cmd::biblio_get_needs_refresh,
            commands::biblio_cmd::biblio_get_status,
            commands::biblio_cmd::biblio_get_authors,
            commands::biblio_cmd::biblio_get_terms,
            commands::biblio_cmd::biblio_get_coauthor_network,
            commands::biblio_cmd::biblio_get_kpis,
            commands::biblio_cmd::biblio_get_author_institutions,
            commands::biblio_cmd::biblio_get_unmatched_affiliation_count,
            commands::biblio_cmd::biblio_get_author_pubs_by_year,
            commands::biblio_cmd::biblio_get_citation_network,
            commands::biblio_cmd::biblio_get_keyword_network,
            commands::biblio_cmd::biblio_get_author_rankings,
            commands::biblio_cmd::biblio_get_author_detail,
            commands::biblio_cmd::biblio_get_author_productivity_kpis,
            commands::biblio_cmd::biblio_get_cocitation_network,
            commands::trends::check_trends_url,
            commands::wiki_cmd::wiki_get_status,
            commands::wiki_cmd::wiki_get_root_dir,
            commands::wiki_cmd::wiki_set_root_dir,
            commands::wiki_cmd::wiki_init,
            commands::wiki_cmd::wiki_export_raw,
            commands::wiki_cmd::wiki_add_raw_file,
            commands::wiki_cmd::wiki_add_raw_url,
            commands::wiki_cmd::wiki_list_raw_files,
            commands::wiki_cmd::wiki_search,
            commands::wiki_cmd::wiki_lint,
            commands::wiki_cmd::wiki_get_page,
            commands::wiki_cmd::wiki_update_page,
            commands::wiki_cmd::wiki_delete_page,
            commands::wiki_cmd::wiki_delete_wiki,
            commands::wiki_cmd::wiki_chat,
            commands::wiki_cmd::wiki_get_graph,
            commands::wiki_cmd::wiki_ingest,
            commands::wiki_cmd::wiki_list_pages,
            commands::wiki_cmd::wiki_list_sources,
            commands::wiki_cmd::wiki_rebuild,
            commands::wiki_cmd::wiki_export_and_ingest,
            commands::wiki_cmd::wiki_check_for_updates,
            commands::wiki_cmd::wiki_generate_export,
            commands::wiki_cmd::wiki_zip_export,
            commands::wiki_cmd::cancel_wiki_ingest,
            commands::openalex::search_openalex,
            commands::openalex::import_openalex_articles,
            commands::openalex::check_dois_in_library,
            commands::openalex::get_openalex_settings,
            commands::openalex::set_openalex_settings,
            commands::openalex::smart_search_openalex,
            commands::openalex::download_and_attach_openalex_pdf,
            commands::embedding::generate_embeddings,
            commands::embedding::recall_articles,
            commands::embedding::get_embedding_status,
            commands::embedding::probe_embeddings,
            commands::embedding::set_embedding_model_override,
            commands::embedding::get_embedding_model_mismatch,
            commands::embedding::regenerate_embeddings,
            commands::citation_finder::find_citations,
            commands::citation_finder::cancel_citation_search,
            commands::citation_finder::get_citation_finder_readiness,
        ]);

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_pilot::init());

    if let Err(e) = builder.run(tauri::generate_context!()) {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}

/// Production [`llm::orchestrator::TemperatureFlagPersister`] backed by the
/// Tauri `AppHandle`.
///
/// `persist(skip)` locks `DbState` and runs the targeted
/// `llm_config_repo::set_skip_temperature` `UPDATE`. Best-effort: errors are
/// logged to stderr and swallowed so a DB hiccup never fails a successful LLM
/// call (the next call will simply retry temperature-recovery again).
///
/// This struct is the single concrete bridge from the LLM layer to the DB layer
/// for temperature-flag persistence. It deliberately does NOT use `save_config`
/// (which `DELETE`s + `INSERT`s the whole row and would race with concurrent
/// `save_llm_config` calls from the UI); the targeted `UPDATE` touches only
/// `skip_temperature`.
///
/// The `AppHandle` is `Clone`-cheap (internally an `Arc`), so holding a copy
/// here for the lifetime of the orchestrator is free.
struct AppHandleTemperaturePersister {
    handle: tauri::AppHandle,
}

impl llm::orchestrator::TemperatureFlagPersister for AppHandleTemperaturePersister {
    fn persist(&self, skip: bool) {
        // Bind the lock result to a local before matching, mirroring the
        // established pattern in `init_background_state`: inlining
        // `match lock_conn(&db.conn)` keeps the `MutexGuard` temporary alive
        // until after `db` is dropped, tripping E0597 under the borrow checker.
        let db = self.handle.state::<DbState>();
        let result = db::connection::lock_conn(&db.conn);
        match result {
            Ok(conn) => {
                if let Err(e) = db::llm_config_repo::set_skip_temperature(&conn, skip) {
                    eprintln!(
                        "[LlmOrchestrator] best-effort skip_temperature persistence failed: {e}"
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "[LlmOrchestrator] best-effort skip_temperature persistence failed to lock DB: {e}"
                );
            }
        }
    }
}

/// `AppHandle`-based loader used by the legacy upgrade command after it
/// rebuilds the schema. Looks up the bundled journal_index DB via the handle's
/// resource path and bulk-copies records if the (newly recreated) table is empty.
pub(crate) fn load_journal_index_if_empty_handle(
    app: &tauri::AppHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    let guard = app.state::<DbState>();
    let conn = db::connection::lock_conn(&guard.conn)?;
    let resource_path = resolve_journal_resource_path(app.path());
    load_journal_index_from_path(&conn, &resource_path)
}

/// Ensure the `journal_index` table is populated. If it is empty, resolve the
/// bundled portal DB (via `resolve_journal_resource_path`) and bulk-copy its
/// rows. Used after `reset_project` (blocking; an error is surfaced to the
/// frontend so the user sees a Toast) and at startup (best-effort; the caller
/// logs the audit error and continues so the app still starts).
///
/// Returns `Ok(())` when the table is already populated or after a successful
/// load. Returns `Err` when the table is still empty after the load attempt so
/// the caller can decide whether to surface the error (reset) or just log it
/// (startup).
pub(crate) fn ensure_journal_index_populated(
    conn: &rusqlite::Connection,
    app: &tauri::AppHandle,
) -> Result<(), crate::error::AppError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }

    let resource_path = resolve_journal_resource_path(app.path());
    if let Err(e) = load_journal_index_from_path(conn, &resource_path) {
        return Err(crate::error::AppError::Import(format!(
            "Failed to load journal index from {:?}: {e}",
            resource_path
        )));
    }

    let new_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))?;
    if new_count == 0 {
        return Err(crate::error::AppError::Import(format!(
            "journal_index still empty after load; bundled DB not found at {:?}",
            resource_path
        )));
    }
    Ok(())
}

/// Background initialization: runs everything that does NOT need to block
/// `.setup()` from returning. Runs on a dedicated OS thread
/// (`std::thread::spawn`) because the work is entirely synchronous blocking
/// I/O, so it must not occupy a tokio executor worker thread.
///
/// Operations (in order):
/// 1. Journal index auto-load (first-startup bulk copy; no-op if populated).
/// 2. LLM orchestrator creation from saved config (defaults if none).
/// 3. Translation worker spawn + stranded-article crash recovery.
///
/// Each step is independent and logs + skips on error so a failure in one
/// does not prevent the others from running.
fn init_background_state(handle: &tauri::AppHandle) {
    // Journal Index auto-load (best-effort: a failure is logged to the audit
    // table + stderr but does not stop startup; journal matching will simply
    // be degraded until the next reset/upgrade reloads the table).
    //
    // Bind the lock result to a local before matching (mirroring the LLM +
    // stranded-recovery blocks below). Inlining `match lock_conn(&db.conn)`
    // keeps the `MutexGuard` temporary alive until after `db` is dropped,
    // which trips E0597 under the borrow checker.
    {
        let db = handle.state::<DbState>();
        let result = db::connection::lock_conn(&db.conn);
        match result {
            Ok(conn) => {
                if let Err(e) = ensure_journal_index_populated(&conn, handle) {
                    let msg = format!("Startup journal index load failed: {e}");
                    eprintln!("warning: {msg}");
                    let _ = db::audit_repo::log_error(&conn, &msg);
                }
            }
            Err(e) => eprintln!("warning: failed to lock DB for journal index load: {e}"),
        }
    }

    // LLM orchestrator from saved config (defaults if no config saved yet).
    let (max_conc, delay_ms) = {
        let db = handle.state::<DbState>();
        let result = db::connection::lock_conn(&db.conn);
        match result {
            Ok(conn) => match crate::db::llm_config_repo::get_config(&conn) {
                Ok(Some(cfg)) => {
                    (cfg.max_concurrent_requests as usize, cfg.request_delay_ms as u64)
                }
                _ => (3, 500), // defaults
            },
            Err(e) => {
                eprintln!("warning: failed to lock DB for LLM config: {e:#}");
                (3, 500)
            }
        }
    };
    let orchestrator = std::sync::Arc::new(LlmOrchestrator::new(max_conc, delay_ms));

    // Wire the best-effort `skip_temperature` persister so that when the LLM
    // client recovers from a temperature-rejection 400 (models that only
    // support the default temperature), the flag is persisted once and future
    // calls skip the wasteful first-attempt failure. The persister holds its
    // own clone of the `AppHandle` so it can reach `DbState` from the detached
    // persistence task without borrowing the orchestrator.
    orchestrator.set_temperature_persister(std::sync::Arc::new(AppHandleTemperaturePersister {
        handle: handle.clone(),
    }));

    handle.manage(orchestrator);

    // Tier 1e: the in-process broadcast bus the worker emits on after each
    // job. Managed BEFORE the worker spawns so the worker's first job can
    // always find it.
    handle.manage(translation::TranslationDoneBus::new());

    // Translation worker + stranded recovery. The worker is spawned after the
    // orchestrator + bus are managed so it can fetch them per-job.
    let translation_handle = translation::worker::spawn_translation_worker(handle.clone());
    {
        let db = handle.state::<DbState>();
        let result = db::connection::lock_conn(&db.conn);
        match result {
            Ok(conn) => {
                translation::worker::reenqueue_stranded_on_startup(
                    &conn,
                    translation_handle.sender(),
                );
            }
            Err(e) => eprintln!("warning: failed to lock DB for stranded recovery: {e:#}"),
        }
    }
    handle.manage(translation_handle);
}

/// Show a native modal dialog when the database migrations fail at startup.
///
/// Migrations run inside `.setup()` before the webview exists, so a native OS
/// dialog is the only viable UX. The message shows the resolved `app_data_dir`
/// path (so the user does not have to guess where the database files live),
/// explains the most common cause (an interrupted update), and tells the user
/// to back up or delete the database files and restart. The underlying error
/// string is appended at the bottom so the user can copy-paste it into a
/// support request.
///
/// The dialog is `blocking_show` because the caller (`run` setup hook) exits
/// the process immediately after this returns.
fn show_migration_failure_dialog(app: &tauri::App, app_data_dir: &std::path::Path, error: &str) {
    let dir_display = app_data_dir.display();

    // Build the platform-specific database file list. WAL journal mode
    // (enabled in `create_connection_at`) creates two sidecar files; all
    // three should be deleted together for a clean reset.
    let db_files = ["bango.db", "bango.db-wal", "bango.db-shm"]
        .into_iter()
        .map(|f| format!("  - {f}"))
        .collect::<Vec<_>>()
        .join("\n");

    let message = format!(
        "Bango could not open its database, so the app cannot continue.\n\n\
         This usually happens after an interrupted update (for example, the \
         app was force-closed while it was finishing a database migration).\n\n\
         To recover:\n\
         1. Back up your data (optional but recommended):\n    {dir_display}\n\n\
         2. Delete these files:\n{db_files}\n\n\
         3. Restart Bango.\n\n\
         Technical details (for support):\n{error}"
    );

    app.dialog()
        .message(message)
        .title("Bango - Cannot start")
        .kind(MessageDialogKind::Error)
        .blocking_show();
}

/// Resolve the path to the bundled `journal_index.db`, preferring the bundle
/// resource dir and falling back to the source tree in dev mode.
fn resolve_journal_resource_path(
    path: &tauri::path::PathResolver<tauri::Wry>,
) -> std::path::PathBuf {
    // Tier 1: Tauri `resource_dir()` (canonical on most platforms).
    let prod_path = match path.resource_dir() {
        Ok(p) => p.join("journal_index.db"),
        Err(e) => {
            eprintln!("[journal_index] resource_dir() failed: {e}");
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("journal_index.db")
        }
    };
    if prod_path.exists() {
        return prod_path;
    }

    // Tier 2: relative to the running executable
    // (`<exe_dir>/resources/journal_index.db`). This is the reliable fallback
    // for Tauri 2.x NSIS/MSI/Store deployments where `resource_dir()` can
    // resolve incorrectly (paths with spaces, sandboxed dirs, stale dirs after
    // an update, etc.). The bundled resources ship in a `resources/`
    // subdirectory of the main executable's directory.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let exe_relative = exe_dir.join("resources").join("journal_index.db");
            if exe_relative.exists() {
                eprintln!(
                    "[journal_index] resource_dir path missing; using exe-relative path: {:?}",
                    exe_relative
                );
                return exe_relative;
            }
        }
    }

    // Tier 3: dev-mode source-tree fallback (only exists during `cargo run`).
    let src_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("journal_index.db");
    if src_path.exists() {
        eprintln!("[journal_index] dev-mode fallback: using source tree path {:?}", src_path);
        return src_path;
    }

    // None found. Return prod_path so the caller has a concrete path to report;
    // log every path tried so Windows diagnostics show what was searched.
    eprintln!(
        "[journal_index] bundled portal DB not found. Tried: resource_dir={:?}, exe-relative, source-tree={:?}",
        prod_path, src_path
    );
    prod_path
}

/// Core loader: copy records from the bundled portal DB into the target's
/// empty `journal_index` table using **two separate connections**.
///
/// IMPORTANT: journal_index is system-distributed reference data. It must
/// survive project reset/upgrade and must not be exported/imported via backups.
///
/// # Why two connections instead of `ATTACH DATABASE`
///
/// The previous implementation used `ATTACH DATABASE 'source' AS portal` on the
/// target connection and then ran `INSERT INTO journal_index ... SELECT FROM
/// portal.journal_index` inside a transaction. On Windows this fails when the
/// bundled source DB is in WAL mode: SQLite cannot acquire the right
/// cross-database lock within the target's transaction context, surfacing as an
/// `ATTACH DATABASE` / `SQLITE_BUSY` / lock-acquisition error during a scripted
/// first-run setup.
///
/// The robust fix is to open a **separate, read-only** connection to the source
/// and stream rows into the target via its own transaction. Each connection
/// holds an independent lock against an independent file, so there is no
/// cross-database lock acquisition inside the target's transaction. The source
/// is opened `SQLITE_OPEN_READ_ONLY`, so SQLite never needs a write lock on the
/// bundled file. This is the canonical SQLite recommendation for copying data
/// between databases.
///
/// `pub` so integration tests in `tests/` can drive the loader directly
/// (per the project convention: helpers tested externally are `pub`).
pub fn load_journal_index_from_path(
    conn: &rusqlite::Connection,
    resource_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))?;
    if count > 0 {
        eprintln!("[journal_index] already populated ({} records), skipping load", count);
        return Ok(());
    }

    if !resource_path.exists() {
        eprintln!(
            "[journal_index] no bundled portal DB at {:?} - skipping auto-load",
            resource_path
        );
        return Ok(());
    }

    eprintln!("[journal_index] loading from bundled portal DB: {:?}", resource_path);

    // Open a READ-ONLY connection to the source DB. `READ_ONLY` guarantees the
    // bundled file is never modified even if a stale `-wal`/`-shm` sidecar is
    // present; SQLite's read-only WAL path reads the WAL'd data correctly.
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
        | rusqlite::OpenFlags::SQLITE_OPEN_URI;
    let source = rusqlite::Connection::open_with_flags(resource_path, flags)?;
    // Defensive busy_timeout so a stray writer on the source (shouldn't happen,
    // but possible if two app instances race) returns SQLITE_BUSY_SNAPSHOT
    // instead of an immediate error.
    source.busy_timeout(std::time::Duration::from_secs(5))?;

    // Stream rows from source -> target. CRITICAL: the SELECT is prepared on
    // the SOURCE connection and the INSERT on the TARGET transaction. Both
    // borrow their respective connections immutably; the borrows are
    // independent so they can coexist in the same scope.
    //
    // `unchecked_transaction` matches the existing shared-ref signature and is
    // correct here because we never mix it with `execute_batch`.
    let tx = conn.unchecked_transaction()?;

    {
        let mut insert = tx.prepare(
            "INSERT OR IGNORE INTO journal_index
                (id, journal_title, issn, eissn, publisher_name,
                 publisher_address, languages, web_of_science_categories,
                 is_system, source_file, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?10, ?11)",
        )?;
        let mut select = source.prepare(
            "SELECT id, journal_title, issn, eissn, publisher_name,
                    publisher_address, languages, web_of_science_categories,
                    source_file, created_at, updated_at
             FROM journal_index",
        )?;
        let mut rows = select.query([])?;

        let mut copied: i64 = 0;
        while let Some(row) = rows.next()? {
            insert.execute(rusqlite::params![
                row.get::<_, String>(0)?,         // id
                row.get::<_, String>(1)?,         // journal_title
                row.get::<_, Option<String>>(2)?, // issn
                row.get::<_, Option<String>>(3)?, // eissn
                row.get::<_, Option<String>>(4)?, // publisher_name
                row.get::<_, Option<String>>(5)?, // publisher_address
                row.get::<_, Option<String>>(6)?, // languages
                row.get::<_, Option<String>>(7)?, // web_of_science_categories
                row.get::<_, Option<String>>(8)?, // source_file
                row.get::<_, String>(9)?,         // created_at
                row.get::<_, String>(10)?,        // updated_at
            ])?;
            copied += 1;
        }
        eprintln!("[journal_index] streamed {} records from source", copied);
    }

    tx.commit()?;

    let new_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))?;

    eprintln!("[journal_index] loaded {} records from bundled portal DB", new_count);

    Ok(())
}
