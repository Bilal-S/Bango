use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::crypto::aes_gcm;
use crate::db::llm_config_repo;
use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportMetadata {
    pub spec_version: String,
    pub exported_at: String,
    pub app_name: String,
    pub app_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBackup {
    pub metadata: ExportMetadata,
    pub research_aims: Vec<serde_json::Value>,
    pub criteria: Vec<serde_json::Value>,
    pub articles: Vec<serde_json::Value>,
    pub tags: Vec<serde_json::Value>,
    pub labels: Vec<serde_json::Value>,
    pub audit_entries: Vec<serde_json::Value>,
    pub llm_config: Option<LlmConfigBackup>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigBackup {
    pub provider: String,
    pub endpoint_url: String,
    pub model_name: String,
    pub api_key_encrypted: Option<String>,
}

pub fn export_project(conn: &Connection, password: &str) -> Result<String, AppError> {
    let aims = serialize_table(conn, "SELECT * FROM research_aims")?;
    let criteria = serialize_table(conn, "SELECT * FROM criteria")?;
    let articles = serialize_table(conn, "SELECT * FROM articles")?;
    let tags = serialize_table(conn, "SELECT * FROM tags")?;
    let labels = serialize_table(conn, "SELECT * FROM labels")?;
    let audit = serialize_table(conn, "SELECT * FROM audit_entries")?;

    let llm_backup = llm_config_repo::get_config(conn)?.map(|c| {
        let key = aes_gcm::derive_key_from_password(password);
        let encrypted_key =
            c.api_key_encrypted.as_ref().and_then(|k| aes_gcm::encrypt(k.as_bytes(), &key).ok());
        LlmConfigBackup {
            provider: c.provider.as_str().to_string(),
            endpoint_url: c.endpoint_url,
            model_name: c.model_name,
            api_key_encrypted: encrypted_key,
        }
    });

    let backup = ProjectBackup {
        metadata: ExportMetadata {
            spec_version: "3.0".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            app_name: "Bango".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        research_aims: aims,
        criteria,
        articles,
        tags,
        labels,
        audit_entries: audit,
        llm_config: llm_backup,
    };

    serde_json::to_string_pretty(&backup).map_err(AppError::Serialization)
}

fn serialize_table(conn: &Connection, query: &str) -> Result<Vec<serde_json::Value>, AppError> {
    let mut stmt = conn.prepare(query)?;
    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    let rows = stmt.query_map([], |row| {
        let mut map = serde_json::Map::new();
        for (i, name) in column_names.iter().enumerate() {
            let value: serde_json::Value = match row.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::json!(n),
                Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                Ok(rusqlite::types::ValueRef::Text(s)) => {
                    let text = String::from_utf8_lossy(s).to_string();
                    // Try to parse as JSON first
                    serde_json::from_str::<serde_json::Value>(&text)
                        .unwrap_or_else(|_| serde_json::json!(text))
                }
                Ok(rusqlite::types::ValueRef::Blob(_)) => serde_json::Value::Null,
                Err(_) => serde_json::Value::Null,
            };
            // Convert snake_case column names to camelCase for consistency
            let camel_name = to_camel_case(name);
            map.insert(camel_name, value);
        }
        Ok(serde_json::Value::Object(map))
    })?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn to_camel_case(s: &str) -> String {
    let parts: Vec<&str> = s.split('_').collect();
    if parts.len() == 1 {
        return s.to_string();
    }
    let mut result = parts[0].to_string();
    for part in &parts[1..] {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());
            result.extend(chars);
        }
    }
    result
}

pub fn import_project(conn: &Connection, json_str: &str, password: &str) -> Result<(), AppError> {
    let backup: ProjectBackup = serde_json::from_str(json_str)
        .map_err(|e| AppError::Import(format!("Invalid backup file: {}", e)))?;

    // Check spec version
    let version: i32 =
        backup.metadata.spec_version.split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
    if version > 3 {
        return Err(AppError::Import(format!(
            "Backup is spec version {} but this app supports version 3. Some data may not import correctly.",
            backup.metadata.spec_version
        )));
    }

    // Clear existing data (reverse dependency order)
    conn.execute("DELETE FROM audit_entries", [])?;
    conn.execute("DELETE FROM article_tags", [])?;
    conn.execute("DELETE FROM article_labels", [])?;
    conn.execute("DELETE FROM articles", [])?;
    conn.execute("DELETE FROM criteria", [])?;
    conn.execute("DELETE FROM research_aims", [])?;
    conn.execute("DELETE FROM tags", [])?;
    conn.execute("DELETE FROM labels", [])?;
    conn.execute("DELETE FROM llm_config", [])?;

    // Restore research aims
    for aim in &backup.research_aims {
        let id = get_str(aim, "id");
        let text = get_str(aim, "text");
        let created_at = get_str_field(aim, "createdAt", "created_at");
        conn.execute(
            "INSERT INTO research_aims (id, text, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, text, created_at],
        )?;
    }

    // Restore criteria
    for c in &backup.criteria {
        let id = get_str(c, "id");
        let ctype = get_str_field(c, "criterionType", "type")
            .or_else(|| get_str_field(c, "type", "type"))
            .unwrap_or_else(|| "inclusion".to_string());
        let text = get_str(c, "text");
        let priority = {
            let p = get_str(c, "priority");
            if p.is_empty() {
                "standard".to_string()
            } else {
                p
            }
        };
        let created_at = get_str_field(c, "createdAt", "created_at");
        conn.execute(
            "INSERT INTO criteria (id, type, text, priority, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, ctype, text, priority, created_at],
        )?;
    }

    // Restore tags
    for t in &backup.tags {
        let id = get_str(t, "id");
        let name = get_str(t, "name");
        let source = {
            let s = get_str(t, "source");
            if s.is_empty() {
                "user_created".to_string()
            } else {
                s
            }
        };
        conn.execute(
            "INSERT INTO tags (id, name, source) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, source],
        )?;
    }

    // Restore labels
    for l in &backup.labels {
        let id = get_str(l, "id");
        let name = get_str(l, "name");
        let source = {
            let s = get_str(l, "source");
            if s.is_empty() {
                "user_created".to_string()
            } else {
                s
            }
        };
        conn.execute(
            "INSERT INTO labels (id, name, source) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, source],
        )?;
    }

    // Restore articles
    for a in &backup.articles {
        let id = get_str(a, "id");
        let status = get_str(a, "status");
        let title = get_str(a, "title");
        let abstract_text = get_str_field(a, "abstractText", "abstract_text");
        let authors_json =
            serde_json::to_string(&a.get("authors").cloned().unwrap_or(serde_json::json!([])))
                .unwrap_or_default();
        let import_source = get_str_field(a, "importSource", "import_source");

        conn.execute(
            "INSERT INTO articles (id, status, title, abstract_text, authors, import_source, imported_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
            rusqlite::params![id, status, title, abstract_text, authors_json, import_source],
        )?;
    }

    // Restore LLM config with decrypted key
    if let Some(ref llm_backup) = backup.llm_config {
        let key = aes_gcm::derive_key_from_password(password);
        let decrypted_key = llm_backup
            .api_key_encrypted
            .as_ref()
            .and_then(|enc| aes_gcm::decrypt(enc, &key).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok());

        let machine_key = aes_gcm::derive_key_from_machine();
        let re_encrypted = decrypted_key
            .as_ref()
            .map(|k| aes_gcm::encrypt(k.as_bytes(), &machine_key))
            .transpose()
            .ok()
            .flatten();

        conn.execute(
            "INSERT INTO llm_config (id, provider, endpoint_url, api_key_encrypted, model_name, \
             temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) \
             VALUES (1, ?1, ?2, ?3, ?4, 0.2, 3, 500, 50000)",
            rusqlite::params![
                llm_backup.provider,
                llm_backup.endpoint_url,
                re_encrypted,
                llm_backup.model_name
            ],
        )?;
    }

    Ok(())
}

fn get_str(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn get_str_field(v: &serde_json::Value, camel: &str, snake: &str) -> Option<String> {
    v.get(camel).or_else(|| v.get(snake)).and_then(|v| v.as_str()).map(|s| s.to_string())
}
