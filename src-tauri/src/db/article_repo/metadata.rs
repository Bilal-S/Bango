//! `ArticleMetaField` + `ArticleMetaValue` + `update_article_metadata_field`.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use rusqlite::{params, Connection};

use crate::error::AppError;

/// Whitelist of article metadata fields that the UI can edit in-place via the
/// `update_article_metadata` Tauri command. Each variant maps to exactly one
/// validated `articles` column so SQLite column names are **never** derived
/// from user input (per CLAUDE.md "Never interpolate user input into SQL").
///
/// Variants cover the Title (edited in the detail header via double-click)
/// plus the seven fields surfaced in the Article Detail "Metadata" card:
/// Authors, Affiliation, Journal, Year, Lang, DOI, Keywords. Adding a new
/// editable metadata field means adding a variant here AND extending
/// [`ArticleMetaField::column`] + the value-binding arm in
/// [`update_article_metadata_field`].
///
/// Note: `Title` is the only field whose `articles` column is `TEXT NOT NULL`,
/// so its binding arm rejects empty/whitespace input with [`AppError`] instead
/// of clearing to NULL like the other scalar fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArticleMetaField {
    Title,
    Authors,
    Affiliation,
    Journal,
    PublicationYear,
    Language,
    Doi,
    Keywords,
}

impl ArticleMetaField {
    /// The validated `articles` column name this field writes to.
    #[must_use]
    pub fn column(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Authors => "authors",
            Self::Affiliation => "affiliation",
            Self::Journal => "journal",
            Self::PublicationYear => "publication_year",
            Self::Language => "language",
            Self::Doi => "doi",
            Self::Keywords => "keywords",
        }
    }

    /// Human-readable label for the audit detail string.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Authors => "Authors",
            Self::Affiliation => "Affiliation",
            Self::Journal => "Journal",
            Self::PublicationYear => "Year",
            Self::Language => "Language",
            Self::Doi => "DOI",
            Self::Keywords => "Keywords",
        }
    }
}

/// Payload for the `update_article_metadata` Tauri command. The scalar fields
/// arrive as a string (empty string means "clear to NULL"); the two JSON-array
/// fields (`authors`, `keywords`) arrive as `Vec<String>`. The frontend always
/// sends the appropriate variant so the `#[serde(untagged)]` deserialization
/// picks the right one without a discriminator field.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ArticleMetaValue {
    /// Scalar string value (Journal, Year, Language, DOI, Affiliation).
    Scalar(Option<String>),
    /// JSON-array value (Authors, Keywords).
    Array(Vec<String>),
}

/// Valid publication-year range for the metadata editor. Years outside this
/// range are rejected (cleared to NULL) as defense-in-depth; the frontend
/// inline editor also blocks invalid commits with a visible error.
const MIN_PUBLICATION_YEAR: i32 = 1800;
const MAX_PUBLICATION_YEAR: i32 = 2100;

/// Update a single metadata field on an article. The `field` enum validates
/// the column name (no string interpolation); `value` is bound as a parameter.
/// `authors` and `keywords` are serialized to JSON; `publication_year` parses
/// to `Option<i32>` (empty/invalid/out-of-range -> NULL).
///
/// When the `Journal` field changes, `journal_index_id` is re-resolved via
/// `journal_repo::resolve_journal_id` (using the article's existing ISSN/eISSN
/// and the new journal name) so the bibliometric pipelines stay in sync
/// without a manual "Rematch Journals" round-trip. An unrecognized journal
/// name clears `journal_index_id` to `NULL`.
pub fn update_article_metadata_field(
    conn: &Connection,
    article_id: &str,
    field: ArticleMetaField,
    value: ArticleMetaValue,
) -> Result<(), AppError> {
    let col = field.column();
    let sql = format!("UPDATE articles SET {col} = ?1, changed_at = datetime('now') WHERE id = ?2");

    match (field, value) {
        (ArticleMetaField::Title, ArticleMetaValue::Scalar(s)) => {
            // `title` is `TEXT NOT NULL`, so unlike the other scalar fields
            // an empty/whitespace-only value is rejected (not cleared to
            // NULL). The frontend inline editor also blocks empty commits
            // with a visible error; this is defense-in-depth.
            let owned = s.unwrap_or_default();
            let trimmed = owned.trim();
            if trimmed.is_empty() {
                return Err(AppError::Validation(
                    "Title cannot be empty (articles.title is NOT NULL).".to_string(),
                ));
            }
            conn.execute(&sql, params![trimmed, article_id])?;
        }
        (ArticleMetaField::Authors, ArticleMetaValue::Array(arr)) => {
            let json = serde_json::to_string(&arr)?;
            conn.execute(&sql, params![json, article_id])?;
        }
        (ArticleMetaField::Keywords, ArticleMetaValue::Array(arr)) => {
            let json = serde_json::to_string(&arr)?;
            conn.execute(&sql, params![json, article_id])?;
        }
        (ArticleMetaField::PublicationYear, ArticleMetaValue::Scalar(s)) => {
            // Parse + range-check. Empty/invalid/out-of-range -> NULL.
            let year: Option<i32> = s.and_then(|v| {
                let trimmed = v.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    trimmed.parse::<i32>().ok()
                }
            });
            let in_range =
                year.is_some_and(|y| (MIN_PUBLICATION_YEAR..=MAX_PUBLICATION_YEAR).contains(&y));
            let bounded = if in_range { year } else { None };
            conn.execute(&sql, params![bounded, article_id])?;
        }
        (ArticleMetaField::Journal, ArticleMetaValue::Scalar(s)) => {
            let bound: Option<&str> =
                s.as_deref().and_then(|v| if v.trim().is_empty() { None } else { Some(v) });
            conn.execute(&sql, params![bound, article_id])?;
            // Re-resolve journal_index_id using ONLY the new journal name (not
            // the article's existing ISSN/eISSN). When the user manually edits
            // the journal name, the old ISSN belongs to the OLD journal - using
            // it to resolve the new name would keep the stale link alive even
            // for a completely different journal. Matching on the typed name
            // only means an unrecognized name correctly clears the link to NULL.
            let journal_id = crate::db::journal_repo::resolve_journal_id(conn, None, None, bound);
            conn.execute(
                "UPDATE articles SET journal_index_id = ?1, changed_at = datetime('now') \
                 WHERE id = ?2",
                params![journal_id, article_id],
            )?;
        }
        (_, ArticleMetaValue::Scalar(s)) => {
            // Empty string -> NULL so "clear the field" sets it to NULL rather
            // than an empty string, matching how RIS import treats absent fields.
            let bound: Option<&str> =
                s.as_deref().and_then(|v| if v.trim().is_empty() { None } else { Some(v) });
            conn.execute(&sql, params![bound, article_id])?;
        }
        // A scalar field sent as an array (or vice versa) is a frontend bug;
        // treat it as a no-op rather than crashing so a malformed payload does
        // not corrupt the row.
        (_, ArticleMetaValue::Array(_)) => {}
    }
    Ok(())
}
