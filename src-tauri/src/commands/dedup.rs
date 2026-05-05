use serde::Deserialize;
use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::dedup::engine::{self, DedupArticle};
use crate::dedup::types::{DedupResolution, DedupResult};
use crate::error::AppError;
use crate::models::article::Article;

use uuid::Uuid;

#[tauri::command]
pub fn run_deduplication(db_state: State<'_, DbState>) -> Result<DedupResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let imported = article_repo::get_imported_articles(&conn)?;
    let working = article_repo::get_working_articles(&conn)?;

    // Convert to dedup articles for comparison
    let dedup_articles: Vec<DedupArticle> = imported
        .iter()
        .chain(working.iter())
        .map(|a| DedupArticle {
            id: a.id.clone(),
            title: a.title.clone(),
            authors: a.authors.clone(),
            publication_year: a.publication_year,
            doi: a.doi.clone(),
        })
        .collect();

    // Compare only if there are imported articles
    let imported_count = imported.len();
    let result = if imported_count > 0 {
        engine::run_dedup(&dedup_articles)
    } else {
        DedupResult {
            exact_duplicates: vec![],
            fuzzy_matches: vec![],
            auto_merged_count: 0,
            needs_review_count: 0,
        }
    };

    // Auto-merge exact duplicates
    for pair in &result.exact_duplicates {
        // Determine which article survives (most metadata fields)
        let count_a = article_repo::get_article_field_count(&conn, &pair.article_a_id).unwrap_or(0);
        let count_b = article_repo::get_article_field_count(&conn, &pair.article_b_id).unwrap_or(0);

        let (surviving_id, duplicate_id) = if count_a >= count_b {
            (&pair.article_a_id, &pair.article_b_id)
        } else {
            (&pair.article_b_id, &pair.article_a_id)
        };

        article_repo::mark_as_duplicate(&conn, duplicate_id, surviving_id)?;

        // Audit entry
        let audit_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_merge', ?3, 'system')",
            rusqlite::params![audit_id, duplicate_id, format!("Merged into article {}", surviving_id)],
        )?;

        // Move surviving article to Working
        article_repo::move_to_working(&conn, surviving_id)?;

        let audit_id2 = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO audit_entries (id, article_id, action, from_status, to_status, details, source) VALUES (?1, ?2, 'status_change', 'imported', 'working', 'Advanced after deduplication', 'system')",
            rusqlite::params![audit_id2, surviving_id],
        )?;
    }

    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFuzzyRequest {
    pub pair_index: usize,
    pub resolution: DedupResolution,
    pub article_a_id: String,
    pub article_b_id: String,
}

#[tauri::command]
pub fn resolve_fuzzy_match(
    db_state: State<'_, DbState>,
    request: ResolveFuzzyRequest,
) -> Result<Article, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    match request.resolution {
        DedupResolution::KeepA => {
            article_repo::mark_as_duplicate(&conn, &request.article_b_id, &request.article_a_id)?;
            article_repo::move_to_working(&conn, &request.article_a_id)?;
            let audit_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_merge', ?3, 'user')",
                rusqlite::params![
                    audit_id,
                    request.article_b_id,
                    format!("User chose to keep article A ({})", request.article_a_id)
                ],
            )?;
            article_repo::get_article_by_id(&conn, &request.article_a_id)
        }
        DedupResolution::KeepB => {
            article_repo::mark_as_duplicate(&conn, &request.article_a_id, &request.article_b_id)?;
            article_repo::move_to_working(&conn, &request.article_b_id)?;
            let audit_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_merge', ?3, 'user')",
                rusqlite::params![
                    audit_id,
                    request.article_a_id,
                    format!("User chose to keep article B ({})", request.article_b_id)
                ],
            )?;
            article_repo::get_article_by_id(&conn, &request.article_b_id)
        }
        DedupResolution::KeepBoth => {
            article_repo::move_to_working(&conn, &request.article_a_id)?;
            article_repo::move_to_working(&conn, &request.article_b_id)?;
            let audit_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_flag', 'User marked as not duplicates', 'user')",
                rusqlite::params![audit_id, request.article_a_id],
            )?;
            article_repo::get_article_by_id(&conn, &request.article_a_id)
        }
    }
}
