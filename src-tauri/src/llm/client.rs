use crate::error::AppError;
use crate::models::llm_config::LlmConfig;

pub async fn send_chat_completion(
    _config: &LlmConfig,
    _system_prompt: &str,
    _user_prompt: &str,
) -> Result<String, AppError> {
    // Will be fully implemented in Plan 5/6
    Err(AppError::Validation("LLM client not yet implemented".to_string()))
}
