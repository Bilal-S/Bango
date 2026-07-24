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
pub mod commands;
pub mod crypto;
pub mod db;
pub mod dedup;
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

use commands::screening::ScreeningState;
use commands::startup::StartupStatus;
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
            commands::labels::get_labels,
            commands::labels::get_labels_with_counts,
            commands::labels::create_label,
            commands::labels::rename_label,
            commands::labels::delete_label,
            commands::labels::update_label_color,
            commands::labels::suggest_labels,
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
            commands::articles::update_article_criteria,
            commands::articles::update_article_metadata,
            commands::articles::get_import_activities,
            commands::articles::get_activity_feed,
            commands::articles::get_generic_audit_entries,
            commands::articles::clear_generic_audit,
            commands::articles::rematch_journals,
            commands::articles::biblio_get_journal_info,
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
            commands::openalex::search_openalex,
            commands::openalex::import_openalex_articles,
            commands::openalex::check_dois_in_library,
            commands::openalex::get_openalex_settings,
            commands::openalex::set_openalex_settings,
            commands::openalex::smart_search_openalex,
            commands::openalex::download_and_attach_openalex_pdf,
        ]);

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_pilot::init());

    if let Err(e) = builder.run(tauri::generate_context!()) {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
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
    // Journal Index auto-load.
    let resource_path = resolve_journal_resource_path(handle.path());
    {
        let db = handle.state::<DbState>();
        let result = db::connection::lock_conn(&db.conn);
        match result {
            Ok(conn) => {
                if let Err(e) = load_journal_index_from_path(&conn, &resource_path) {
                    eprintln!("warning: failed to load journal index: {e:#}");
                }
            }
            Err(e) => eprintln!("warning: failed to lock DB for journal index load: {e:#}"),
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
    handle.manage(std::sync::Arc::new(LlmOrchestrator::new(max_conc, delay_ms)));

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
    let prod_path = match path.resource_dir() {
        Ok(p) => p.join("journal_index.db"),
        Err(_) => std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("journal_index.db"),
    };
    if prod_path.exists() {
        return prod_path;
    }
    let src_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("journal_index.db");
    if src_path.exists() {
        eprintln!("[journal_index] dev-mode fallback: using source tree path {:?}", src_path);
        return src_path;
    }
    // Neither found: return prod_path so the caller emits the "not found" warning.
    prod_path
}

/// Core loader: ATTACH the portal DB and bulk-copy records if the table is empty.
///
/// IMPORTANT: journal_index is system-distributed reference data. It must
/// survive project reset/upgrade and must not be exported/imported via backups.
fn load_journal_index_from_path(
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

    let portal_path = resource_path.to_string_lossy().to_string();
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS portal;",
        portal_path.replace('\'', "''")
    ))?;

    conn.execute_batch(
        "INSERT INTO journal_index
            (id, journal_title, issn, eissn, publisher_name,
             publisher_address, languages, web_of_science_categories,
             is_system, source_file, created_at, updated_at)
         SELECT
            id, journal_title, issn, eissn, publisher_name,
            publisher_address, languages, web_of_science_categories,
            1, source_file, created_at, updated_at
         FROM portal.journal_index;
         DETACH DATABASE portal;",
    )?;

    let new_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))?;

    eprintln!("[journal_index] loaded {} records from bundled portal DB", new_count);

    Ok(())
}
