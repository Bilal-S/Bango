//! Regression tests for the Test Connection embedding-probe persistence path
//! (`commands::llm_config::persist_embedding_probe_to_conn`) + the
//! `save_llm_config` conditional-reset contract (`embedding_relevant_changed`).
//!
//! Bug 1 fixed: `persist_embedding_probe` previously hardcoded `dimensions = 0`,
//! dropping the `ProbeOutcome.dimensions` value returned by the probe. This
//! left `app_settings.embedding_dimensions` at 0 after Test Connection, which
//! gated `recall` off (`dimensions <= 0`) until the first
//! `generate_embeddings` call populated the real value.
//!
//! Bug 2 fixed: `save_llm_config` unconditionally called
//! `reset_embedding_status` on every save, including parameters-only edits
//! (concurrency / delay / context / temperature). This discarded a known-good
//! `enabled`/`disabled` state (e.g. one just set by `test_llm_connection`) and
//! forced the next embedding call to re-probe redundantly — the root cause of
//! the "probe fires on first Citation Finder call" bug. The fix guards the
//! reset behind `embedding_relevant_changed` (provider / endpoint / model /
//! api-key comparison) so parameters-only saves preserve the status.
//!
//! These tests exercise the extracted DB-write core + the pure helper directly
//! (no Tauri `State<DbState>` construction needed) against an in-memory SQLite
//! DB with the full migration chain.

use bango_lib::commands::llm_config::{
    embedding_relevant_changed, persist_embedding_probe_to_conn,
};
use bango_lib::db::app_settings_repo::{self, EmbeddingStatus};
use bango_lib::db::connection::create_connection;
use bango_lib::db::llm_config_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};

#[test]
fn persist_probe_forwards_real_dimensions_when_enabled() {
    // Regression: the prior shape hardcoded 0 here, so `recall` (which gates
    // on `dimensions > 0`) returned empty until the first generate_embeddings.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let status = Some("enabled".to_string());
    let model = Some("text-embedding-3-small".to_string());
    persist_embedding_probe_to_conn(&conn, &status, &model, 1536).unwrap();

    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Enabled);
    assert_eq!(
        app_settings_repo::get_embedding_model(&conn).unwrap().as_deref(),
        Some("text-embedding-3-small")
    );
    // The key assertion: dimensions are forwarded, not hardcoded to 0.
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 1536);
}

#[test]
fn persist_probe_stores_zero_dimensions_when_disabled() {
    // A disabled probe carries no vectors, so dimensions = 0 is correct (the
    // recall gate then returns empty, which is the right behavior when the
    // provider has no embedding capability).
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let status = Some("disabled".to_string());
    persist_embedding_probe_to_conn(&conn, &status, &None, 0).unwrap();

    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Disabled);
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 0);
}

#[test]
fn persist_probe_overwrites_stale_dimensions_on_re_probe() {
    // Re-probing after a provider switch must overwrite the previously-stored
    // dimensions, not be stuck at the old value (or at 0 from the bug).
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    // First probe: OpenAI 3-small (1536).
    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("text-embedding-3-small".to_string()),
        1536,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 1536);

    // Second probe: switch to a 3072-dim model (e.g. 3-large).
    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("text-embedding-3-large".to_string()),
        3072,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 3072);
    assert_eq!(
        app_settings_repo::get_embedding_model(&conn).unwrap().as_deref(),
        Some("text-embedding-3-large")
    );
}

#[test]
fn persist_probe_disabled_then_enabled_round_trip() {
    // Covers the realistic sequence: probe a provider that has no embedding
    // support (disabled, dims 0), then switch providers and re-probe enabled.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    persist_embedding_probe_to_conn(&conn, &Some("disabled".to_string()), &None, 0).unwrap();
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 0);

    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("mistral-embed".to_string()),
        1024,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Enabled);
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 1024);
}

// ── embedding_relevant_changed (pure helper) ───────────────────────────

#[test]
fn embedding_relevant_changed_provider_change_detected() {
    let mut a = LlmConfig::default();
    a.provider = LlmProvider::Openai;
    let mut b = a.clone();
    b.provider = LlmProvider::MistralAi;
    assert!(embedding_relevant_changed(&a, &b));
}

#[test]
fn embedding_relevant_changed_endpoint_change_detected() {
    let mut a = LlmConfig::default();
    a.endpoint_url = "https://api.openai.com/v1".to_string();
    let mut b = a.clone();
    b.endpoint_url = "https://api.mistral.ai/v1".to_string();
    assert!(embedding_relevant_changed(&a, &b));
}

#[test]
fn embedding_relevant_changed_model_change_detected() {
    let mut a = LlmConfig::default();
    a.model_name = "gpt-4o".to_string();
    let mut b = a.clone();
    b.model_name = "gpt-4o-mini".to_string();
    assert!(embedding_relevant_changed(&a, &b));
}

#[test]
fn embedding_relevant_changed_api_key_change_detected() {
    let mut a = LlmConfig::default();
    a.api_key_encrypted = Some("sk-old".to_string());
    let mut b = a.clone();
    b.api_key_encrypted = Some("sk-new".to_string());
    assert!(embedding_relevant_changed(&a, &b));
}

#[test]
fn embedding_relevant_changed_parameters_only_not_detected() {
    // The bug-fix contract: parameters-only edits (concurrency / delay /
    // context window / temperature / skip_temperature) do NOT reset the
    // embedding status. This is the regression pin for the "probe fires on
    // first Citation Finder call" bug.
    let mut a = LlmConfig::default();
    a.max_concurrent_requests = 3;
    a.request_delay_ms = 500;
    a.context_window_tokens = 50_000;
    a.temperature = 0.2;
    a.skip_temperature = false;
    let mut b = a.clone();
    b.max_concurrent_requests = 5;
    b.request_delay_ms = 1000;
    b.context_window_tokens = 100_000;
    b.temperature = 0.7;
    b.skip_temperature = true;
    assert!(
        !embedding_relevant_changed(&a, &b),
        "parameters-only changes must NOT trigger an embedding-status reset"
    );
}

#[test]
fn embedding_relevant_changed_identical_configs_not_detected() {
    let a = LlmConfig::default();
    let b = a.clone();
    assert!(!embedding_relevant_changed(&a, &b));
}

// ── save_llm_config conditional-reset contract ─────────────────────────
//
// These exercise the full conditional-reset flow against an in-memory DB
// (the command layer is not invoked directly — it needs Tauri `State` — but
// the conditional-reset logic mirrors `save_llm_config`'s body exactly: read
// prev, compute `embedding_relevant_changed`, save, conditionally reset).
// They pin the contract that a parameters-only save preserves a known-good
// `enabled` status while an embedding-relevant change resets it.

/// Mirrors `save_llm_config`'s conditional-reset logic without the Tauri
/// `State` + orchestrator dependencies, so it is callable from a plain
/// integration test. Reads prev, saves next, conditionally resets the
/// embedding status — exactly the sequence the command performs.
fn save_llm_config_conditional_reset(conn: &rusqlite::Connection, next: &LlmConfig) {
    let prev = llm_config_repo::get_config(conn).unwrap();
    let needs_reset = prev.as_ref().is_none_or(|p| embedding_relevant_changed(p, next));
    llm_config_repo::save_config(conn, next).unwrap();
    if needs_reset {
        app_settings_repo::reset_embedding_status(conn).unwrap();
    }
}

#[test]
fn save_llm_config_parameters_only_preserves_enabled_status() {
    // Regression for the "probe fires on first Citation Finder call" bug.
    // After Test Connection sets `embedding_status = enabled`, a subsequent
    // parameters-only save (e.g. from the Settings auto-save watcher) must
    // NOT reset the status to `unknown`.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    // Seed: an initial config + a successful probe outcome (enabled).
    let mut initial = LlmConfig::default();
    initial.provider = LlmProvider::Openai;
    initial.endpoint_url = "https://api.openai.com/v1".to_string();
    initial.model_name = "gpt-4o".to_string();
    initial.api_key_encrypted = Some("sk-test".to_string());
    llm_config_repo::save_config(&conn, &initial).unwrap();
    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("text-embedding-3-small".to_string()),
        1536,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Enabled);

    // Act: a parameters-only save (concurrency / delay / context / temperature).
    let mut params_only = initial.clone();
    params_only.max_concurrent_requests = 8;
    params_only.request_delay_ms = 1200;
    params_only.context_window_tokens = 128_000;
    params_only.temperature = 0.5;
    save_llm_config_conditional_reset(&conn, &params_only);

    // Assert: the enabled status (and model + dimensions) survived the save.
    assert_eq!(
        app_settings_repo::get_embedding_status(&conn).unwrap(),
        EmbeddingStatus::Enabled,
        "parameters-only save must NOT reset a known-good enabled status"
    );
    assert_eq!(
        app_settings_repo::get_embedding_model(&conn).unwrap().as_deref(),
        Some("text-embedding-3-small"),
        "model must survive a parameters-only save"
    );
    assert_eq!(
        app_settings_repo::get_embedding_dimensions(&conn).unwrap(),
        1536,
        "dimensions must survive a parameters-only save"
    );
}

#[test]
fn save_llm_config_provider_change_resets_status() {
    // The flip side: a genuine provider/endpoint/model/api-key change DOES
    // reset the status so the next embedding call re-probes against the new
    // provider. Without this, a switch from OpenAI (enabled) to Anthropic
    // (known-unsupported) would leave the stale `enabled` status.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let mut initial = LlmConfig::default();
    initial.provider = LlmProvider::Openai;
    initial.endpoint_url = "https://api.openai.com/v1".to_string();
    initial.model_name = "gpt-4o".to_string();
    initial.api_key_encrypted = Some("sk-openai".to_string());
    llm_config_repo::save_config(&conn, &initial).unwrap();
    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("text-embedding-3-small".to_string()),
        1536,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Enabled);

    // Act: switch provider (embedding-relevant change).
    let mut switched = initial.clone();
    switched.provider = LlmProvider::Anthropic;
    switched.endpoint_url = "https://api.anthropic.com/v1".to_string();
    switched.model_name = "claude-3-5-sonnet".to_string();
    switched.api_key_encrypted = Some("sk-anthropic".to_string());
    save_llm_config_conditional_reset(&conn, &switched);

    // Assert: status reset to unknown so the next probe re-evaluates.
    assert_eq!(
        app_settings_repo::get_embedding_status(&conn).unwrap(),
        EmbeddingStatus::Unknown,
        "provider change MUST reset the status so the next call re-probes"
    );
}

#[test]
fn save_llm_config_first_save_resets_status() {
    // The very first save (no previous config) should reset the status to
    // unknown (prev = None → needs_reset = true). This covers the cold-start
    // path where there is no prior config to compare against.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    // Seed an enabled status directly (simulating a prior probe in a
    // different profile, then the config row is wiped).
    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("text-embedding-3-small".to_string()),
        1536,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Enabled);

    // Act: first-ever config save (no prev row).
    let cfg = LlmConfig::default();
    save_llm_config_conditional_reset(&conn, &cfg);

    // Assert: status reset to unknown (first save = no prev = needs_reset).
    assert_eq!(
        app_settings_repo::get_embedding_status(&conn).unwrap(),
        EmbeddingStatus::Unknown,
        "first save with no prior config MUST reset (no prev to compare)"
    );
}
