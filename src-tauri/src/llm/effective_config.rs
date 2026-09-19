//! Effective generation configuration (T6).
//!
//! The resolver is the single source of truth for "what config does the
//! generation layer run with": the stored `llm_config` row under
//! `configured_provider`, or a complete Bango AI config built from the
//! engine's reserved loopback port plus recommended machine settings under
//! `bango_ai`. Callers use it for prompt budgeting and gating; the
//! orchestrator swaps in the engine's LIVE config (with the per-start API
//! key) at send time.

use crate::db::app_settings_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::backend::LlmBackend;
use crate::llm::local::engine::reserved_port;
use crate::llm::local::profile::LOCAL_LLM_PROFILE_ID;
use crate::models::llm_config::{LlmConfig, LlmProvider};

/// Bango AI call temperature (model card's precise-task recommendation).
pub const LOCAL_TEMPERATURE: f64 = 0.6;

/// Fallback window when nothing is configured (matches the UI floor).
pub const FALLBACK_CONTEXT_WINDOW: usize = 16_384;

/// Build the local effective config from the reserved port + the persisted
/// engine settings (single source of truth; RAM-aware defaults when the keys
/// are absent - the same values the status panel shows and the server uses).
pub fn local_config(conn: &rusqlite::Connection) -> Result<LlmConfig, AppError> {
    let port = reserved_port().ok_or_else(|| {
        AppError::Validation("Bango AI engine has not been initialized.".to_string())
    })?;
    let settings = app_settings_repo::get_bango_ai_settings(conn)?;
    Ok(LlmConfig {
        provider: LlmProvider::BangoAi,
        endpoint_url: format!("http://127.0.0.1:{port}/v1"),
        api_key_encrypted: None,
        model_name: LOCAL_LLM_PROFILE_ID.to_string(),
        temperature: LOCAL_TEMPERATURE,
        skip_temperature: false,
        max_concurrent_requests: 1,
        request_delay_ms: 0,
        context_window_tokens: settings.context,
    })
}

/// Resolve the effective generation config (decrypted cloud row or local).
pub fn resolve(conn: &rusqlite::Connection) -> Result<Option<LlmConfig>, AppError> {
    match app_settings_repo::get_llm_backend(conn)? {
        LlmBackend::BangoAi => Ok(Some(local_config(conn)?)),
        LlmBackend::ConfiguredProvider => llm_config_repo::get_config(conn),
    }
}

/// No-decrypt variant for the screening fast path (no PBKDF2 on hot paths).
pub fn resolve_no_decrypt(conn: &rusqlite::Connection) -> Result<Option<LlmConfig>, AppError> {
    match app_settings_repo::get_llm_backend(conn)? {
        LlmBackend::BangoAi => Ok(Some(local_config(conn)?)),
        LlmBackend::ConfiguredProvider => llm_config_repo::get_config_no_decrypt(conn),
    }
}

/// Prompt-budgeting context window for the active backend (a stored 50k
/// cloud row must never budget a 16k local prompt).
pub fn effective_context_window(conn: &rusqlite::Connection) -> Result<usize, AppError> {
    Ok(resolve_no_decrypt(conn)?
        .map_or(FALLBACK_CONTEXT_WINDOW, |c| c.context_window_tokens.max(1) as usize))
}

/// Whether the active backend serves generation locally (Bango AI).
pub fn is_local_backend(conn: &rusqlite::Connection) -> Result<bool, AppError> {
    Ok(app_settings_repo::get_llm_backend(conn)? == LlmBackend::BangoAi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::run_migrations;
    use crate::llm::local::engine::{
        BangoAiEngine, EngineState, HealthProbe, ServerProcess, ServerSpawner, ServerSpec,
    };
    use crate::llm::local::policy::ALLOWED_CONTEXTS;
    use std::sync::Arc;

    struct NoopSpawner;

    impl ServerSpawner for NoopSpawner {
        fn spawn(
            &self,
            _spec: &ServerSpec,
            _args: &[String],
        ) -> Result<Box<dyn ServerProcess>, AppError> {
            Err(AppError::Import("never spawns in this test".to_string()))
        }
    }

    struct NoopProbe;

    impl HealthProbe for NoopProbe {
        fn healthy(&self, _port: u16) -> bool {
            false
        }
    }

    #[test]
    fn resolver_returns_a_complete_local_config_without_starting_the_engine() {
        let conn = rusqlite::Connection::open_in_memory().expect("db");
        run_migrations(&conn).expect("migrations");
        let engine = BangoAiEngine::with_seams(Arc::new(NoopSpawner), Arc::new(NoopProbe));
        app_settings_repo::set_llm_backend(&conn, LlmBackend::BangoAi).expect("backend");

        let config = resolve(&conn).expect("resolve").expect("local config present");
        assert_eq!(config.provider, LlmProvider::BangoAi);
        let port = reserved_port().expect("reserved port");
        assert!(config.endpoint_url.contains(&format!(":{port}/v1")));
        assert_eq!(config.model_name, LOCAL_LLM_PROFILE_ID);
        assert_eq!(config.max_concurrent_requests, 1);
        assert_eq!(config.request_delay_ms, 0);
        assert_eq!(config.api_key_encrypted, None);
        assert!(ALLOWED_CONTEXTS.contains(&config.context_window_tokens));
        let window = effective_context_window(&conn).expect("window");
        assert_eq!(config.context_window_tokens as usize, window);
        assert!(is_local_backend(&conn).expect("backend check"));
        assert_eq!(
            engine.state().expect("state"),
            EngineState::Stopped,
            "resolving must not start the server"
        );

        // The default backend still reads the stored row (none here).
        app_settings_repo::set_llm_backend(&conn, LlmBackend::ConfiguredProvider).expect("backend");
        assert!(resolve(&conn).expect("resolve").is_none());
        assert_eq!(effective_context_window(&conn).expect("window"), FALLBACK_CONTEXT_WINDOW);
        assert!(!is_local_backend(&conn).expect("backend check"));
    }
}
