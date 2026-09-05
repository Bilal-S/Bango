use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::db::tag_label_core::{self, read_row, NamedEntityRow};
use crate::error::AppError;
use crate::models::label::{Label, LabelSource};

fn label_source(source: &str) -> LabelSource {
    match source {
        "ai_generated" => LabelSource::AiGenerated,
        _ => LabelSource::UserCreated,
    }
}

/// Map one raw `labels` row to the public `Label` model.
fn label_from_row(row: NamedEntityRow) -> Label {
    Label { id: row.id, name: row.name, source: label_source(&row.source), color: row.color }
}

pub fn get_all_labels(conn: &Connection) -> Result<Vec<Label>, AppError> {
    let mut stmt = conn.prepare("SELECT id, name, source, color FROM labels ORDER BY name")?;
    let rows = stmt.query_map([], |row| Ok(label_from_row(read_row(row)?)))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_label(conn: &Connection, name: &str, source: &str) -> Result<Label, AppError> {
    // Dedupe case-insensitively to avoid the UNIQUE constraint violation
    // (pinned by tests/db/tags_labels_test.rs::create_label_dedupes_normalized_name).
    if let Some(raw) = tag_label_core::find_by_normalized_name(conn, "labels", name) {
        return Ok(label_from_row(raw));
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO labels (id, name, source) VALUES (?1, ?2, ?3)",
        params![id, name, source],
    )?;
    Ok(Label { id, name: name.to_string(), source: label_source(source), color: None })
}

pub fn rename_label(conn: &Connection, id: &str, new_name: &str) -> Result<Label, AppError> {
    conn.execute("UPDATE labels SET name = ?1 WHERE id = ?2", params![new_name, id])?;
    get_all_labels(conn)?
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Label {} not found", id)))
}

pub fn delete_label(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM labels WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn update_label_color(
    conn: &Connection,
    id: &str,
    color: Option<&str>,
) -> Result<Label, AppError> {
    conn.execute("UPDATE labels SET color = ?1 WHERE id = ?2", params![color, id])?;
    get_all_labels(conn)?
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Label {} not found", id)))
}

pub fn merge_labels(
    conn: &Connection,
    source_id: &str,
    target_id: &str,
) -> Result<Label, AppError> {
    conn.execute(
        "UPDATE OR IGNORE article_labels SET label_id = ?1 WHERE label_id = ?2",
        params![target_id, source_id],
    )?;
    delete_label(conn, source_id)?;
    get_all_labels(conn)?
        .into_iter()
        .find(|l| l.id == target_id)
        .ok_or_else(|| AppError::NotFound(format!("Label {} not found", target_id)))
}

pub fn get_article_count_for_label(conn: &Connection, label_id: &str) -> Result<usize, AppError> {
    let count: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM article_labels WHERE label_id = ?1",
            params![label_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(count)
}

pub fn create_labels_batch(
    conn: &Connection,
    names: &[String],
    source: &str,
) -> Result<Vec<Label>, AppError> {
    let mut labels = Vec::with_capacity(names.len());
    for name in names {
        if !tag_label_core::exists_normalized(conn, "labels", name) {
            labels.push(create_label(conn, name, source)?);
        }
    }
    Ok(labels)
}
