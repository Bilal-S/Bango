//! Coverage for db::llm_config_repo (get/save/has_config, no-decrypt path).
use bango_lib::db::connection::create_connection;
use bango_lib::db::llm_config_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};

fn cfg(provider: LlmProvider, endpoint: &str, model: &str, key: Option<&str>) -> LlmConfig {
    LlmConfig {
        provider,
        endpoint_url: endpoint.to_string(),
        api_key_encrypted: key.map(String::from),
        model_name: model.to_string(),
        temperature: 0.5,
        skip_temperature: true,
        max_concurrent_requests: 7,
        request_delay_ms: 1200,
        context_window_tokens: 32_000,
    }
}

#[test]
fn get_config_returns_none_when_empty() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    assert!(llm_config_repo::get_config(&conn).expect("get").is_none());
    assert!(llm_config_repo::get_config_no_decrypt(&conn).expect("get nd").is_none());
    assert!(!llm_config_repo::has_config(&conn).expect("has"));
}

#[test]
fn save_then_get_round_trips_cloud_provider_with_key() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    let original = cfg(LlmProvider::Openai, "https://api.openai.com", "gpt-4", Some("sk-test"));
    llm_config_repo::save_config(&conn, &original).expect("save");

    let fetched = llm_config_repo::get_config(&conn).expect("get").expect("some");
    assert!(matches!(fetched.provider, LlmProvider::Openai));
    assert_eq!(fetched.endpoint_url, "https://api.openai.com");
    assert_eq!(fetched.model_name, "gpt-4");
    // Decrypted key should round-trip
    assert_eq!(fetched.api_key_encrypted.as_deref(), Some("sk-test"));
    assert_eq!(fetched.temperature, 0.5);
    assert!(fetched.skip_temperature);
    assert_eq!(fetched.max_concurrent_requests, 7);
    assert_eq!(fetched.request_delay_ms, 1200);
    assert_eq!(fetched.context_window_tokens, 32_000);

    assert!(llm_config_repo::has_config(&conn).expect("has"));
}

#[test]
fn get_config_no_decrypt_omits_key() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    let original = cfg(LlmProvider::Anthropic, "https://api.anthropic.com", "claude", Some("key"));
    llm_config_repo::save_config(&conn, &original).expect("save");

    let fetched = llm_config_repo::get_config_no_decrypt(&conn).expect("get nd").expect("some");
    assert!(matches!(fetched.provider, LlmProvider::Anthropic));
    assert!(fetched.api_key_encrypted.is_none(), "no-decrypt must not populate key");
    assert_eq!(fetched.model_name, "claude");
}

#[test]
fn has_config_local_providers_do_not_require_key() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    // Ollama with no key but with endpoint+model => has_config true
    let local = cfg(LlmProvider::Ollama, "http://localhost:11434", "llama3", None);
    llm_config_repo::save_config(&conn, &local).expect("save");
    assert!(llm_config_repo::has_config(&conn).expect("has ollama"));

    // LmStudio
    let local2 = cfg(LlmProvider::LmStudio, "http://localhost:1234", "qwen", None);
    llm_config_repo::save_config(&conn, &local2).expect("save2");
    assert!(llm_config_repo::has_config(&conn).expect("has lm studio"));

    // LlamaCpp
    let local3 = cfg(LlmProvider::LlamaCpp, "http://localhost:8080", "gguf", None);
    llm_config_repo::save_config(&conn, &local3).expect("save3");
    assert!(llm_config_repo::has_config(&conn).expect("has llama_cpp"));
}

#[test]
fn has_config_false_when_endpoint_empty() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    let bad = cfg(LlmProvider::Openai, "", "gpt-4", Some("k"));
    llm_config_repo::save_config(&conn, &bad).expect("save");
    assert!(!llm_config_repo::has_config(&conn).expect("has"));
}

#[test]
fn has_config_false_when_model_empty() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    let bad = cfg(LlmProvider::Openai, "https://api.openai.com", "", Some("k"));
    llm_config_repo::save_config(&conn, &bad).expect("save");
    assert!(!llm_config_repo::has_config(&conn).expect("has"));
}

#[test]
fn save_overwrites_previous_config() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    let first = cfg(LlmProvider::Openai, "https://a", "m1", Some("k1"));
    llm_config_repo::save_config(&conn, &first).expect("save1");
    let second = cfg(LlmProvider::Google, "https://b", "m2", Some("k2"));
    llm_config_repo::save_config(&conn, &second).expect("save2");

    let fetched = llm_config_repo::get_config(&conn).expect("get").expect("some");
    assert!(matches!(fetched.provider, LlmProvider::Google));
    assert_eq!(fetched.endpoint_url, "https://b");
    assert_eq!(fetched.api_key_encrypted.as_deref(), Some("k2"));
}

#[test]
fn save_without_api_key_stores_none() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    let no_key = cfg(LlmProvider::Custom, "https://custom", "m", None);
    llm_config_repo::save_config(&conn, &no_key).expect("save");

    let fetched = llm_config_repo::get_config(&conn).expect("get").expect("some");
    assert!(fetched.api_key_encrypted.is_none());
    // Custom provider with no key => has_config false (not local, no key)
    assert!(!llm_config_repo::has_config(&conn).expect("has"));
}

#[test]
fn all_provider_variants_round_trip() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    for (provider, endpoint) in [
        (LlmProvider::MistralAi, "https://api.mistral.ai"),
        (LlmProvider::ZAi, "https://api.z.ai"),
        (LlmProvider::Custom, "https://my.proxy"),
    ] {
        let c = cfg(provider.clone(), endpoint, "model-x", Some("k"));
        llm_config_repo::save_config(&conn, &c).expect("save");
        let fetched = llm_config_repo::get_config(&conn).expect("get").expect("some");
        assert!(
            fetched.provider.as_str() == provider.as_str(),
            "provider {} round-trip",
            provider.as_str()
        );
        assert_eq!(fetched.endpoint_url, endpoint);
    }
}
