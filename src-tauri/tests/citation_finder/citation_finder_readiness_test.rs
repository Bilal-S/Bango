//! External unit tests for `citation_finder::readiness` (pure helpers).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` per `docs/CLAUDE.md`
//! §Testing ("Avoid large inline unit tests in library source files").
//!
//! `compute_readiness` itself is DB-backed (covered indirectly via
//! `embedding_recall_multistatus_test`); only the pure `coverage_percentage`
//! helper is unit-tested here.

use bango_lib::citation_finder::readiness::coverage_percentage;

// ── coverage_percentage (pure) ───────────────────────────────────────────

#[test]
fn coverage_empty_corpus_is_full() {
    assert_eq!(coverage_percentage(0, 0), 100.0);
}

#[test]
fn coverage_full() {
    assert_eq!(coverage_percentage(10, 10), 100.0);
}

#[test]
fn coverage_half() {
    let pct = coverage_percentage(10, 5);
    assert!((pct - 50.0).abs() < 1e-9);
}

#[test]
fn coverage_zero_embedded() {
    assert_eq!(coverage_percentage(10, 0), 0.0);
}

#[test]
fn coverage_embedded_exceeds_total_clamps() {
    // Defensive: embedded > total shouldn't happen but clamps to 100%.
    assert_eq!(coverage_percentage(10, 15), 100.0);
}

#[test]
fn coverage_negative_embedded_clamps_to_zero() {
    assert_eq!(coverage_percentage(10, -3), 0.0);
}

// ── compute_readiness static-override for known-unsupported providers ────
//
// `compute_readiness` overrides the reported `embedding_status` to "disabled"
// when the persisted status is "unknown" BUT the configured provider is
// statically known-unsupported (Anthropic, Z.AI). This closes the UX gap
// where selecting Anthropic + navigating to Chat (without clicking Test
// Connection first) showed the toggle as clickable even though Anthropic can
// never serve embeddings.

use bango_lib::citation_finder::readiness::compute_readiness;
use bango_lib::db::app_settings_repo::{self, EmbeddingStatus};
use bango_lib::db::connection::create_connection;
use bango_lib::db::llm_config_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};

fn seed_provider(conn: &rusqlite::Connection, provider: LlmProvider) {
    let cfg = LlmConfig {
        provider,
        endpoint_url: "https://api.example.com/v1".to_string(),
        api_key_encrypted: Some("sk-test".to_string()),
        model_name: "model".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    };
    llm_config_repo::save_config(conn, &cfg).expect("save config");
}

#[test]
fn compute_readiness_anthropic_overrides_unknown_to_disabled() {
    // The persisted status is "unknown" (probe has not run). Anthropic is
    // statically known-unsupported, so the readiness payload must report
    // "disabled" so the frontend can render the disabled toggle immediately.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_provider(&conn, LlmProvider::Anthropic);
    // Leave embedding_status at its default (Unknown) - do NOT call
    // set_embedding_status, simulating "user just selected Anthropic in
    // Settings and navigated to Chat without clicking Test Connection".
    let r = compute_readiness(&conn, &[]).unwrap();
    assert_eq!(r.embedding_status, "disabled", "Anthropic must override unknown → disabled");
    assert!(!r.provider_supports_embeddings);
}

#[test]
fn compute_readiness_zai_overrides_unknown_to_disabled() {
    // Same contract for Z.AI (the other known-unsupported provider).
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_provider(&conn, LlmProvider::ZAi);
    let r = compute_readiness(&conn, &[]).unwrap();
    assert_eq!(r.embedding_status, "disabled");
    assert!(!r.provider_supports_embeddings);
}

#[test]
fn compute_readiness_openai_keeps_unknown_when_not_probed() {
    // OpenAI is NOT statically known-unsupported, so an un-probed (unknown)
    // status stays "unknown" - Phase B will probe on the first run. This is
    // the path where the toggle is intentionally clickable so the user can
    // trigger the one-time embedding-generation pass.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_provider(&conn, LlmProvider::Openai);
    let r = compute_readiness(&conn, &[]).unwrap();
    assert_eq!(r.embedding_status, "unknown", "OpenAI must NOT override unknown");
    assert!(r.provider_supports_embeddings, "unknown is still clickable");
}

#[test]
fn compute_readiness_anthropic_overrides_persisted_enabled() {
    // The static check is AUTHORITATIVE: even when the persisted status is
    // "enabled" (e.g. the user previously had OpenAI configured + probed,
    // then switched to Anthropic without re-probing), the readiness payload
    // reports "disabled" because Anthropic can never serve embeddings. This
    // protects against stale persisted state, save-debounce races, and the
    // edge case where a prior session left `embedding_status = enabled`
    // behind. The persisted value is NOT mutated here (the probe/runner keep
    // reading it directly); only the readiness payload reflects the static
    // truth.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_provider(&conn, LlmProvider::Anthropic);
    app_settings_repo::set_embedding_status(
        &conn,
        EmbeddingStatus::Enabled,
        "text-embedding-3-small",
        1536,
    )
    .unwrap();
    let r = compute_readiness(&conn, &[]).unwrap();
    assert_eq!(r.embedding_status, "disabled", "static override wins over stale persisted enabled");
    assert!(!r.provider_supports_embeddings);
}

#[test]
fn compute_readiness_anthropic_keeps_persisted_disabled() {
    // When the probe HAS run and persisted "disabled", the static check is a
    // no-op (the value is already disabled).
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_provider(&conn, LlmProvider::Anthropic);
    app_settings_repo::set_embedding_status(&conn, EmbeddingStatus::Disabled, "", 0).unwrap();
    let r = compute_readiness(&conn, &[]).unwrap();
    assert_eq!(r.embedding_status, "disabled");
    assert!(!r.provider_supports_embeddings);
}
