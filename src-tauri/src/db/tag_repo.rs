use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::tag::{Tag, TagSource};

pub fn get_all_tags(conn: &Connection) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare("SELECT id, name, source FROM tags ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        let source_str: String = row.get(2)?;
        let source = match source_str.as_str() {
            "ai_suggested" => TagSource::AiSuggested,
            "ris_keyword" => TagSource::RisKeyword,
            _ => TagSource::UserCreated,
        };
        Ok(Tag { id: row.get(0)?, name: row.get(1)?, source })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_tag(conn: &Connection, name: &str, source: &str) -> Result<Tag, AppError> {
    // Check if tag already exists (case-insensitive) to avoid UNIQUE constraint violation
    let existing: Option<Tag> = conn
        .query_row(
            "SELECT id, name, source FROM tags WHERE LOWER(name) = LOWER(?1)",
            params![name],
            |row| {
                let source_str: String = row.get(2)?;
                let source_enum = match source_str.as_str() {
                    "ai_suggested" => TagSource::AiSuggested,
                    "ris_keyword" => TagSource::RisKeyword,
                    _ => TagSource::UserCreated,
                };
                Ok(Tag { id: row.get(0)?, name: row.get(1)?, source: source_enum })
            },
        )
        .ok();

    if let Some(tag) = existing {
        return Ok(tag);
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO tags (id, name, source) VALUES (?1, ?2, ?3)",
        params![id, name, source],
    )?;
    let source_enum = match source {
        "ai_suggested" => TagSource::AiSuggested,
        "ris_keyword" => TagSource::RisKeyword,
        _ => TagSource::UserCreated,
    };
    Ok(Tag { id, name: name.to_string(), source: source_enum })
}

pub fn rename_tag(conn: &Connection, id: &str, new_name: &str) -> Result<Tag, AppError> {
    conn.execute("UPDATE tags SET name = ?1 WHERE id = ?2", params![new_name, id])?;
    get_all_tags(conn)?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Tag {} not found", id)))
}

pub fn delete_tag(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn merge_tags(conn: &Connection, source_id: &str, target_id: &str) -> Result<Tag, AppError> {
    // Re-link all articles from source tag to target tag
    conn.execute(
        "UPDATE OR IGNORE article_tags SET tag_id = ?1 WHERE tag_id = ?2",
        params![target_id, source_id],
    )?;
    // Delete the source tag
    delete_tag(conn, source_id)?;
    get_all_tags(conn)?
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| AppError::NotFound(format!("Tag {} not found", target_id)))
}

pub fn get_article_count_for_tag(conn: &Connection, tag_id: &str) -> Result<usize, AppError> {
    let count: usize = conn
        .query_row("SELECT COUNT(*) FROM article_tags WHERE tag_id = ?1", params![tag_id], |row| {
            row.get(0)
        })
        .unwrap_or(0);
    Ok(count)
}

pub fn create_tags_batch(
    conn: &Connection,
    names: &[String],
    source: &str,
) -> Result<Vec<Tag>, AppError> {
    let mut tags = Vec::with_capacity(names.len());
    for name in names {
        // Skip duplicates (case-insensitive)
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE LOWER(name) = LOWER(?1)",
                params![name],
                |row| row.get::<_, usize>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if !exists {
            tags.push(create_tag(conn, name, source)?);
        }
    }
    Ok(tags)
}
