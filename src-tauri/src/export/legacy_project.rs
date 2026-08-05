//! Reads the legacy `article_references` table and emits a current-format
//! `ProjectBackup` JSON. Rows are de-duplicated into normalized
//! `reference_papers` (by DOI → title+authors+year) and re-emitted as
//! `article_reference_links`. Feeds directly into `project::import_project`.

use std::collections::HashMap;

use rusqlite::Connection;

use super::project::{ExportMetadata, ProjectBackup};
use crate::error::AppError;

/// `Option<String>` to JSON: None → Null, Some(s) → string.
#[inline]
fn opt_str_to_value(opt: Option<String>) -> serde_json::Value {
    match opt {
        Some(s) => serde_json::Value::String(s),
        None => serde_json::Value::Null,
    }
}

/// Convert an `Option<i64>` into a JSON value: `None` -> Null, `Some(n)` -> int.
#[inline]
fn opt_i64_to_value(opt: Option<i64>) -> serde_json::Value {
    match opt {
        Some(n) => serde_json::Value::from(n),
        None => serde_json::Value::Null,
    }
}

/// Export a legacy-schema database to the current `ProjectBackup` JSON format.
pub fn export_legacy_project(conn: &Connection) -> Result<String, AppError> {
    let aims = opt_table(conn, "research_aims")?;
    let criteria = opt_table(conn, "criteria")?;
    let articles = opt_table(conn, "articles")?;
    let tags = opt_table(conn, "tags")?;
    let labels = opt_table(conn, "labels")?;
    let article_tags = opt_table(conn, "article_tags")?;
    let article_labels = opt_table(conn, "article_labels")?;
    let audit = opt_table(conn, "audit_entries")?;

    let (reference_papers, article_reference_links) = if table_exists(conn, "article_references") {
        dedup_legacy_references(conn)?
    } else {
        (Vec::new(), Vec::new())
    };

    let llm_backup =
        if table_exists(conn, "llm_config") { read_legacy_llm_config(conn)? } else { None };

    let backup = ProjectBackup {
        metadata: ExportMetadata {
            spec_version: "3.0".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            app_name: "Bango".to_string(),
            app_version: format!("{} (legacy-upgrade)", env!("CARGO_PKG_VERSION")),
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
        biblio_authors: Vec::new(),
        biblio_article_authors: Vec::new(),
        biblio_institutions: Vec::new(),
        biblio_author_affiliations: Vec::new(),
        biblio_terms: Vec::new(),
        biblio_article_terms: Vec::new(),
        biblio_network_meta: Vec::new(),
        biblio_network_nodes: Vec::new(),
        biblio_network_edges: Vec::new(),
        // Legacy schema predates the translation originals archive; emit empty.
        article_original_content: Vec::new(),
        article_original_chunks: Vec::new(),
        llm_config: llm_backup,
        /* Legacy schema predates project-portable app_settings (screening rules, summary
        mode, auto-translate); emit empty so the modern importer accepts. */
        app_settings: Vec::new(),
    };

    serde_json::to_string_pretty(&backup).map_err(AppError::Serialization)
}

fn opt_table(conn: &Connection, name: &str) -> Result<Vec<serde_json::Value>, AppError> {
    if !table_exists(conn, name) {
        return Ok(Vec::new());
    }
    serialize_table(conn, &format!("SELECT * FROM {name}"))
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    let count: i64 = match conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    ) {
        Ok(c) => c,
        Err(_) => return false,
    };
    count > 0
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
                    serde_json::from_str::<serde_json::Value>(&text)
                        .unwrap_or_else(|_| serde_json::json!(text))
                }
                Ok(rusqlite::types::ValueRef::Blob(_)) => serde_json::Value::Null,
                Err(_) => serde_json::Value::Null,
            };
            map.insert(to_camel_case(name), value);
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

fn dedup_legacy_references(
    conn: &Connection,
) -> Result<(Vec<serde_json::Value>, Vec<serde_json::Value>), AppError> {
    let mut papers: Vec<serde_json::Value> = Vec::new();
    let mut links: Vec<serde_json::Value> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT id, parent_id, type, match_status, matched_article_id,
                title, abstract_text, authors, publication_year, doi, journal,
                volume, issue, start_page, end_page, keywords, url, language,
                publisher, publisher_city, publisher_address, issn, eissn,
                reference_type, date, notes, ris_extras, num_cited,
                num_references, import_source, imported_at
         FROM article_references",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(LegacyRefRow {
            id: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
            parent_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ref_type: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            match_status: row
                .get::<_, Option<String>>(3)?
                .unwrap_or_else(|| "unmatched".to_string()),
            matched_article_id: row.get::<_, Option<String>>(4)?,
            title: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            abstract_text: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            authors: row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "[]".to_string()),
            publication_year: row.get::<_, Option<i64>>(8)?,
            doi: row.get::<_, Option<String>>(9)?,
            journal: row.get::<_, Option<String>>(10)?,
            volume: row.get::<_, Option<String>>(11)?,
            issue: row.get::<_, Option<String>>(12)?,
            start_page: row.get::<_, Option<String>>(13)?,
            end_page: row.get::<_, Option<String>>(14)?,
            keywords: row.get::<_, Option<String>>(15)?.unwrap_or_else(|| "[]".to_string()),
            url: row.get::<_, Option<String>>(16)?,
            language: row.get::<_, Option<String>>(17)?,
            publisher: row.get::<_, Option<String>>(18)?,
            publisher_city: row.get::<_, Option<String>>(19)?,
            publisher_address: row.get::<_, Option<String>>(20)?,
            issn: row.get::<_, Option<String>>(21)?,
            eissn: row.get::<_, Option<String>>(22)?,
            reference_type: row.get::<_, Option<String>>(23)?,
            date: row.get::<_, Option<String>>(24)?,
            notes: row.get::<_, Option<String>>(25)?,
            ris_extras: row.get::<_, Option<String>>(26)?.unwrap_or_else(|| "{}".to_string()),
            num_cited: row.get::<_, Option<i64>>(27)?,
            num_references: row.get::<_, Option<i64>>(28)?,
            import_source: row.get::<_, Option<String>>(29)?,
            imported_at: row
                .get::<_, Option<String>>(30)?
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        })
    })?;

    for row in rows {
        let row = row?;
        let dedup_key = match &row.doi {
            Some(d) if !d.is_empty() => format!("doi:{}", d.to_lowercase()),
            _ => format!(
                "ta:{}|{}|{}",
                row.title.to_lowercase(),
                row.authors,
                row.publication_year.map(|y| y.to_string()).unwrap_or_default()
            ),
        };

        let surviving_id = if let Some(&idx) = seen.get(&dedup_key) {
            papers[idx]["id"].as_str().unwrap_or_default().to_string()
        } else {
            let new_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let mut paper = serde_json::Map::new();
            paper.insert("id".to_string(), serde_json::Value::String(new_id.clone()));
            paper.insert("title".to_string(), serde_json::Value::String(row.title.clone()));
            paper.insert(
                "abstractText".to_string(),
                serde_json::Value::String(row.abstract_text.clone()),
            );
            paper.insert("authors".to_string(), parse_json_value(&row.authors));
            paper.insert("publicationYear".to_string(), opt_i64_to_value(row.publication_year));
            paper.insert("doi".to_string(), opt_str_to_value(row.doi.clone()));
            paper.insert("journal".to_string(), opt_str_to_value(row.journal.clone()));
            paper.insert("volume".to_string(), opt_str_to_value(row.volume.clone()));
            paper.insert("issue".to_string(), opt_str_to_value(row.issue.clone()));
            paper.insert("startPage".to_string(), opt_str_to_value(row.start_page.clone()));
            paper.insert("endPage".to_string(), opt_str_to_value(row.end_page.clone()));
            paper.insert("keywords".to_string(), parse_json_value(&row.keywords));
            paper.insert("url".to_string(), opt_str_to_value(row.url.clone()));
            paper.insert("language".to_string(), opt_str_to_value(row.language.clone()));
            paper.insert("publisher".to_string(), opt_str_to_value(row.publisher.clone()));
            paper.insert("publisherCity".to_string(), opt_str_to_value(row.publisher_city.clone()));
            paper.insert(
                "publisherAddress".to_string(),
                opt_str_to_value(row.publisher_address.clone()),
            );
            paper.insert("issn".to_string(), opt_str_to_value(row.issn.clone()));
            paper.insert("eissn".to_string(), opt_str_to_value(row.eissn.clone()));
            paper.insert("referenceType".to_string(), opt_str_to_value(row.reference_type.clone()));
            paper.insert("date".to_string(), opt_str_to_value(row.date.clone()));
            paper.insert("notes".to_string(), opt_str_to_value(row.notes.clone()));
            paper.insert("risExtras".to_string(), parse_json_value(&row.ris_extras));
            paper.insert(
                "matchStatus".to_string(),
                serde_json::Value::String(row.match_status.clone()),
            );
            paper.insert(
                "matchedArticleId".to_string(),
                opt_str_to_value(row.matched_article_id.clone()),
            );
            paper.insert("citationCount".to_string(), opt_i64_to_value(row.num_cited));
            paper.insert("referenceCount".to_string(), opt_i64_to_value(row.num_references));
            paper.insert("importSource".to_string(), opt_str_to_value(row.import_source.clone()));
            paper.insert(
                "createdAt".to_string(),
                serde_json::Value::String(row.imported_at.clone()),
            );
            paper.insert("updatedAt".to_string(), serde_json::Value::String(now));

            let idx = papers.len();
            seen.insert(dedup_key, idx);
            papers.push(serde_json::Value::Object(paper));
            new_id
        };

        let link_id = uuid::Uuid::new_v4().to_string();
        let mut link = serde_json::Map::new();
        link.insert("id".to_string(), serde_json::Value::String(link_id));
        link.insert(
            "parentArticleId".to_string(),
            serde_json::Value::String(row.parent_id.clone()),
        );
        link.insert(
            "referencePaperId".to_string(),
            serde_json::Value::String(surviving_id.clone()),
        );
        link.insert("type".to_string(), serde_json::Value::from(row.ref_type));
        link.insert("createdAt".to_string(), serde_json::Value::String(row.imported_at.clone()));
        links.push(serde_json::Value::Object(link));
    }

    Ok((papers, links))
}

fn parse_json_value(s: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(s)
        .unwrap_or_else(|_| serde_json::Value::String(s.to_string()))
}

fn read_legacy_llm_config(
    conn: &Connection,
) -> Result<Option<super::project::LlmConfigBackup>, AppError> {
    use super::project::LlmConfigBackup;
    let row: Option<(String, String, String)> = conn
        .query_row("SELECT provider, endpoint_url, model_name FROM llm_config LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .ok();
    Ok(row.map(|(provider, endpoint_url, model_name)| LlmConfigBackup {
        provider,
        endpoint_url,
        model_name,
    }))
}
struct LegacyRefRow {
    /// Intentionally unread: dedup mints fresh UUIDs. Kept for positional `row.get(0)` alignment.
    #[allow(dead_code)]
    id: String,
    parent_id: String,
    ref_type: i64,
    match_status: String,
    matched_article_id: Option<String>,
    title: String,
    abstract_text: String,
    authors: String,
    publication_year: Option<i64>,
    doi: Option<String>,
    journal: Option<String>,
    volume: Option<String>,
    issue: Option<String>,
    start_page: Option<String>,
    end_page: Option<String>,
    keywords: String,
    url: Option<String>,
    language: Option<String>,
    publisher: Option<String>,
    publisher_city: Option<String>,
    publisher_address: Option<String>,
    issn: Option<String>,
    eissn: Option<String>,
    reference_type: Option<String>,
    date: Option<String>,
    notes: Option<String>,
    ris_extras: String,
    num_cited: Option<i64>,
    num_references: Option<i64>,
    import_source: Option<String>,
    imported_at: String,
}
