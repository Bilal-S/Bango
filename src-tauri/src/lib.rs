// Deny `unwrap()`/`expect()`/`panic!()` in production code.
// `cfg_attr(not(test), ...)` suspends these under `cfg(test)` so inline
// `#[cfg(test)] mod tests` blocks and integration test crates stay idiomatic.
// The crate-wide `[lints.clippy]` in `Cargo.toml` intentionally does NOT set
// these, so nothing re-escalates in test profile.
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

            // Probe live schema BEFORE migrations to flag legacy install (old
            // `article_references` table, both old and new have user_version=1).
            // The frontend triggers `perform_legacy_upgrade` when needed.
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

            // Premium flag read synchronously (gates batch reference scraping;
            // must be ready before any IPC call). The two queries are ~1ms.
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

            // Defer non-critical init to a dedicated OS thread because the work
            // is entirely synchronous blocking I/O (must not occupy a tokio
            // executor worker). Frontend bootstrap only touches DbState +
            // AppFlags, both ready synchronously above.
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
            commands::screening::get_two_stage_thresholds,
            commands::screening::set_two_stage_thresholds,
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

/// `TemperatureFlagPersister` backed by `AppHandle`. Persists `skip_temperature`
/// via targeted `UPDATE` (not full `save_config`, which would race with UI saves).
/// Best-effort; errors logged to stderr.
struct AppHandleTemperaturePersister {
    handle: tauri::AppHandle,
}

impl llm::orchestrator::TemperatureFlagPersister for AppHandleTemperaturePersister {
    fn persist(&self, skip: bool) {
        // Bind lock result to a local before matching, mirroring the
        // established pattern in `init_background_state`. Inlining `match
        // lock_conn(&db.conn)` keeps the `MutexGuard` temporary alive until
        // after `db` is dropped, tripping E0597 under the borrow checker.
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

/// Load journal index from handle's resource path. Used by legacy upgrade
/// after schema rebuild.
pub(crate) fn load_journal_index_if_empty_handle(
    app: &tauri::AppHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    let guard = app.state::<DbState>();
    let conn = db::connection::lock_conn(&guard.conn)?;
    let resource_path = resolve_journal_resource_path(app.path());
    load_journal_index_from_path(&conn, &resource_path)
}

/// Ensure `journal_index` is populated. Empty → copy from bundled DB.
/// Returns `Ok(())` when already populated or after successful load.
/// Used after `reset_project` (blocking, surfaced to frontend) and at startup
/// (best-effort, logged).
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

/// Background init: journal index load, LLM orchestrator creation, translation
/// worker spawn + stranded recovery. Each step independent; errors logged + skipped.
fn init_background_state(handle: &tauri::AppHandle) {
    // Journal index auto-load (best-effort; degraded matching on failure).
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

    // LLM orchestrator from saved config (defaults: max_conc=3, delay=500ms).
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

    // Wire best-effort `skip_temperature` persister so the LLM client can
    // persist the recovery flag after a temperature-rejection 400.
    orchestrator.set_temperature_persister(std::sync::Arc::new(AppHandleTemperaturePersister {
        handle: handle.clone(),
    }));

    handle.manage(orchestrator);

    // TranslationDoneBus managed BEFORE worker spawn so first job finds it.
    handle.manage(translation::TranslationDoneBus::new());

    // Translation worker + stranded recovery. Spawned after orchestrator+bus
    // are managed.
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

/// Show native modal dialog when DB migrations fail at startup.
///
/// Runs inside `.setup()` before the webview exists. Shows `app_data_dir` path,
/// the database files to delete, and the error string. `blocking_show`.
fn show_migration_failure_dialog(app: &tauri::App, app_data_dir: &std::path::Path, error: &str) {
    let dir_display = app_data_dir.display();

    // WAL journal mode creates sidecar files; all three should be deleted
    // for a clean reset.
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

/// Resolve path to bundled `journal_index.db`: resource_dir → exe-relative → source tree.
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

    // Tier 2: exe-relative (`<exe_dir>/resources/`). Reliable for Tauri 2.x
    // NSIS/MSI/Store where `resource_dir()` can resolve incorrectly.
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

    // Tier 3: dev-mode source-tree fallback.
    let src_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("journal_index.db");
    if src_path.exists() {
        eprintln!("[journal_index] dev-mode fallback: using source tree path {:?}", src_path);
        return src_path;
    }

    // None found. Return prod_path for a concrete path in error messages;
    // log every path tried for Windows diagnostics.
    eprintln!(
        "[journal_index] bundled portal DB not found. Tried: resource_dir={:?}, exe-relative, source-tree={:?}",
        prod_path, src_path
    );
    prod_path
}

/// Core loader: copy bundled portal DB → target `journal_index` via two
/// separate connections.
///
/// # Why two connections instead of `ATTACH DATABASE`
///
/// The previous `ATTACH … AS portal` → `INSERT … SELECT FROM portal` pattern
/// failed on Windows when the bundled source was WAL-mode: SQLite cannot
/// acquire the cross-DB lock within the target's transaction. The fix: open a
/// read-only source connection and stream rows into the target's own transaction.
/// Each connection holds an independent lock on an independent file.
///
/// `pub` so integration tests can drive the loader directly.
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

    // Open READ_ONLY connection to source. Guarantees bundled file never
    // modified even if stale -wal/-shm sidecars present.
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
        | rusqlite::OpenFlags::SQLITE_OPEN_URI;
    let source = rusqlite::Connection::open_with_flags(resource_path, flags)?;
    // Defensive busy_timeout: a stray writer on the source returns SQLITE_BUSY_SNAPSHOT.
    source.busy_timeout(std::time::Duration::from_secs(5))?;

    // Stream rows source → target. SELECT on SOURCE connection, INSERT on TARGET
    // transaction. Borrows are independent, coexist in same scope.
    //
    // `unchecked_transaction` matches existing shared-ref signature; correct
    // because we never mix it with `execute_batch`.
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
