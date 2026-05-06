use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::audit::{AuditAction, AuditEntry, AuditSource, ImportActivity};

pub fn get_audit_trail(conn: &Connection, article_id: &str) -> Result<Vec<AuditEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, article_id, timestamp, action, from_status, to_status, details, source \
         FROM audit_entries WHERE article_id = ?1 ORDER BY timestamp DESC",
    )?;
    let rows = stmt.query_map([article_id], row_to_audit_entry)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_recent_audit_entries(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<AuditEntry>, AppError> {
    // Exclude 'import' entries — those are served by get_import_activities instead
    let mut stmt = conn.prepare(
        "SELECT id, article_id, timestamp, action, from_status, to_status, details, source \
         FROM audit_entries WHERE action != 'import' ORDER BY timestamp DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], row_to_audit_entry)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Returns one row per import file with the correct article count,
/// aggregated at the SQL level so the count is always accurate.
pub fn get_import_activities(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<ImportActivity>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT MIN(id) as id, MIN(timestamp) as timestamp, \
         REPLACE(details, 'Imported from ', '') as filename, COUNT(*) as count \
         FROM audit_entries WHERE action = 'import' \
         GROUP BY details \
         ORDER BY MIN(timestamp) DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(ImportActivity {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            filename: row.get(2)?,
            count: row.get::<_, usize>(3)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_entry(
    conn: &Connection,
    article_id: &str,
    action: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    details: Option<&str>,
    source: &str,
) -> Result<AuditEntry, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, from_status, to_status, details, source) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, article_id, now, action, from_status, to_status, details, source],
    )?;
    Ok(AuditEntry {
        id,
        article_id: article_id.to_string(),
        timestamp: now,
        action: parse_action(action),
        from_status: from_status.map(String::from),
        to_status: to_status.map(String::from),
        details: details.map(String::from),
        source: parse_source(source),
    })
}

fn row_to_audit_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
    let action_str: String = row.get(3)?;
    let source_str: String = row.get(7)?;
    Ok(AuditEntry {
        id: row.get(0)?,
        article_id: row.get(1)?,
        timestamp: row.get(2)?,
        action: parse_action(&action_str),
        from_status: row.get(4)?,
        to_status: row.get(5)?,
        details: row.get(6)?,
        source: parse_source(&source_str),
    })
}

fn parse_action(s: &str) -> AuditAction {
    match s {
        "import" => AuditAction::Import,
        "dedup_merge" => AuditAction::DedupMerge,
        "dedup_flag" => AuditAction::DedupFlag,
        "status_change" => AuditAction::StatusChange,
        "tag_add" => AuditAction::TagAdd,
        "tag_remove" => AuditAction::TagRemove,
        "label_add" => AuditAction::LabelAdd,
        "label_remove" => AuditAction::LabelRemove,
        "criteria_match" => AuditAction::CriteriaMatch,
        "ai_screen" => AuditAction::AiScreen,
        "manual_override" => AuditAction::ManualOverride,
        "ai_summary" => AuditAction::AiSummary,
        _ => AuditAction::StatusChange,
    }
}

fn parse_source(s: &str) -> AuditSource {
    match s {
        "ai" => AuditSource::Ai,
        "user" => AuditSource::User,
        _ => AuditSource::System,
    }
}
