use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::audit::{AuditAction, AuditEntry, AuditSource, ImportActivity};

pub fn get_audit_trail(conn: &Connection, article_id: &str) -> Result<Vec<AuditEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT ae.id, ae.article_id, ae.timestamp, ae.action, ae.from_status, ae.to_status, \
         ae.details, ae.source, SUBSTR(a.title, 1, 55) as article_title \
         FROM audit_entries ae \
         LEFT JOIN articles a ON a.id = ae.article_id \
         WHERE ae.article_id = ?1 ORDER BY ae.timestamp DESC",
    )?;
    let rows = stmt.query_map([article_id], row_to_audit_entry)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_recent_audit_entries(
    conn: &Connection,
    limit: usize,
    offset: usize,
) -> Result<Vec<AuditEntry>, AppError> {
    // Exclude 'import' entries - those are served by get_import_activities instead
    let mut stmt = conn.prepare(
        "SELECT ae.id, ae.article_id, ae.timestamp, ae.action, ae.from_status, ae.to_status, \
         ae.details, ae.source, SUBSTR(a.title, 1, 55) as article_title \
         FROM audit_entries ae \
         LEFT JOIN articles a ON a.id = ae.article_id \
         WHERE ae.action != 'import' AND ae.action != 'error' ORDER BY ae.timestamp DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], row_to_audit_entry)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Returns one row per import file with the correct article count,
/// aggregated at the SQL level so the count is always accurate.
pub fn get_import_activities(
    conn: &Connection,
    limit: usize,
    offset: usize,
) -> Result<Vec<ImportActivity>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT MIN(id) as id, MIN(timestamp) as timestamp, \
         REPLACE(details, 'Imported from ', '') as filename, COUNT(*) as count \
         FROM audit_entries WHERE action = 'import' \
         GROUP BY details \
         ORDER BY MIN(timestamp) DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], |row| {
        Ok(ImportActivity {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            filename: row.get(2)?,
            count: row.get::<_, usize>(3)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Coalescing window for [`create_or_update_entry`]. When a second entry with
/// the same `article_id + action + source` arrives within this many seconds of
/// the previous one, the existing row is updated instead of inserting a new
/// row. This prevents audit-trail spam when the user makes several rapid edits
/// of the same type (e.g. adding 3 labels one at a time produces a single
/// `label_add` entry showing the final count).
const COALESCE_WINDOW_SECS: i64 = 300;

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
    // Fetch article title for context
    let article_title: Option<String> = conn
        .query_row("SELECT SUBSTR(title, 1, 55) FROM articles WHERE id = ?1", [article_id], |row| {
            row.get(0)
        })
        .ok();

    Ok(AuditEntry {
        id,
        article_id: article_id.to_string(),
        timestamp: now,
        action: parse_action(action),
        from_status: from_status.map(String::from),
        to_status: to_status.map(String::from),
        details: details.map(String::from),
        source: parse_source(source),
        article_title,
    })
}

/// Create an audit entry, or **coalesce** with the most recent matching entry
/// if one exists within the [`COALESCE_WINDOW_SECS`] window.
///
/// Coalescing matches on `article_id + action + source`. When a match is found,
/// the existing row's `details`, `from_status`, `to_status`, and `timestamp` are
/// updated to reflect the latest change. This prevents audit-trail spam when the
/// user makes several rapid edits of the same type (e.g. adding 3 labels one at
/// a time produces a single `label_add` entry showing the final count).
///
/// When no recent matching entry exists, this delegates to [`create_entry`] and
/// inserts a new row.
pub fn create_or_update_entry(
    conn: &Connection,
    article_id: &str,
    action: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    details: Option<&str>,
    source: &str,
) -> Result<AuditEntry, AppError> {
    // Look for the most recent entry with the same article_id + action + source
    // within the coalesce window. SQLite's `datetime('now', '-N seconds')`
    // computes the cutoff in UTC, matching the RFC3339 timestamps we write.
    let cutoff = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::seconds(COALESCE_WINDOW_SECS))
        .map(|t| t.to_rfc3339())
        .unwrap_or_default();

    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM audit_entries \
             WHERE article_id = ?1 AND action = ?2 AND source = ?3 \
             AND timestamp >= ?4 \
             ORDER BY timestamp DESC LIMIT 1",
            params![article_id, action, source, cutoff],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing_id {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE audit_entries \
             SET details = ?1, from_status = ?2, to_status = ?3, timestamp = ?4 \
             WHERE id = ?5",
            params![details, from_status, to_status, now, id],
        )?;
        // Fetch article title for context
        let article_title: Option<String> = conn
            .query_row(
                "SELECT SUBSTR(title, 1, 55) FROM articles WHERE id = ?1",
                [article_id],
                |row| row.get(0),
            )
            .ok();

        return Ok(AuditEntry {
            id,
            article_id: article_id.to_string(),
            timestamp: now,
            action: parse_action(action),
            from_status: from_status.map(String::from),
            to_status: to_status.map(String::from),
            details: details.map(String::from),
            source: parse_source(source),
            article_title,
        });
    }

    create_entry(conn, article_id, action, from_status, to_status, details, source)
}

fn row_to_audit_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
    let action_str: String = row.get(3)?;
    let source_str: String = row.get(7)?;
    Ok(AuditEntry {
        id: row.get(0)?,
        article_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
        timestamp: row.get(2)?,
        action: parse_action(&action_str),
        from_status: row.get(4)?,
        to_status: row.get(5)?,
        details: row.get(6)?,
        source: parse_source(&source_str),
        article_title: row.get(8)?,
    })
}

fn parse_action(s: &str) -> AuditAction {
    match s {
        "import" => AuditAction::Import,
        "dedup_merge" => AuditAction::DedupMerge,
        "dedup_flag" => AuditAction::DedupFlag,
        "status_change" => AuditAction::StatusChange,
        "note_add" => AuditAction::NoteAdd,
        "tag_add" => AuditAction::TagAdd,
        "tag_remove" => AuditAction::TagRemove,
        "label_add" => AuditAction::LabelAdd,
        "label_remove" => AuditAction::LabelRemove,
        "criteria_match" => AuditAction::CriteriaMatch,
        "ai_screen" => AuditAction::AiScreen,
        "manual_override" => AuditAction::ManualOverride,
        "ai_summary" => AuditAction::AiSummary,
        "ai_summary_error" => AuditAction::Error,
        "error" => AuditAction::Error,
        "dedup_auto" => AuditAction::DedupAuto,
        "reference_import" => AuditAction::ReferenceImport,
        "reference_match" => AuditAction::ReferenceMatch,
        "wiki_ingest_error" => AuditAction::WikiIngestError,
        "translation" => AuditAction::Translation,
        "translation_error" => AuditAction::TranslationError,
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

/// Log a generic/system error to the audit table (not connected to any article).
pub fn log_error(conn: &Connection, details: &str) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, from_status, to_status, details, source) \
         VALUES (?1, NULL, ?2, 'error', NULL, NULL, ?3, 'system')",
        params![id, now, details],
    )?;
    Ok(())
}

/// System-level (non-article, non-error) audit actions recordable via
/// [`log_system_action`]. Each variant maps to a row in `audit_entries` with
/// `article_id = NULL` and `source = 'ai'`. Kept distinct from the
/// article-bound [`AuditAction`] enum so the "no article" contract is
/// explicit at the call site.
#[derive(Debug, Clone, Copy)]
pub enum SystemAction {
    /// Search Strategy Builder produced a Boolean search strategy (spec §8.4).
    /// Maps to `action = 'search_strategy'`.
    SearchStrategy,
}

impl SystemAction {
    #[must_use]
    fn as_str(&self) -> &'static str {
        match self {
            Self::SearchStrategy => "search_strategy",
        }
    }
}

/// Log a system-level (non-article, non-error) audit row.
///
/// Mirrors [`log_error`]'s NULL-writing shape (`article_id = NULL` so the row
/// surfaces in `get_generic_audit_entries`) but takes an arbitrary action +
/// records `source = 'ai'` (the actor for system-level successes). Use this
/// instead of overloading [`create_entry`]'s `article_id: &str` signature
/// (which would force every caller to pass an empty string and risk the row
/// being missed by the `article_id IS NULL` filter).
///
/// For error-path system rows, keep using [`log_error`] / [`log_error_best_effort`].
pub fn log_system_action(
    conn: &Connection,
    action: SystemAction,
    details: &str,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, from_status, to_status, details, source) \
         VALUES (?1, NULL, ?2, ?3, NULL, NULL, ?4, 'ai')",
        params![id, now, action.as_str(), details],
    )?;
    Ok(())
}

/// Best-effort wrapper around [`log_error`] for the common "I'm in an error arm
/// and want to record what happened, but I must not mask the real error"
/// pattern. Acquires the shared connection mutex tolerantly; if the mutex is
/// poisoned OR the audit write itself fails, the failure is swallowed because
/// the caller is already on its way to returning a more important error.
///
/// This exists so call sites do not inline `if let Ok(conn) = ...lock()` blocks
/// (which trip the "MUST route through `lock_conn`" rule). Use this instead of
/// `log_error` ONLY when you are inside an `Err(e) =>` arm and intend to
/// `return Err(e)` immediately after - for non-error-path audit writes, call
/// `log_error` directly with a `lock_conn` guard.
pub fn log_error_best_effort(conn_mutex: &std::sync::Mutex<Connection>, details: &str) {
    let Ok(conn) = conn_mutex.lock() else {
        // Mutex poisoned: cannot record the audit row. The caller is already
        // returning a real error, so do not mask it with a poison complaint.
        return;
    };
    let _ = log_error(&conn, details);
}

/// Get generic audit entries (system errors with empty article_id).
pub fn get_generic_audit_entries(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<AuditEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT ae.id, ae.article_id, ae.timestamp, ae.action, ae.from_status, ae.to_status, \
         ae.details, ae.source, NULL as article_title \
         FROM audit_entries ae \
         WHERE ae.article_id IS NULL ORDER BY ae.timestamp DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], row_to_audit_entry)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Clear all generic audit entries (those not connected to any article).
/// These are entries with empty article_id, typically system errors.
pub fn clear_generic_entries(conn: &Connection) -> Result<usize, AppError> {
    let count =
        conn.query_row("SELECT COUNT(*) FROM audit_entries WHERE article_id IS NULL", [], |row| {
            row.get::<_, usize>(0)
        })?;
    conn.execute("DELETE FROM audit_entries WHERE article_id IS NULL", [])?;
    Ok(count)
}
