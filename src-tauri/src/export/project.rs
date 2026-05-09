use rusqlite::Connection;
use serde::{Deserialize, Serialize};

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
    pub article_tags: Vec<serde_json::Value>,
    pub article_labels: Vec<serde_json::Value>,
    pub audit_entries: Vec<serde_json::Value>,
    pub llm_config: Option<LlmConfigBackup>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigBackup {
    pub provider: String,
    pub endpoint_url: String,
    pub model_name: String,
}

pub fn export_project(conn: &Connection) -> Result<String, AppError> {
    let aims = serialize_table(conn, "SELECT * FROM research_aims")?;
    let criteria = serialize_table(conn, "SELECT * FROM criteria")?;
    let articles = serialize_table(conn, "SELECT * FROM articles")?;
    let tags = serialize_table(conn, "SELECT * FROM tags")?;
    let labels = serialize_table(conn, "SELECT * FROM labels")?;
    let article_tags = serialize_table(conn, "SELECT * FROM article_tags")?;
    let article_labels = serialize_table(conn, "SELECT * FROM article_labels")?;
    let audit = serialize_table(conn, "SELECT * FROM audit_entries")?;

    let llm_backup = llm_config_repo::get_config(conn)?.map(|c| LlmConfigBackup {
        provider: c.provider.as_str().to_string(),
        endpoint_url: c.endpoint_url,
        model_name: c.model_name,
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
        article_tags,
        article_labels,
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

pub fn import_project(conn: &Connection, json_str: &str) -> Result<(), AppError> {
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
    for (i, a) in backup.articles.iter().enumerate() {
        let id = get_str(a, "id");
        let status = get_str(a, "status");
        let screening_error = a.get("screeningError").and_then(|v| v.as_i64()).unwrap_or(0);
        let title = get_str(a, "title");
        let abstract_text = get_str_field(a, "abstractText", "abstract_text");
        let authors =
            serde_json::to_string(&a.get("authors").cloned().unwrap_or(serde_json::json!([])))
                .unwrap_or_default();
        let publication_year = a.get("publicationYear").and_then(|v| v.as_i64());
        let doi = get_str_field(a, "doi", "doi");
        let journal = get_str_field(a, "journal", "journal");
        let volume = get_str_field(a, "volume", "volume");
        let issue = get_str_field(a, "issue", "issue");
        let start_page = get_str_field(a, "startPage", "start_page");
        let end_page = get_str_field(a, "endPage", "end_page");
        let keywords =
            serde_json::to_string(&a.get("keywords").cloned().unwrap_or(serde_json::json!([])))
                .unwrap_or_default();
        let url = get_str_field(a, "url", "url");
        let language = get_str_field(a, "language", "language");
        let publisher = get_str_field(a, "publisher", "publisher");
        let publisher_city = get_str_field(a, "publisherCity", "publisher_city");
        let publisher_address = get_str_field(a, "publisherAddress", "publisher_address");
        let issn = get_str_field(a, "issn", "issn");
        let reference_type = get_str_field(a, "referenceType", "reference_type");
        let date = get_str_field(a, "date", "date");
        let author_address = get_str_field(a, "authorAddress", "author_address");
        let accession_number = get_str_field(a, "accessionNumber", "accession_number");
        let custom_field3 = get_str_field(a, "customField3", "custom_field3");
        let journal_abbreviation = get_str_field(a, "journalAbbreviation", "journal_abbreviation");
        let journal_iso_abbreviation =
            get_str_field(a, "journalIsoAbbreviation", "journal_iso_abbreviation");
        let notes = get_str_field(a, "notes", "notes");
        let web_of_science_db = get_str_field(a, "webOfScienceDb", "web_of_science_db");
        let user_notes = get_str_field(a, "userNotes", "user_notes");
        let ris_extras =
            serde_json::to_string(&a.get("risExtras").cloned().unwrap_or(serde_json::json!({})))
                .unwrap_or_default();
        let duplicate_of = get_str_field(a, "duplicateOf", "duplicate_of");
        let ai_decision = get_str_field(a, "aiDecision", "ai_decision");
        let ai_reasoning = get_str_field(a, "aiReasoning", "ai_reasoning");
        let ai_confidence = a.get("aiConfidence").and_then(|v| v.as_f64());
        let matched_inclusion_criteria = serde_json::to_string(
            &a.get("matchedInclusionCriteria").cloned().unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        let matched_exclusion_criteria = serde_json::to_string(
            &a.get("matchedExclusionCriteria").cloned().unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        let manual_override = a.get("manualOverride").and_then(|v| v.as_i64()).unwrap_or(0);
        let import_source = get_str_field(a, "importSource", "import_source");
        let imported_at = get_str_field(a, "importedAt", "imported_at")
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let screened_at = get_str_field(a, "screenedAt", "screened_at");
        // Preserve sequence_id from backup; old backups lack it, so assign 1-based index
        let sequence_id = a
            .get("sequenceId")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| {
                // Old backup — assign based on import order
                let sid = (i as i64) + 1;
                sid
            });
        conn.execute(
            "INSERT INTO articles (
                id, sequence_id, status, screening_error, title, abstract_text, authors, publication_year, doi, journal,
                volume, issue, start_page, end_page, keywords, url, language, publisher, publisher_city,
                publisher_address, issn, reference_type, date, author_address, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation, notes, web_of_science_db,
                user_notes, ris_extras, duplicate_of, ai_decision, ai_reasoning, ai_confidence,
                matched_inclusion_criteria, matched_exclusion_criteria, manual_override, import_source,
                imported_at, screened_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38,
                ?39, ?40, ?41, ?42
            )",
            rusqlite::params![
                id, sequence_id, status, screening_error, title, abstract_text, authors, publication_year, doi, journal,
                volume, issue, start_page, end_page, keywords, url, language, publisher, publisher_city,
                publisher_address, issn, reference_type, date, author_address, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation, notes, web_of_science_db,
                user_notes, ris_extras, duplicate_of, ai_decision, ai_reasoning, ai_confidence,
                matched_inclusion_criteria, matched_exclusion_criteria, manual_override, import_source,
                imported_at, screened_at
            ],
        )?;
    }

    // next_sequence_id() uses SELECT MAX(sequence_id) FROM articles,
    // so it will naturally return the correct value after import — no extra work needed.

    // Restore article_tags
    for at in &backup.article_tags {
        let article_id = get_str(at, "articleId");
        let tag_id = get_str(at, "tagId");
        conn.execute(
            "INSERT INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![article_id, tag_id],
        )?;
    }

    // Restore article_labels
    for al in &backup.article_labels {
        let article_id = get_str(al, "articleId");
        let label_id = get_str(al, "labelId");
        conn.execute(
            "INSERT INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
            rusqlite::params![article_id, label_id],
        )?;
    }

    // Restore audit entries
    for ae in &backup.audit_entries {
        let id = get_str(ae, "id");
        let article_id = get_str(ae, "articleId");
        let timestamp = get_str(ae, "timestamp");
        let action = get_str(ae, "action");
        let from_status = get_str_field(ae, "fromStatus", "from_status");
        let to_status = get_str_field(ae, "toStatus", "to_status");
        let details = get_str(ae, "details");
        let source = get_str(ae, "source");
        conn.execute(
            "INSERT INTO audit_entries (id, article_id, timestamp, action, from_status, to_status, details, source) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![id, article_id, timestamp, action, from_status, to_status, details, source],
        )?;
    }

    // Restore LLM config (without keys)
    if let Some(ref llm_backup) = backup.llm_config {
        conn.execute(
            "INSERT INTO llm_config (id, provider, endpoint_url, model_name, \
             temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) \
             VALUES (1, ?1, ?2, ?3, 0.2, 3, 500, 50000)",
            rusqlite::params![llm_backup.provider, llm_backup.endpoint_url, llm_backup.model_name],
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
