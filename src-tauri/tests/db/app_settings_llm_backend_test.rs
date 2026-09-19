//! DB round-trip and portability tests for the `llm_backend` setting.
//!
//! The setting selects which backend serves generation calls (Configured
//! Provider vs Bango AI). It is a project-level preference that travels with
//! a project backup; readiness stays machine-evaluated. Engine settings
//! (context/threads/reasoning) are machine-local and must NOT travel.

use bango_lib::db::app_settings_repo::{
    get_llm_backend, is_project_portable, set_llm_backend, set_setting, LLM_BACKEND_KEY,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::llm::backend::LlmBackend;
use rusqlite::Connection;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

#[test]
fn llm_backend_round_trips_and_defaults() {
    let conn = test_db();
    assert_eq!(
        get_llm_backend(&conn).unwrap(),
        LlmBackend::ConfiguredProvider,
        "fresh DB must default to the configured provider"
    );
    set_llm_backend(&conn, LlmBackend::BangoAi).unwrap();
    assert_eq!(get_llm_backend(&conn).unwrap(), LlmBackend::BangoAi);
    set_llm_backend(&conn, LlmBackend::ConfiguredProvider).unwrap();
    assert_eq!(get_llm_backend(&conn).unwrap(), LlmBackend::ConfiguredProvider);

    // A corrupted row must never silently select the local backend.
    set_setting(&conn, LLM_BACKEND_KEY, Some("garbage-value")).unwrap();
    assert_eq!(get_llm_backend(&conn).unwrap(), LlmBackend::ConfiguredProvider);
}

#[test]
fn llm_backend_travels_with_project_backup() {
    assert!(
        is_project_portable(LLM_BACKEND_KEY),
        "llm_backend is a project-level preference and must travel with a project backup"
    );
    // Engine settings are hardware-specific and must stay machine-local.
    for key in ["bango_ai_context", "bango_ai_threads", "bango_ai_reasoning"] {
        assert!(!is_project_portable(key), "{key} must stay machine-local");
    }
}
