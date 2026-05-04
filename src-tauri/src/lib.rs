pub mod commands;
pub mod db;
pub mod error;
pub mod models;

use db::connection::{create_connection, DbState};
use db::migration::run_migrations;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let conn = create_connection().expect("Failed to create database connection");
    run_migrations(&conn).expect("Failed to run database migrations");

    if let Err(e) = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DbState {
            conn: std::sync::Mutex::new(conn),
        })
        .invoke_handler(tauri::generate_handler![commands::health_check])
        .run(tauri::generate_context!())
    {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
