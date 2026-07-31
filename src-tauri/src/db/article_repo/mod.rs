//! Article repository: CRUD + queries + mutations for the `articles` table.
//!
//! Directory module split (refactor v6, see `.worktrees/refactor6.md`):
//! - `mod.rs` (this file) - shared constants (`MAX_ARTICLES`,
//!   `ARTICLE_SELECT_BASE`), shared row mapper + helpers (`pub(super)`),
//!   module declarations, and comprehensive `pub use` re-exports so every
//!   historical `crate::db::article_repo::*` import path keeps resolving.
//! - `screening_queries.rs` - counts + unscreened-working batch fetches +
//!   capacity helpers.
//! - `insert.rs` - `insert_article`, `insert_articles_batch`.
//! - `query.rs` - `ArticleQuery` + `query_articles` + the read-many fns.
//! - `mutations.rs` - status / dedup / tags / labels / notes / AI decision /
//!   criteria / field-count mutations.
//! - `metadata.rs` - `ArticleMetaField` + `ArticleMetaValue` +
//!   `update_article_metadata_field`.
//! - `bulk_ops.rs` - bulk status + bulk tag/label add/remove + resets.
//! - `full_text.rs` - full text + AI summary helpers.
//! - `translation.rs` - `TranslationStatusInfo` + the translation-status helpers.
//! - `doi_journal.rs` - DOI/journal/counts helpers + `rematch_all_journals`.
//! - `delete.rs` - the hard-delete cascade.
//!
//! Public API unchanged: `bango_lib::db::article_repo::*` import paths work
//! identically to the pre-split single-file module.

mod bulk_ops;
mod delete;
mod doi_journal;
mod full_text;
mod insert;
mod metadata;
mod mutations;
mod query;
mod screening_queries;
mod translation;

// Re-export every public symbol so callers continue to use
// `crate::db::article_repo::<name>` without caring about the submodule split.
pub use bulk_ops::{
    bulk_add_label_to_articles, bulk_add_tag_to_articles, bulk_remove_label_from_articles,
    bulk_remove_tag_from_articles, bulk_update_article_status, reset_screening_errors,
    reset_working_list,
};
pub use delete::delete_article;
pub use doi_journal::{
    check_dois_in_library, get_article_counts, get_articles_with_doi_info, rematch_all_journals,
    resolve_journal_links, ArticleDoiInfo,
};
pub use full_text::{
    clear_full_text, get_full_text_file_name, get_full_text_for_summary, set_ai_summary,
    update_full_text,
};
pub use insert::{insert_article, insert_articles_batch};
pub use metadata::{update_article_metadata_field, ArticleMetaField, ArticleMetaValue};
pub use mutations::{
    bump_changed_at, clear_ai_reasoning, get_article_field_count, mark_as_duplicate,
    move_articles_to_working_batch, move_to_working, override_ai_decision, update_article_criteria,
    update_article_labels, update_article_status, update_article_tags, update_user_notes,
};
pub use query::{
    get_all_articles, get_article_by_id, get_articles_by_ids, get_articles_by_status,
    get_articles_for_export, get_duplicate_articles, get_working_articles, query_articles,
    ArticleQuery,
};
pub use screening_queries::{
    count_articles, count_unscreened_working, count_working, get_next_unscreened_working_batch,
    get_unscreened_working_article_by_id, max_article_char_len, remaining_capacity,
};
pub use translation::{
    get_stranded_translation_articles, get_translatable_import_ids, get_translation_status,
    get_unscreened_working_ids, mark_stranded_capped_failed, mark_translation_queued_batch,
    reset_translation_status, update_translation_status, update_translation_status_failed,
    TranslationStatusInfo,
};

use rusqlite::Connection;

use crate::error::AppError;
use crate::models::article::{AiDecision, Article, ArticleStatus};

/// Project-wide hard cap on the number of articles a single project may hold
/// (see `docs/bango-v4-spec.md` §3.1 - 10,000 total article project limit).
pub(super) const MAX_ARTICLES: usize = 10_000;

/// Shared SELECT base for the `articles` table.
///
/// Includes the `tags` and `labels` correlated subqueries as `tags_json` /
/// `labels_json` so every article fetch returns the joined data in one shot.
/// All article read functions (`get_article_by_id`, `get_all_articles`,
/// `get_articles_by_status`, `get_articles_for_export`, `get_duplicate_articles`,
/// `get_working_articles`, `query_articles`) compose their SQL by appending a
/// WHERE / ORDER BY clause to this constant. Keeps the column list in one place
/// so a schema change is a single edit, not ten.
pub(super) const ARTICLE_SELECT_BASE: &str = "\
SELECT articles.*, \
(SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id \
 WHERE at.article_id = articles.id) AS tags_json, \
(SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id \
 WHERE al.article_id = articles.id) AS labels_json \
FROM articles";

/// Compute the next auto-incrementing `sequence_id` for a new article.
/// Shared by `insert_article` and `insert_articles_batch`.
pub(super) fn next_sequence_id(conn: &Connection) -> Result<i64, AppError> {
    let max_id: i64 =
        conn.query_row("SELECT COALESCE(MAX(sequence_id), 0) FROM articles", [], |row| row.get(0))?;
    Ok(max_id + 1)
}

/// Transaction-scoped variant of [`get_article_by_id`]. Used by
/// `insert_articles_batch` to read back each freshly-inserted article within
/// the same transaction so the batch is atomic.
pub(super) fn get_article_by_id_tx(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
) -> Result<Article, AppError> {
    let sql = format!("{ARTICLE_SELECT_BASE} WHERE id = ?1");
    tx.query_row(&sql, [id], row_to_article).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound(format!("Article {} not found", id))
        }
        other => AppError::Database(other),
    })
}

/// Shared row mapper: decodes one `articles` row (+ the `tags_json` /
/// `labels_json` correlated-subquery columns) into an [`Article`] struct.
/// Every read fn in `query.rs` and `screening_queries.rs` routes through here.
pub(super) fn row_to_article(row: &rusqlite::Row<'_>) -> rusqlite::Result<Article> {
    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "duplicate" => ArticleStatus::Duplicate,
        "working" => ArticleStatus::Working,
        "included" => ArticleStatus::Included,
        "rejected" => ArticleStatus::Rejected,
        _ => ArticleStatus::Duplicate,
    };

    let ai_decision_str: Option<String> = row.get("ai_decision")?;
    let ai_decision = ai_decision_str.map(|d| match d.as_str() {
        "include" => AiDecision::Include,
        _ => AiDecision::Exclude,
    });

    let authors_str: String = row.get("authors")?;
    let authors: Vec<String> = serde_json::from_str(&authors_str).unwrap_or_default();

    let keywords_str: Option<String> = row.get("keywords")?;
    let keywords: Vec<String> =
        keywords_str.and_then(|k| serde_json::from_str(&k).ok()).unwrap_or_default();

    let matched_inc_str: Option<String> = row.get("matched_inclusion_criteria")?;
    let matched_inclusion: Vec<String> =
        matched_inc_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

    let matched_exc_str: Option<String> = row.get("matched_exclusion_criteria")?;
    let matched_exclusion: Vec<String> =
        matched_exc_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

    let ris_extras_str: Option<String> = row.get("ris_extras")?;
    let ris_extras: Option<serde_json::Value> =
        ris_extras_str.and_then(|s| serde_json::from_str(&s).ok());

    let screening_error_int: i32 = row.get("screening_error")?;
    let manual_override_int: i32 = row.get("manual_override")?;

    Ok(Article {
        id: row.get("id")?,
        sequence_id: row.get("sequence_id")?,
        status,
        screening_error: screening_error_int != 0,
        title: row.get("title")?,
        abstract_text: row.get("abstract_text")?,
        authors,
        publication_year: row.get("publication_year")?,
        doi: row.get("doi")?,
        journal: row.get("journal")?,
        volume: row.get("volume")?,
        issue: row.get("issue")?,
        start_page: row.get("start_page")?,
        end_page: row.get("end_page")?,
        keywords,
        url: row.get("url")?,
        language: row.get("language")?,
        publisher: row.get("publisher")?,
        publisher_city: row.get("publisher_city")?,
        publisher_address: row.get("publisher_address")?,
        issn: row.get("issn")?,
        eissn: row.get("eissn")?,
        journal_index_id: row.get("journal_index_id")?,
        reference_type: row.get("reference_type")?,
        date: row.get("date")?,
        author_address: row.get("author_address")?,
        affiliation: row.get("affiliation")?,
        accession_number: row.get("accession_number")?,
        custom_field3: row.get("custom_field3")?,

        journal_abbreviation: row.get("journal_abbreviation")?,
        journal_iso_abbreviation: row.get("journal_iso_abbreviation")?,
        notes: row.get("notes")?,
        web_of_science_db: row.get("web_of_science_db")?,
        user_notes: row.get("user_notes")?,
        ris_extras,
        duplicate_of: row.get("duplicate_of")?,
        ai_decision,
        ai_reasoning: row.get("ai_reasoning")?,
        ai_confidence: row.get("ai_confidence")?,
        matched_inclusion_criteria: matched_inclusion,
        matched_exclusion_criteria: matched_exclusion,
        tags: row
            .get::<_, Option<String>>("tags_json")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        labels: row
            .get::<_, Option<String>>("labels_json")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        manual_override: manual_override_int != 0,
        import_source: row.get("import_source")?,
        imported_at: row.get("imported_at")?,
        changed_at: row.get("changed_at")?,
        screened_at: row.get("screened_at")?,
        data_length: row.get("data_length")?,
        token_estimate: row.get("token_estimate")?,
        actual_tokens: row.get("actual_tokens")?,
        full_text: row.get("full_text")?,
        full_text_ai_summary: row.get("full_text_ai_summary")?,
        num_cited: row.get("num_cited")?,
        num_references: row.get("num_references")?,
        has_citation_details: row.get::<_, i32>("has_citation_details")? != 0,
        has_reference_details: row.get::<_, i32>("has_reference_details")? != 0,
        has_full_text: row.get::<_, i32>("has_full_text")? != 0,
        full_text_file_name: row.get("full_text_file_name")?,
        has_figures_or_tables: row.get::<_, i32>("has_figures_or_tables")? != 0,
        is_translated: row.get::<_, i32>("is_translated")? != 0,
        translation_status: row.get("translation_status")?,
        translation_error: row.get("translation_error")?,
        translated_at: row.get("translated_at")?,
    })
}
