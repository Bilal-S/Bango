pub mod commands;
pub mod db;
pub mod error;
pub mod models;
pub mod ris;

use db::connection::{create_connection, DbState};
use db::migration::run_migrations;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let conn = match create_connection() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("fatal: failed to create database connection: {e:#}");
            std::process::exit(1);
        }
    };
    if let Err(e) = run_migrations(&conn) {
        eprintln!("fatal: failed to run database migrations: {e:#}");
        std::process::exit(1);
    }

    if let Err(e) = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DbState { conn: std::sync::Mutex::new(conn) })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::import::parse_ris_file,
            commands::import::import_ris_file,
            commands::import::get_articles,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
