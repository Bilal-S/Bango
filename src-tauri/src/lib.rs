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
pub mod prisma;
pub mod ris;
pub mod scraping;
pub mod screening;
pub mod summary;
pub mod utils;

use commands::screening::ScreeningState;
use db::connection::DbState;
use llm::orchestrator::LlmOrchestrator;

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
            use tauri::Manager;
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

            if let Err(e) = crate::db::migration::run_migrations(&conn) {
                eprintln!("fatal: failed to run database migrations: {e:#}");
                std::process::exit(1);
            }

            // ── Journal Index: auto-load from bundled portal DB on first startup ──
            // IMPORTANT: journal_index is system-distributed reference data.
            // Do NOT include in project backup export/import or reset operations.
            if let Err(e) = load_journal_index_if_empty(&conn, app) {
                eprintln!("warning: failed to load journal index: {e:#}");
            }

            app.manage(DbState { conn: std::sync::Mutex::new(conn) });

            // Parse CLI / env flags and persist feature flags to DB.
            let args: Vec<String> = std::env::args().collect();
            let premium_from_cli = args.iter().any(|a| a == "--premium");
            let premium_from_env = std::env::var("BANGO_PREMIUM")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let premium_requested = premium_from_cli || premium_from_env;
            {
                let guard = app.state::<DbState>();
                let conn = guard.conn.lock().unwrap_or_else(|e| e.into_inner());
                if premium_requested {
                    if let Err(e) =
                        db::app_settings_repo::set_setting(&conn, "flag_premium", Some("true"))
                    {
                        eprintln!("warning: failed to persist flag_premium: {e:#}");
                    }
                }
            }

            // Read authoritative flag values from DB (persists across restarts).
            let premium = {
                let guard = app.state::<DbState>();
                let conn = guard.conn.lock().unwrap_or_else(|e| e.into_inner());
                db::app_settings_repo::get_setting(&conn, "flag_premium")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false)
            };
            app.manage(AppFlags { premium });

            // Initialize LLM orchestrator from saved config (defaults if no config saved yet)
            let (max_conc, delay_ms) = {
                let guard = app.state::<DbState>();
                let conn = guard.conn.lock().unwrap_or_else(|e| e.into_inner());
                match crate::db::llm_config_repo::get_config(&conn) {
                    Ok(Some(cfg)) => {
                        (cfg.max_concurrent_requests as usize, cfg.request_delay_ms as u64)
                    }
                    _ => (3, 500), // defaults
                }
            };
            app.manage(std::sync::Arc::new(LlmOrchestrator::new(max_conc, delay_ms)));

            Ok(())
        })
        .manage(ScreeningState { engine: tokio::sync::RwLock::new(None) })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
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
            commands::criteria::delete_research_aim,
            commands::criteria::get_criteria,
            commands::criteria::create_criterion,
            commands::criteria::update_criterion,
            commands::criteria::delete_criterion,
            commands::criteria::generate_criteria,
            commands::criteria::critique_criteria,
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
            commands::articles::update_article_status,
            commands::articles::get_audit_trail,
            commands::articles::get_recent_audit_entries,
            commands::articles::update_article_notes,
            commands::articles::update_article_tags,
            commands::articles::update_article_labels,
            commands::articles::override_ai_decision,
            commands::articles::update_article_criteria,
            commands::articles::get_import_activities,
            commands::articles::get_generic_audit_entries,
            commands::articles::clear_generic_audit,
            commands::articles::rematch_journals,
            commands::articles::bulk_update_article_status,
            commands::articles::bulk_add_tag_to_articles,
            commands::articles::bulk_add_label_to_articles,
            commands::app_settings::get_app_flags,
            commands::app_settings::get_fulltext_storage_dir,
            commands::app_settings::set_fulltext_storage_dir,
            commands::full_text::attach_full_text,
            commands::full_text::delete_full_text,
            commands::full_text::read_full_text,
            commands::full_text::get_full_text_file_path,
            commands::full_text::read_full_text_file_bytes,
            commands::screening::get_screening_readiness,
            commands::screening::start_screening,
            commands::screening::get_screening_progress,
            commands::screening::pause_screening,
            commands::screening::resume_screening,
            commands::screening::stop_screening,
            commands::screening::reset_screening_errors,
            commands::screening::reset_working_list,
            commands::screening::estimate_screening_tokens,
            commands::summary::generate_summary,
            commands::summary::get_saved_summary,
            commands::summary::generate_article_ai_summary,
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
            commands::scraping::scrape_citation_chaser_cmd,
            commands::biblio_cmd::biblio_normalize,
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
        ]);

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_pilot::init());

    if let Err(e) = builder.run(tauri::generate_context!()) {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}

/// Load journal index from the bundled portal DB into the main database,
/// but only if the journal_index table is currently empty (first startup).
///
/// IMPORTANT: journal_index is system-distributed reference data.
/// It must survive project reset, and must not be exported/imported via backups.
fn load_journal_index_if_empty(
    conn: &rusqlite::Connection,
    app: &tauri::App,
) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;

    // Check if journal_index already has data
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))?;

    if count > 0 {
        eprintln!("[journal_index] already populated ({} records), skipping load", count);
        return Ok(());
    }

    // Try to find the bundled portal DB resource.
    // Production: resource_dir() points to the bundle's resources folder.
    // Dev mode (cargo tauri dev): resources are NOT copied to target/debug/,
    //   so we fall back to the source tree via CARGO_MANIFEST_DIR.
    let resource_path = {
        let prod_path = app.path().resource_dir()?.join("journal_index.db");
        if prod_path.exists() {
            prod_path
        } else {
            let src_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("journal_index.db");
            if src_path.exists() {
                eprintln!(
                    "[journal_index] dev-mode fallback: using source tree path {:?}",
                    src_path
                );
                src_path
            } else {
                prod_path // will trigger the "not found" warning below
            }
        }
    };

    if !resource_path.exists() {
        eprintln!(
            "[journal_index] no bundled portal DB at {:?} — skipping auto-load",
            resource_path
        );
        return Ok(());
    }

    eprintln!("[journal_index] loading from bundled portal DB: {:?}", resource_path);

    // ATTACH the portal DB and bulk-copy into the main database
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
