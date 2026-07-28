use crate::error::AppError;
use rusqlite::Connection;

/// Maximum character length for newly created tags or labels that don't match
/// existing ones.
///
/// Tags and labels must be concise descriptors, not full justifications. The
/// screening system prompt instructs the LLM to keep tags <= 35 chars; this is
/// the backend enforcement that strips prefixes and truncates at word
/// boundaries so a too-long name never silently loses context (e.g.
/// `"inclusion: this is a very long cr"` -> `"this-is-a-very-long"`).
pub const MAX_NEW_TAG_LABEL_LEN: usize = 35;

/// Prefixes the LLM sometimes erroneously adds to tags/labels despite the
/// system prompt instruction not to. Stripped before storage so the tag/label
/// is a clean descriptor.
pub const TAG_LABEL_PREFIXES: &[&str] = &["inclusion:", "exclusion:", "inclusion -", "exclusion -"];

/// Sanitize a tag or label name before storage:
/// 1. Strip any `inclusion:`/`exclusion:` prefix the LLM may have added.
/// 2. Lowercase + trim.
/// 3. Replace spaces/underscores with hyphens.
/// 4. Truncate at a word boundary (hyphen) so the name is never cut mid-word.
///
/// Pure function (`#[must_use]`) so it can be unit-tested independently.
#[must_use]
pub fn sanitize_tag_or_label_name(raw: &str, max_len: usize) -> String {
    // 1. Strip known prefixes (case-insensitive).
    let mut cleaned = raw.trim().to_lowercase();
    for prefix in TAG_LABEL_PREFIXES {
        if cleaned.starts_with(prefix) {
            cleaned = cleaned[prefix.len()..].trim_start().to_string();
            break;
        }
    }

    // 2. Replace whitespace + underscores with hyphens, collapse repeats.
    cleaned = cleaned
        .chars()
        .map(|c| if c.is_whitespace() || c == '_' { '-' } else { c })
        .collect::<String>();
    while cleaned.contains("--") {
        cleaned = cleaned.replace("--", "-");
    }
    let cleaned = cleaned.trim_matches('-').to_string();

    if cleaned.is_empty() {
        return cleaned;
    }

    // 3. If within limit, return as-is.
    if cleaned.chars().count() <= max_len {
        return cleaned;
    }

    // 4. Truncate at the last hyphen within the limit (word boundary).
    let truncated: String = cleaned.chars().take(max_len).collect();
    if let Some(idx) = truncated.rfind('-') {
        truncated[..idx].to_string()
    } else {
        // No hyphen within the limit - hard truncate (single-word name).
        truncated
    }
}

/// Truncate at a word boundary (hyphen) so the result is at most `max_len`
/// chars.
///
/// Pure helper (`#[must_use]`) used by `create_or_match_tag`/
/// `create_or_match_label` to avoid cutting names mid-word.
#[must_use]
pub fn truncate_at_word_boundary(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_len).collect();
    if let Some(idx) = truncated.rfind('-') {
        truncated[..idx].to_string()
    } else {
        truncated
    }
}

pub fn create_or_match_tag(
    conn: &Connection,
    tag_name: &str,
    article_id: &str,
) -> Result<(), AppError> {
    // Sanitize: strip prefixes, lowercase, hyphenate, word-boundary truncate.
    let sanitized = sanitize_tag_or_label_name(tag_name, MAX_NEW_TAG_LABEL_LEN);
    if sanitized.is_empty() {
        return Ok(());
    }

    // Check if tag exists (case-insensitive)
    let existing_id: Option<String> = conn
        .query_row("SELECT id FROM tags WHERE LOWER(name) = ?1", [&sanitized], |row| row.get(0))
        .ok();

    let tag_id = match existing_id {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO tags (id, name, source) VALUES (?1, ?2, 'ai_suggested')",
                rusqlite::params![id, sanitized],
            )?;
            id
        }
    };

    // Link tag to article (ignore if already linked)
    conn.execute(
        "INSERT OR IGNORE INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, tag_id],
    )?;

    Ok(())
}

pub fn create_or_match_label(
    conn: &Connection,
    label_name: &str,
    article_id: &str,
) -> Result<(), AppError> {
    // Sanitize: strip prefixes, lowercase, hyphenate, word-boundary truncate.
    let sanitized = sanitize_tag_or_label_name(label_name, MAX_NEW_TAG_LABEL_LEN);
    if sanitized.is_empty() {
        return Ok(());
    }

    // Check if label exists (case-insensitive)
    let existing_id: Option<String> = conn
        .query_row("SELECT id FROM labels WHERE LOWER(name) = ?1", [&sanitized], |row| row.get(0))
        .ok();

    let label_id = match existing_id {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO labels (id, name, source) VALUES (?1, ?2, 'ai_generated')",
                rusqlite::params![id, sanitized],
            )?;
            id
        }
    };

    // Link label to article (ignore if already linked)
    conn.execute(
        "INSERT OR IGNORE INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, label_id],
    )?;

    Ok(())
}
