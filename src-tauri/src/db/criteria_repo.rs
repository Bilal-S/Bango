use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::criterion::{Criterion, CriterionType, Priority, ResearchAim};

// Research Aims

pub fn get_all_aims(conn: &Connection) -> Result<Vec<ResearchAim>, AppError> {
    let mut stmt =
        conn.prepare("SELECT id, text, created_at FROM research_aims ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| {
        Ok(ResearchAim { id: row.get(0)?, text: row.get(1)?, created_at: row.get(2)? })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_aim(conn: &Connection, text: &str) -> Result<ResearchAim, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO research_aims (id, text, created_at) VALUES (?1, ?2, ?3)",
        params![id, text, now],
    )?;
    Ok(ResearchAim { id, text: text.to_string(), created_at: now })
}

pub fn delete_aim(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM research_aims WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn update_aim(conn: &Connection, id: &str, text: &str) -> Result<ResearchAim, AppError> {
    conn.execute("UPDATE research_aims SET text = ?1 WHERE id = ?2", params![text, id])?;
    get_all_aims(conn)?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Aim {id} not found")))
}

// Criteria

pub fn get_all_criteria(conn: &Connection) -> Result<Vec<Criterion>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, type, text, priority, created_at FROM criteria ORDER BY created_at")?;
    let rows = stmt.query_map([], row_to_criterion)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_criteria_by_type(
    conn: &Connection,
    criterion_type: &str,
) -> Result<Vec<Criterion>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, type, text, priority, created_at FROM criteria WHERE type = ?1 ORDER BY created_at",
    )?;
    let rows = stmt.query_map([criterion_type], row_to_criterion)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_criterion(
    conn: &Connection,
    criterion_type: &str,
    text: &str,
    priority: &str,
) -> Result<Criterion, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO criteria (id, type, text, priority, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, criterion_type, text, priority, now],
    )?;
    Ok(Criterion {
        id,
        criterion_type: parse_criterion_type(criterion_type),
        text: text.to_string(),
        priority: parse_priority(priority),
        created_at: now,
    })
}

pub fn update_criterion(
    conn: &Connection,
    id: &str,
    text: &str,
    priority: &str,
) -> Result<Criterion, AppError> {
    conn.execute(
        "UPDATE criteria SET text = ?1, priority = ?2 WHERE id = ?3",
        params![text, priority, id],
    )?;
    get_all_criteria(conn)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Criterion {id} not found")))
}

pub fn delete_criterion(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM criteria WHERE id = ?1", params![id])?;
    Ok(())
}

fn row_to_criterion(row: &rusqlite::Row<'_>) -> rusqlite::Result<Criterion> {
    let type_str: String = row.get(1)?;
    let priority_str: String = row.get(3)?;
    Ok(Criterion {
        id: row.get(0)?,
        criterion_type: parse_criterion_type(&type_str),
        text: row.get(2)?,
        priority: parse_priority(&priority_str),
        created_at: row.get(4)?,
    })
}

fn parse_criterion_type(s: &str) -> CriterionType {
    match s {
        "inclusion" => CriterionType::Inclusion,
        _ => CriterionType::Exclusion,
    }
}

fn parse_priority(s: &str) -> Priority {
    match s {
        "critical" => Priority::Critical,
        "high" => Priority::High,
        "low" => Priority::Low,
        "optional" => Priority::Optional,
        _ => Priority::Standard,
    }
}
