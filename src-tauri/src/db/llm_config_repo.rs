use rusqlite::{params, Connection};

use crate::crypto::aes_gcm;
use crate::error::AppError;
use crate::models::llm_config::{LlmConfig, LlmProvider};

pub fn get_config(conn: &Connection) -> Result<Option<LlmConfig>, AppError> {
    let result = conn.query_row(
        "SELECT provider, endpoint_url, api_key_encrypted, model_name, temperature, \
         max_concurrent_requests, request_delay_ms, context_window_tokens FROM llm_config WHERE id = 1",
        [],
        |row| {
            let provider_str: String = row.get(0)?;
            let provider = parse_provider(&provider_str);
            let api_key_encrypted: Option<String> = row.get(2)?;

            // Decrypt API key using machine-derived key
            let api_key_decrypted = api_key_encrypted.and_then(|enc| {
                let key = aes_gcm::derive_key_from_machine();
                aes_gcm::decrypt(&enc, &key).ok().and_then(|bytes| String::from_utf8(bytes).ok())
            });

            Ok(LlmConfig {
                provider,
                endpoint_url: row.get(1)?,
                api_key_encrypted: api_key_decrypted,
                model_name: row.get(3)?,
                temperature: row.get(4)?,
                max_concurrent_requests: row.get(5)?,
                request_delay_ms: row.get(6)?,
                context_window_tokens: row.get(7)?,
            })
        },
    );

    match result {
        Ok(config) => Ok(Some(config)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

pub fn save_config(conn: &Connection, config: &LlmConfig) -> Result<(), AppError> {
    let key = aes_gcm::derive_key_from_machine();
    let encrypted_api_key = config
        .api_key_encrypted
        .as_ref()
        .map(|k| aes_gcm::encrypt(k.as_bytes(), &key))
        .transpose()
        .map_err(|_| AppError::Validation("Failed to encrypt API key".to_string()))?;

    conn.execute("DELETE FROM llm_config WHERE id = 1", [])?;

    conn.execute(
        "INSERT INTO llm_config (id, provider, endpoint_url, api_key_encrypted, model_name, \
         temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            config.provider.as_str(),
            config.endpoint_url,
            encrypted_api_key,
            config.model_name,
            config.temperature,
            config.max_concurrent_requests,
            config.request_delay_ms,
            config.context_window_tokens,
        ],
    )?;

    Ok(())
}

fn parse_provider(s: &str) -> LlmProvider {
    match s {
        "openai" => LlmProvider::Openai,
        "google" => LlmProvider::Google,
        "z_ai" => LlmProvider::ZAi,
        "llama_cpp" => LlmProvider::LlamaCpp,
        "ollama" => LlmProvider::Ollama,
        "lm_studio" => LlmProvider::LmStudio,
        _ => LlmProvider::Custom,
    }
}
