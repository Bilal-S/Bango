use serde::Deserialize;
use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::dedup::engine::{self, DedupArticle};
use crate::dedup::types::{DedupResolution, DedupResult, DuplicatePair};
use crate::error::AppError;
use crate::models::article::Article;

use uuid::Uuid;

/// Result of classifying newly imported articles.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationResult {
    /// Articles moved to working (no duplicates found)
    pub moved_to_working: usize,
    /// Articles that are duplicates of existing ones
    pub duplicates_found: usize,
    /// Exact duplicates auto-merged
    pub exact_duplicates: usize,
    /// Fuzzy matches needing manual review
    pub fuzzy_matches: usize,
}

/// After import, classify newly inserted articles:
/// - Run dedup against ALL existing articles
/// - Exact duplicates → mark `duplicate_of`, keep in `duplicate` status
/// - Fuzzy matches → keep in `duplicate` for manual review
/// - No match → move to `working`
///
/// **Key rule**: existing articles (working/included/rejected) are NEVER modified.
pub fn classify_imported_articles(
    conn: &rusqlite::Connection,
    new_articles: &[Article],
) -> Result<ClassificationResult, AppError> {
    if new_articles.is_empty() {
        return Ok(ClassificationResult {
            moved_to_working: 0,
            duplicates_found: 0,
            exact_duplicates: 0,
            fuzzy_matches: 0,
        });
    }

    // Collect IDs of newly imported articles
    let new_ids: std::collections::HashSet<String> =
        new_articles.iter().map(|a| a.id.clone()).collect();

    // Fetch ALL articles (including the newly imported ones) for dedup comparison
    let all_articles = article_repo::get_all_articles(conn)?;

    // Convert to DedupArticle for the engine
    let all_dedup: Vec<DedupArticle> = all_articles
        .iter()
        .map(|a| DedupArticle {
            id: a.id.clone(),
            title: a.title.clone(),
            authors: a.authors.clone(),
            publication_year: a.publication_year,
            doi: a.doi.clone(),
            import_source: a.import_source.clone(),
        })
        .collect();

    // Run dedup on everything (read-only operation, no transaction needed)
    let result = engine::run_dedup(&all_dedup);

    // Collect IDs of newly imported articles that are duplicates
    let mut duplicate_new_ids = std::collections::HashSet::new();

    // Pre-compute exact duplicate pairs for processing
    let exact_pairs: Vec<(String, String)> = result
        .exact_duplicates
        .iter()
        .filter_map(|pair| {
            if new_ids.contains(&pair.article_b_id) {
                Some((pair.article_b_id.clone(), pair.article_a_id.clone()))
            } else if new_ids.contains(&pair.article_a_id) {
                Some((pair.article_a_id.clone(), pair.article_b_id.clone()))
            } else {
                None
            }
        })
        .collect();

    for (duplicate_id, _) in &exact_pairs {
        duplicate_new_ids.insert(duplicate_id.clone());
    }

    // Process fuzzy matches — keep new articles in 'duplicate' for manual review
    for pair in &result.fuzzy_matches {
        if new_ids.contains(&pair.article_b_id) {
            duplicate_new_ids.insert(pair.article_b_id.clone());
        } else if new_ids.contains(&pair.article_a_id) {
            duplicate_new_ids.insert(pair.article_a_id.clone());
        }
    }

    let exact_count = result.exact_duplicates.len();
    let fuzzy_count = result.fuzzy_matches.len();

    // Move non-duplicate new articles to working
    let to_move: Vec<String> = new_ids.difference(&duplicate_new_ids).cloned().collect();

    // Wrap all writes in a transaction for data integrity
    let tx = conn.unchecked_transaction()?;

    for (duplicate_id, surviving_id) in &exact_pairs {
        article_repo::mark_as_duplicate(&tx, duplicate_id, surviving_id)?;
        let audit_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_auto', ?3, 'system')",
            rusqlite::params![audit_id, duplicate_id, format!("Auto-detected duplicate of article {}", surviving_id)],
        )?;
    }

    for id in &to_move {
        tx.execute(
            "UPDATE articles SET status = 'working' WHERE id = ?1 AND status = 'duplicate'",
            rusqlite::params![id],
        )?;
    }

    tx.commit()?;

    Ok(ClassificationResult {
        moved_to_working: to_move.len(),
        duplicates_found: duplicate_new_ids.len(),
        exact_duplicates: exact_count,
        fuzzy_matches: fuzzy_count,
    })
}

/// Detection only — returns duplicate pairs without modifying the database.
#[tauri::command]
pub fn check_duplicates(db_state: State<'_, DbState>) -> Result<DedupResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let duplicates = article_repo::get_duplicate_articles(&conn)?;
    let working = article_repo::get_working_articles(&conn)?;

    let dedup_articles: Vec<DedupArticle> = duplicates
        .iter()
        .chain(working.iter())
        .map(|a| DedupArticle {
            id: a.id.clone(),
            title: a.title.clone(),
            authors: a.authors.clone(),
            publication_year: a.publication_year,
            doi: a.doi.clone(),
            import_source: a.import_source.clone(),
        })
        .collect();

    let duplicate_count = duplicates.len();
    let result = if duplicate_count > 0 {
        engine::run_dedup(&dedup_articles)
    } else {
        DedupResult {
            exact_duplicates: vec![],
            fuzzy_matches: vec![],
            auto_merged_count: 0,
            needs_review_count: 0,
        }
    };

    // Report counts but do NOT merge — that's for merge_exact_duplicates
    let mut report = result;
    report.auto_merged_count = report.exact_duplicates.len();
    report.needs_review_count = report.fuzzy_matches.len();
    Ok(report)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    pub pairs: Vec<DuplicatePair>,
}

/// User-triggered: merge all high-confidence exact duplicates.
/// Idempotent: skips pairs where either article is already merged.
#[tauri::command]
pub fn merge_exact_duplicates(
    db_state: State<'_, DbState>,
    request: MergeRequest,
) -> Result<usize, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let tx = conn.unchecked_transaction()?;

    let mut merged = 0usize;
    for pair in &request.pairs {
        // Idempotency: skip if either article is already marked as a duplicate
        let already_merged: bool = tx
            .query_row(
                "SELECT (duplicate_of IS NOT NULL) FROM articles WHERE id = ?1",
                [&pair.article_a_id],
                |row| row.get(0),
            )
            .unwrap_or(true)
            || tx
                .query_row(
                    "SELECT (duplicate_of IS NOT NULL) FROM articles WHERE id = ?1",
                    [&pair.article_b_id],
                    |row| row.get(0),
                )
                .unwrap_or(true);

        if already_merged {
            continue;
        }

        let count_a = article_repo::get_article_field_count(&tx, &pair.article_a_id).unwrap_or(0);
        let count_b = article_repo::get_article_field_count(&tx, &pair.article_b_id).unwrap_or(0);

        let (surviving_id, duplicate_id) = if count_a >= count_b {
            (&pair.article_a_id, &pair.article_b_id)
        } else {
            (&pair.article_b_id, &pair.article_a_id)
        };

        article_repo::mark_as_duplicate(&tx, duplicate_id, surviving_id)?;

        let audit_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_merge', ?3, 'user')",
            rusqlite::params![audit_id, duplicate_id, format!("Merged into article {}", surviving_id)],
        )?;

        // Read actual status before advancing — survivor may already be 'working'
        let current_status: String = tx
            .query_row("SELECT status FROM articles WHERE id = ?1", [&surviving_id], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| "duplicate".to_string());

        if current_status != "working" {
            article_repo::move_to_working(&tx, surviving_id)?;

            let audit_id2 = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO audit_entries (id, article_id, action, from_status, to_status, details, source) VALUES (?1, ?2, 'status_change', ?3, 'working', 'Advanced after deduplication', 'system')",
                rusqlite::params![audit_id2, surviving_id, current_status],
            )?;
        }

        merged += 1;
    }

    tx.commit()?;
    Ok(merged)
}

// Legacy command kept for backward compatibility — now just delegates to check_duplicates.
#[tauri::command]
pub fn run_deduplication(db_state: State<'_, DbState>) -> Result<DedupResult, AppError> {
    check_duplicates(db_state)
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
