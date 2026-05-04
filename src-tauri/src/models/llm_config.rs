use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub endpoint_url: String,
    pub api_key_encrypted: Option<String>,
    pub model_name: String,
    pub temperature: f64,
    pub max_concurrent_requests: i32,
    pub request_delay_ms: i32,
    pub context_window_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LlmProvider {
    Openai,
    Google,
    ZAi,
    LlamaCpp,
    Ollama,
    LmStudio,
    Custom,
}

impl LlmProvider {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Google => "google",
            Self::ZAi => "z_ai",
            Self::LlamaCpp => "llama_cpp",
            Self::Ollama => "ollama",
            Self::LmStudio => "lm_studio",
            Self::Custom => "custom",
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Openai,
            endpoint_url: String::new(),
            api_key_encrypted: None,
            model_name: String::new(),
            temperature: 0.2,
            max_concurrent_requests: 3,
            request_delay_ms: 500,
            context_window_tokens: 50_000,
        }
    }
}
