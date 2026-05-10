use rusqlite::{params, Connection};

use crate::crypto::aes_gcm;
use crate::error::AppError;
use crate::models::llm_config::{LlmConfig, LlmProvider};

/// Get config without decrypting the API key.
/// Use this when only `context_window_tokens`, `request_delay_ms`, etc. are needed
/// (e.g., screening readiness checks). Avoids the expensive PBKDF2 key derivation.
pub fn get_config_no_decrypt(conn: &Connection) -> Result<Option<LlmConfig>, AppError> {
    let result = conn.query_row(
        "SELECT provider, endpoint_url, model_name, temperature, \
         skip_temperature, max_concurrent_requests, request_delay_ms, context_window_tokens FROM llm_config WHERE id = 1",
        [],
        |row| {
            let provider_str: String = row.get(0)?;
            let provider = parse_provider(&provider_str);
            Ok(LlmConfig {
                provider,
                endpoint_url: row.get(1)?,
                api_key_encrypted: None, // intentionally skipped — no decryption
                model_name: row.get(2)?,
                temperature: row.get(3)?,
                skip_temperature: row.get::<_, i32>(4)? != 0,
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

pub fn get_config(conn: &Connection) -> Result<Option<LlmConfig>, AppError> {
    let result = conn.query_row(
        "SELECT provider, endpoint_url, api_key_encrypted, model_name, temperature, \
         skip_temperature, max_concurrent_requests, request_delay_ms, context_window_tokens FROM llm_config WHERE id = 1",
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
                skip_temperature: row.get::<_, i32>(5)? != 0,
                max_concurrent_requests: row.get(6)?,
                request_delay_ms: row.get(7)?,
                context_window_tokens: row.get(8)?,
            })
        },
    );

    match result {
        Ok(config) => Ok(Some(config)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

pub fn has_config(conn: &Connection) -> Result<bool, AppError> {
    let count: usize =
        conn.query_row("SELECT EXISTS(SELECT 1 FROM llm_config WHERE id = 1)", [], |row| {
            row.get(0)
        })?;
    Ok(count > 0)
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
         temperature, skip_temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            config.provider.as_str(),
            config.endpoint_url,
            encrypted_api_key,
            config.model_name,
            config.temperature,
            config.skip_temperature as i32,
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
        "anthropic" => LlmProvider::Anthropic,
        "google" => LlmProvider::Google,
        "mistral_ai" => LlmProvider::MistralAi,
        "z_ai" => LlmProvider::ZAi,
        "llama_cpp" => LlmProvider::LlamaCpp,
        "ollama" => LlmProvider::Ollama,
        "lm_studio" => LlmProvider::LmStudio,
        _ => LlmProvider::Custom,
    }
}
