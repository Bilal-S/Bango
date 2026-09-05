//! Coverage for small pure model modules: as_str() / Display / Default impls.
use bango_lib::models::article::{AiDecision, ArticleStatus};
use bango_lib::models::criterion::{CriterionType, Priority};
use bango_lib::models::label::LabelSource;
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};
use bango_lib::models::tag::TagSource;

#[test]
fn article_status_as_str_and_display() {
    assert_eq!(ArticleStatus::Duplicate.as_str(), "duplicate");
    assert_eq!(ArticleStatus::Working.as_str(), "working");
    assert_eq!(ArticleStatus::Included.as_str(), "included");
    assert_eq!(ArticleStatus::Rejected.as_str(), "rejected");
    assert_eq!(format!("{}", ArticleStatus::Included), "included");
}

#[test]
fn ai_decision_as_str() {
    assert_eq!(AiDecision::Include.as_str(), "include");
    assert_eq!(AiDecision::Exclude.as_str(), "exclude");
}

#[test]
fn criterion_type_as_str() {
    assert_eq!(CriterionType::Inclusion.as_str(), "inclusion");
    assert_eq!(CriterionType::Exclusion.as_str(), "exclusion");
}

#[test]
fn priority_as_str_and_ordering() {
    assert_eq!(Priority::Critical.as_str(), "critical");
    assert_eq!(Priority::High.as_str(), "high");
    assert_eq!(Priority::Standard.as_str(), "standard");
    assert_eq!(Priority::Low.as_str(), "low");
    assert_eq!(Priority::Optional.as_str(), "optional");
    assert!(Priority::Critical > Priority::Optional);
}

#[test]
fn label_source_as_str() {
    assert_eq!(LabelSource::AiGenerated.as_str(), "ai_generated");
    assert_eq!(LabelSource::UserCreated.as_str(), "user_created");
}

#[test]
fn tag_source_as_str() {
    assert_eq!(TagSource::AiSuggested.as_str(), "ai_suggested");
    assert_eq!(TagSource::RisKeyword.as_str(), "ris_keyword");
    assert_eq!(TagSource::UserCreated.as_str(), "user_created");
}

#[test]
fn llm_provider_as_str_all_variants() {
    assert_eq!(LlmProvider::Openai.as_str(), "openai");
    assert_eq!(LlmProvider::Anthropic.as_str(), "anthropic");
    assert_eq!(LlmProvider::Google.as_str(), "google");
    assert_eq!(LlmProvider::MistralAi.as_str(), "mistral_ai");
    assert_eq!(LlmProvider::ZAi.as_str(), "z_ai");
    assert_eq!(LlmProvider::LlamaCpp.as_str(), "llama_cpp");
    assert_eq!(LlmProvider::Ollama.as_str(), "ollama");
    assert_eq!(LlmProvider::LmStudio.as_str(), "lm_studio");
    assert_eq!(LlmProvider::Custom.as_str(), "custom");
}

#[test]
fn llm_config_default_values() {
    let cfg = LlmConfig::default();
    assert!(matches!(cfg.provider, LlmProvider::Openai));
    assert_eq!(cfg.temperature, 0.2);
    assert!(!cfg.skip_temperature);
    assert_eq!(cfg.max_concurrent_requests, 3);
    assert_eq!(cfg.request_delay_ms, 500);
    assert_eq!(cfg.context_window_tokens, 50_000);
    assert!(cfg.api_key_encrypted.is_none());
}

#[test]
fn models_serialize_round_trip() {
    // Verifies Serialize/Deserialize derives produce camelCase JSON.
    let status = ArticleStatus::Included;
    let json = serde_json::to_string(&status).expect("serialize");
    assert!(json.contains("included"));
    let back: ArticleStatus = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(back, ArticleStatus::Included));

    let ct = CriterionType::Exclusion;
    let json = serde_json::to_string(&ct).expect("serialize");
    let back: CriterionType = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(back, CriterionType::Exclusion));
}
