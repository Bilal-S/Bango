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
pub mod screening;
pub mod summary;

use commands::screening::ScreeningState;
use db::connection::DbState;

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

            app.manage(DbState { conn: std::sync::Mutex::new(conn) });

            Ok(())
        })
        .manage(ScreeningState { engine: tokio::sync::RwLock::new(None) })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::import::parse_ris_file,
            commands::import::import_ris_file,
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
            commands::llm_config::get_llm_config,
            commands::llm_config::save_llm_config,
            commands::llm_config::test_llm_connection,
            commands::llm_config::list_llm_models,
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
            commands::articles::get_import_activities,
            commands::screening::get_screening_readiness,
            commands::screening::start_screening,
            commands::screening::get_screening_progress,
            commands::screening::pause_screening,
            commands::screening::resume_screening,
            commands::screening::stop_screening,
            commands::screening::estimate_screening_tokens,
            commands::summary::generate_summary,
            commands::prisma::get_prisma_data,
            commands::prisma::get_prisma_svg,
            commands::export_cmd::export_ris,
            commands::export_cmd::export_ris_to_file,
            commands::export_cmd::export_project_backup,
            commands::export_cmd::export_project_to_file,
            commands::export_cmd::import_project_backup,
            commands::export_cmd::reset_project,
        ]);

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_pilot::init());

    if let Err(e) = builder.run(tauri::generate_context!()) {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
