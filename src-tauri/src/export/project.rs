use std::collections::HashMap;

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
    #[serde(default)]
    pub reference_papers: Vec<serde_json::Value>,
    #[serde(default)]
    pub article_reference_links: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_authors: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_article_authors: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_institutions: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_author_affiliations: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_terms: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_article_terms: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_network_meta: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_network_nodes: Vec<serde_json::Value>,
    #[serde(default)]
    pub biblio_network_edges: Vec<serde_json::Value>,
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
    // NOTE: journal_index is NOT exported — it is system-distributed reference data
    // that survives project reset and is populated via the import_journals script.

    let aims = serialize_table(conn, "SELECT * FROM research_aims")?;
    let criteria = serialize_table(conn, "SELECT * FROM criteria")?;
    let articles = serialize_table(conn, "SELECT * FROM articles")?;
    let tags = serialize_table(conn, "SELECT * FROM tags")?;
    let labels = serialize_table(conn, "SELECT * FROM labels")?;
    let article_tags = serialize_table(conn, "SELECT * FROM article_tags")?;
    let article_labels = serialize_table(conn, "SELECT * FROM article_labels")?;
    let audit = serialize_table(conn, "SELECT * FROM audit_entries")?;
    let reference_papers = serialize_table(conn, "SELECT * FROM reference_papers")?;
    let article_reference_links = serialize_table(conn, "SELECT * FROM article_reference_links")?;
    let biblio_authors = serialize_table(conn, "SELECT * FROM biblio_authors")?;
    let biblio_article_authors = serialize_table(conn, "SELECT * FROM biblio_article_authors")?;
    let biblio_institutions = serialize_table(conn, "SELECT * FROM biblio_institutions")?;
    let biblio_author_affiliations =
        serialize_table(conn, "SELECT * FROM biblio_author_affiliations")?;
    let biblio_terms = serialize_table(conn, "SELECT * FROM biblio_terms")?;
    let biblio_article_terms = serialize_table(conn, "SELECT * FROM biblio_article_terms")?;
    let biblio_network_meta = serialize_table(conn, "SELECT * FROM biblio_network_meta")?;
    let biblio_network_nodes = serialize_table(conn, "SELECT * FROM biblio_network_nodes")?;
    let biblio_network_edges = serialize_table(conn, "SELECT * FROM biblio_network_edges")?;

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
        reference_papers,
        article_reference_links,
        biblio_authors,
        biblio_article_authors,
        biblio_institutions,
        biblio_author_affiliations,
        biblio_terms,
        biblio_article_terms,
        biblio_network_meta,
        biblio_network_nodes,
        biblio_network_edges,
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

    // Disable foreign key checks during import.
    // PRAGMA cannot be changed inside a transaction, so set it before starting one.
    // This is safe because we delete all data first, then insert in dependency order.
    conn.execute("PRAGMA foreign_keys = OFF", [])?;

    // Wrap entire import in a transaction for atomicity.
    // If any INSERT fails mid-way, all changes are rolled back so we don't
    // leave the database in a partially-imported state.
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| AppError::Import(format!("Failed to start import transaction: {}", e)))?;

    // NOTE: journal_index is NOT cleared during import — it is system-distributed
    // reference data that survives project reset and backup/restore cycles.

    // Clear existing data (reverse dependency order)
    tx.execute("DELETE FROM biblio_network_edges", [])?;
    tx.execute("DELETE FROM biblio_network_nodes", [])?;
    tx.execute("DELETE FROM biblio_network_meta", [])?;
    tx.execute("DELETE FROM biblio_article_terms", [])?;
    tx.execute("DELETE FROM biblio_terms", [])?;
    tx.execute("DELETE FROM biblio_author_affiliations", [])?;
    tx.execute("DELETE FROM biblio_article_authors", [])?;
    tx.execute("DELETE FROM biblio_institutions", [])?;
    tx.execute("DELETE FROM biblio_authors", [])?;
    tx.execute("DELETE FROM article_reference_links", [])?;
    tx.execute("DELETE FROM reference_papers", [])?;
    tx.execute("DELETE FROM audit_entries", [])?;
    tx.execute("DELETE FROM article_tags", [])?;
    tx.execute("DELETE FROM article_labels", [])?;
    tx.execute("DELETE FROM articles", [])?;
    tx.execute("DELETE FROM criteria", [])?;
    tx.execute("DELETE FROM research_aims", [])?;
    tx.execute("DELETE FROM tags", [])?;
    tx.execute("DELETE FROM labels", [])?;
    tx.execute("DELETE FROM llm_config", [])?;
    // Clear any previously generated summary (it was for different articles)
    tx.execute("DELETE FROM summary", [])?;

    // Restore research aims
    for aim in &backup.research_aims {
        let id = get_str(aim, "id");
        let text = get_str(aim, "text");
        let created_at = get_str_field(aim, "createdAt", "created_at");
        tx.execute(
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
        tx.execute(
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
        tx.execute(
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
        tx.execute(
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
        let eissn = get_str_field(a, "eissn", "eissn");
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
        let changed_at =
            get_str_field(a, "changedAt", "changed_at").unwrap_or_else(|| imported_at.clone());
        let screened_at = get_str_field(a, "screenedAt", "screened_at");
        // Preserve sequence_id from backup; old backups lack it, so assign 1-based index
        let sequence_id = a.get("sequenceId").and_then(|v| v.as_i64()).unwrap_or_else(|| {
            // Old backup - assign based on import order
            (i as i64) + 1
        });
        let full_text = get_str_field(a, "fullText", "full_text");
        let full_text_ai_summary = get_str_field(a, "fullTextAiSummary", "full_text_ai_summary");
        let data_length = a.get("dataLength").and_then(|v| v.as_i64());
        let token_estimate = a.get("tokenEstimate").and_then(|v| v.as_i64());
        let num_cited = a.get("numCited").and_then(|v| v.as_i64());
        let num_references = a.get("numReferences").and_then(|v| v.as_i64());
        let has_citation_details =
            a.get("hasCitationDetails").and_then(|v| v.as_i64()).unwrap_or(0);
        let has_reference_details =
            a.get("hasReferenceDetails").and_then(|v| v.as_i64()).unwrap_or(0);
        let has_full_text = a.get("hasFullText").and_then(|v| v.as_i64()).unwrap_or(0);
        let full_text_file_name = get_str_field(a, "fullTextFileName", "full_text_file_name");
        tx.execute(
            "INSERT INTO articles (
                id, sequence_id, status, screening_error, title, abstract_text, authors, publication_year, doi, journal,
                volume, issue, start_page, end_page, keywords, url, language, publisher, publisher_city,
                publisher_address, issn, eissn, reference_type, date, author_address, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation, notes, web_of_science_db,
                user_notes, ris_extras, duplicate_of, ai_decision, ai_reasoning, ai_confidence,
                matched_inclusion_criteria, matched_exclusion_criteria, manual_override, import_source,
                imported_at, changed_at, screened_at, full_text, full_text_ai_summary,
                data_length, token_estimate, num_cited, num_references,
                has_citation_details, has_reference_details, has_full_text, full_text_file_name
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38,
                ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48, ?49, ?50, ?51, ?52, ?53, ?54
            )",
            rusqlite::params![
                id, sequence_id, status, screening_error, title, abstract_text, authors, publication_year, doi, journal,
                volume, issue, start_page, end_page, keywords, url, language, publisher, publisher_city,
                publisher_address, issn, eissn, reference_type, date, author_address, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation, notes, web_of_science_db,
                user_notes, ris_extras, duplicate_of, ai_decision, ai_reasoning, ai_confidence,
                matched_inclusion_criteria, matched_exclusion_criteria, manual_override, import_source,
                imported_at, changed_at, screened_at, full_text, full_text_ai_summary,
                data_length, token_estimate, num_cited, num_references,
                has_citation_details, has_reference_details, has_full_text, full_text_file_name
            ],
        )?;
    }

    // next_sequence_id() uses SELECT MAX(sequence_id) FROM articles,
    // so it will naturally return the correct value after import - no extra work needed.

    // Restore article_tags
    for at in &backup.article_tags {
        let article_id = get_str(at, "articleId");
        let tag_id = get_str(at, "tagId");
        tx.execute(
            "INSERT INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![article_id, tag_id],
        )?;
    }

    // Restore article_labels
    for al in &backup.article_labels {
        let article_id = get_str(al, "articleId");
        let label_id = get_str(al, "labelId");
        tx.execute(
            "INSERT INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
            rusqlite::params![article_id, label_id],
        )?;
    }

    // Restore reference papers (after articles, before links)
    // Build ID mapping for constraint-violation dedup: backup_id → actual_id
    let mut paper_id_map: HashMap<String, String> = HashMap::new();

    for rp in &backup.reference_papers {
        let id = get_str(rp, "id");
        let title = get_str(rp, "title");
        let abstract_text = get_str_field(rp, "abstractText", "abstract_text").unwrap_or_default();
        let authors =
            serde_json::to_string(&rp.get("authors").cloned().unwrap_or(serde_json::json!([])))
                .unwrap_or_default();
        let publication_year = rp.get("publicationYear").and_then(|v| v.as_i64());
        // Normalize DOI: empty string → None (matches unique constraint)
        let doi: Option<String> =
            get_str_field(rp, "doi", "doi").and_then(|d| if d.is_empty() { None } else { Some(d) });
        let journal = get_str_field(rp, "journal", "journal");
        let volume = get_str_field(rp, "volume", "volume");
        let issue = get_str_field(rp, "issue", "issue");
        let start_page = get_str_field(rp, "startPage", "start_page");
        let end_page = get_str_field(rp, "endPage", "end_page");
        let keywords =
            serde_json::to_string(&rp.get("keywords").cloned().unwrap_or(serde_json::json!([])))
                .unwrap_or_default();
        let url = get_str_field(rp, "url", "url");
        let language = get_str_field(rp, "language", "language");
        let publisher = get_str_field(rp, "publisher", "publisher");
        let publisher_city = get_str_field(rp, "publisherCity", "publisher_city");
        let publisher_address = get_str_field(rp, "publisherAddress", "publisher_address");
        let issn = get_str_field(rp, "issn", "issn");
        let eissn = get_str_field(rp, "eissn", "eissn");
        let reference_type = get_str_field(rp, "referenceType", "reference_type");
        let date = get_str_field(rp, "date", "date");
        let notes = get_str_field(rp, "notes", "notes");
        let ris_extras = serde_json::to_string(
            &rp.get("risExtras")
                .or_else(|| rp.get("ris_extras"))
                .cloned()
                .unwrap_or(serde_json::json!({})),
        )
        .unwrap_or_default();
        let match_status = get_str_field(rp, "matchStatus", "match_status")
            .unwrap_or_else(|| "unmatched".to_string());
        let matched_article_id = get_str_field(rp, "matchedArticleId", "matched_article_id");
        let citation_count = rp
            .get("citationCount")
            .or_else(|| rp.get("citation_count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let reference_count = rp
            .get("referenceCount")
            .or_else(|| rp.get("reference_count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let import_source = get_str_field(rp, "importSource", "import_source");
        let created_at = get_str_field(rp, "createdAt", "created_at")
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let updated_at = get_str_field(rp, "updatedAt", "updated_at")
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        // Try INSERT; on unique constraint violation, find existing record
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO reference_papers (
                id, title, abstract_text, authors, publication_year, doi, journal,
                volume, issue, start_page, end_page, keywords, url, language, publisher,
                publisher_city, publisher_address, issn, eissn, reference_type, date, notes,
                ris_extras, match_status, matched_article_id, citation_count,
                reference_count, import_source, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30
            )",
            rusqlite::params![
                id,
                title,
                abstract_text,
                authors,
                publication_year,
                doi,
                journal,
                volume,
                issue,
                start_page,
                end_page,
                keywords,
                url,
                language,
                publisher,
                publisher_city,
                publisher_address,
                issn,
                eissn,
                reference_type,
                date,
                notes,
                ris_extras,
                match_status,
                matched_article_id,
                citation_count,
                reference_count,
                import_source,
                created_at,
                updated_at
            ],
        )?;

        if inserted == 0 {
            // Constraint violation — find existing record and map IDs
            let existing_id =
                find_existing_paper_id(&tx, doi.as_deref(), &title, &authors, publication_year);
            if let Some(eid) = existing_id {
                paper_id_map.insert(id, eid);
            }
        }
    }

    // Restore article reference links (after both articles and reference_papers)
    // Uses paper_id_map to remap deduplicated reference paper IDs
    for rl in &backup.article_reference_links {
        let id = get_str(rl, "id");
        let parent_article_id =
            get_str_field(rl, "parentArticleId", "parent_article_id").unwrap_or_default();
        let original_paper_id =
            get_str_field(rl, "referencePaperId", "reference_paper_id").unwrap_or_default();
        // Remap to actual paper ID if it was deduplicated
        let reference_paper_id =
            paper_id_map.get(&original_paper_id).map(|s| s.as_str()).unwrap_or(&original_paper_id);
        let ref_type = rl
            .get("referenceType")
            .or_else(|| rl.get("type"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let created_at = get_str_field(rl, "createdAt", "created_at")
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        tx.execute(
            "INSERT INTO article_reference_links (
                id, parent_article_id, reference_paper_id, type, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, parent_article_id, reference_paper_id, ref_type, created_at],
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
        tx.execute(
            "INSERT INTO audit_entries (id, article_id, timestamp, action, from_status, to_status, details, source) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![id, article_id, timestamp, action, from_status, to_status, details, source],
        )?;
    }

    // Restore biblio_authors
    for ba in &backup.biblio_authors {
        let id = get_str(ba, "id");
        let normalized_name =
            get_str_field(ba, "normalizedName", "normalized_name").unwrap_or_default();
        let display_name = get_str_field(ba, "displayName", "display_name").unwrap_or_default();
        let first_author_count = ba
            .get("firstAuthorCount")
            .or_else(|| ba.get("first_author_count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let article_count = ba
            .get("articleCount")
            .or_else(|| ba.get("article_count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let created_at = get_str_field(ba, "createdAt", "created_at")
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        tx.execute(
            "INSERT INTO biblio_authors (id, normalized_name, display_name, first_author_count, article_count, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, normalized_name, display_name, first_author_count, article_count, created_at],
        )?;
    }

    // Restore biblio_institutions
    for bi in &backup.biblio_institutions {
        let id = get_str(bi, "id");
        let normalized_name =
            get_str_field(bi, "normalizedName", "normalized_name").unwrap_or_default();
        let country = get_str_field(bi, "country", "country");
        let city = get_str_field(bi, "city", "city");
        tx.execute(
            "INSERT INTO biblio_institutions (id, normalized_name, country, city) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, normalized_name, country, city],
        )?;
    }

    // Restore biblio_article_authors
    for baa in &backup.biblio_article_authors {
        let id = get_str(baa, "id");
        let article_id = get_str_field(baa, "articleId", "article_id").unwrap_or_default();
        let author_id = get_str_field(baa, "authorId", "author_id").unwrap_or_default();
        let author_order = baa
            .get("authorOrder")
            .or_else(|| baa.get("author_order"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let raw_name = get_str_field(baa, "rawName", "raw_name");
        let raw_affiliation = get_str_field(baa, "rawAffiliation", "raw_affiliation");
        tx.execute(
            "INSERT INTO biblio_article_authors (id, article_id, author_id, author_order, raw_name, raw_affiliation) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, article_id, author_id, author_order, raw_name, raw_affiliation],
        )?;
    }

    // Restore biblio_author_affiliations
    for baf in &backup.biblio_author_affiliations {
        let id = get_str(baf, "id");
        let author_id = get_str_field(baf, "authorId", "author_id").unwrap_or_default();
        let institution_id =
            get_str_field(baf, "institutionId", "institution_id").unwrap_or_default();
        let article_id = get_str_field(baf, "articleId", "article_id").unwrap_or_default();
        tx.execute(
            "INSERT INTO biblio_author_affiliations (id, author_id, institution_id, article_id) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, author_id, institution_id, article_id],
        )?;
    }

    // Restore biblio_terms
    for bt in &backup.biblio_terms {
        let id = get_str(bt, "id");
        let normalized_term =
            get_str_field(bt, "normalizedTerm", "normalized_term").unwrap_or_default();
        let raw_term = get_str_field(bt, "rawTerm", "raw_term").unwrap_or_default();
        let term_type =
            get_str_field(bt, "termType", "term_type").unwrap_or_else(|| "keyword".to_string());
        let article_count = bt
            .get("articleCount")
            .or_else(|| bt.get("article_count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let created_at = get_str_field(bt, "createdAt", "created_at")
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        tx.execute(
            "INSERT INTO biblio_terms (id, normalized_term, raw_term, term_type, article_count, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, normalized_term, raw_term, term_type, article_count, created_at],
        )?;
    }

    // Restore biblio_article_terms
    for bat in &backup.biblio_article_terms {
        let id = get_str(bat, "id");
        let article_id = get_str_field(bat, "articleId", "article_id").unwrap_or_default();
        let term_id = get_str_field(bat, "termId", "term_id").unwrap_or_default();
        let frequency = bat.get("frequency").and_then(|v| v.as_i64()).unwrap_or(1);
        tx.execute(
            "INSERT INTO biblio_article_terms (id, article_id, term_id, frequency) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, article_id, term_id, frequency],
        )?;
    }

    // Restore biblio_network_meta
    for bnm in &backup.biblio_network_meta {
        let id = get_str(bnm, "id");
        let network_type = get_str_field(bnm, "networkType", "network_type").unwrap_or_default();
        let label = get_str(bnm, "label");
        let article_filter = get_str_field(bnm, "articleFilter", "article_filter");
        let params_json = get_str_field(bnm, "paramsJson", "params_json");
        let node_count = bnm
            .get("nodeCount")
            .or_else(|| bnm.get("node_count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let edge_count = bnm
            .get("edgeCount")
            .or_else(|| bnm.get("edge_count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let created_at = get_str_field(bnm, "createdAt", "created_at")
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        tx.execute(
            "INSERT INTO biblio_network_meta (id, network_type, label, article_filter, params_json, node_count, edge_count, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![id, network_type, label, article_filter, params_json, node_count, edge_count, created_at],
        )?;
    }

    // Restore biblio_network_nodes
    for bnn in &backup.biblio_network_nodes {
        let id = get_str(bnn, "id");
        let network_id = get_str_field(bnn, "networkId", "network_id").unwrap_or_default();
        let entity_id = get_str_field(bnn, "entityId", "entity_id").unwrap_or_default();
        let label = get_str(bnn, "label");
        let weight = bnn.get("weight").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let cluster: Option<i64> = bnn.get("cluster").and_then(|v| v.as_i64());
        let x: Option<f64> = bnn.get("x").and_then(|v| v.as_f64());
        let y: Option<f64> = bnn.get("y").and_then(|v| v.as_f64());
        tx.execute(
            "INSERT INTO biblio_network_nodes (id, network_id, entity_id, label, weight, cluster, x, y) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![id, network_id, entity_id, label, weight, cluster, x, y],
        )?;
    }

    // Restore biblio_network_edges
    for bne in &backup.biblio_network_edges {
        let id = get_str(bne, "id");
        let network_id = get_str_field(bne, "networkId", "network_id").unwrap_or_default();
        let source_id = get_str_field(bne, "sourceId", "source_id").unwrap_or_default();
        let target_id = get_str_field(bne, "targetId", "target_id").unwrap_or_default();
        let weight = bne.get("weight").and_then(|v| v.as_f64()).unwrap_or(0.0);
        tx.execute(
            "INSERT INTO biblio_network_edges (id, network_id, source_id, target_id, weight) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, network_id, source_id, target_id, weight],
        )?;
    }

    // Restore LLM config (without keys)
    if let Some(ref llm_backup) = backup.llm_config {
        tx.execute(
            "INSERT INTO llm_config (id, provider, endpoint_url, model_name, \
             temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) \
             VALUES (1, ?1, ?2, ?3, 0.2, 3, 500, 50000)",
            rusqlite::params![llm_backup.provider, llm_backup.endpoint_url, llm_backup.model_name],
        )?;
    }

    tx.commit()
        .map_err(|e| AppError::Import(format!("Failed to commit import transaction: {}", e)))?;

    // Re-enable foreign key checks after import
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    // Post-import: resolve journal links for imported articles & reference papers.
    // Backup files don't store journal_index_id (it's derived from ISSN/eISSN/journal name),
    // so we rematch against the journal_index table.
    let _ = crate::db::article_repo::rematch_all_journals(conn);
    let _ = crate::db::reference_repo::rematch_all_journals(conn);

    Ok(())
}

fn get_str(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn get_str_field(v: &serde_json::Value, camel: &str, snake: &str) -> Option<String> {
    v.get(camel).or_else(|| v.get(snake)).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Find an existing reference paper ID by DOI or title+authors+year.
/// Used during import to remap links when INSERT OR IGNORE skips a duplicate.
fn find_existing_paper_id(
    conn: &Connection,
    doi: Option<&str>,
    title: &str,
    authors: &str,
    publication_year: Option<i64>,
) -> Option<String> {
    // Try DOI first
    if let Some(doi) = doi {
        let result: Option<String> = conn
            .query_row("SELECT id FROM reference_papers WHERE doi = ?1 LIMIT 1", [doi], |row| {
                row.get(0)
            })
            .ok();
        if let Some(id) = result {
            return Some(id);
        }
    }
    // Then title + authors + year
    match publication_year {
        Some(y) => conn.query_row(
            "SELECT id FROM reference_papers WHERE LOWER(title) = LOWER(?1) AND authors = ?2 AND publication_year = ?3 LIMIT 1",
            rusqlite::params![title, authors, y],
            |row| row.get::<_, String>(0),
        ).ok(),
        None => conn.query_row(
            "SELECT id FROM reference_papers WHERE LOWER(title) = LOWER(?1) AND authors = ?2 LIMIT 1",
            rusqlite::params![title, authors],
            |row| row.get::<_, String>(0),
        ).ok(),
    }
}
