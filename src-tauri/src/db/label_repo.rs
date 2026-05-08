use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::label::{Label, LabelSource};

pub fn get_all_labels(conn: &Connection) -> Result<Vec<Label>, AppError> {
    let mut stmt = conn.prepare("SELECT id, name, source FROM labels ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        let source_str: String = row.get(2)?;
        let source = match source_str.as_str() {
            "ai_generated" => LabelSource::AiGenerated,
            _ => LabelSource::UserCreated,
        };
        Ok(Label { id: row.get(0)?, name: row.get(1)?, source })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_label(conn: &Connection, name: &str, source: &str) -> Result<Label, AppError> {
    // Check if label already exists (case-insensitive) to avoid UNIQUE constraint violation
    let existing: Option<Label> = conn
        .query_row(
            "SELECT id, name, source FROM labels WHERE LOWER(name) = LOWER(?1)",
            params![name],
            |row| {
                let source_str: String = row.get(2)?;
                let source_enum = match source_str.as_str() {
                    "ai_generated" => LabelSource::AiGenerated,
                    _ => LabelSource::UserCreated,
                };
                Ok(Label { id: row.get(0)?, name: row.get(1)?, source: source_enum })
            },
        )
        .ok();

    if let Some(label) = existing {
        return Ok(label);
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO labels (id, name, source) VALUES (?1, ?2, ?3)",
        params![id, name, source],
    )?;
    let source_enum = match source {
        "ai_generated" => LabelSource::AiGenerated,
        _ => LabelSource::UserCreated,
    };
    Ok(Label { id, name: name.to_string(), source: source_enum })
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
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM labels WHERE LOWER(name) = LOWER(?1)",
                params![name],
                |row| row.get::<_, usize>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if !exists {
            labels.push(create_label(conn, name, source)?);
        }
    }
    Ok(labels)
}
