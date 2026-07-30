//! Embedding director: computes the work list for (re)embedding.
//!
//! Given a scope (`article_ids` OR `status_filter`, plus a `force` flag), the
//! director:
//! 1. Resolves the target article set.
//! 2. Gates on base conditions (LLM configured, embeddings enabled).
//! 3. For each article, computes the expected `(chunk_index, text)` rows and
//!    their `input_hash`.
//! 4. Compares each expected row against the stored row's hash (missing or
//!    mismatch => needs embedding; match => skip).
//! 5. Returns a `WorkList` the runner consumes.

use rusqlite::Connection;

use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::db::article_repo;
use crate::db::chunk_repo;
use crate::db::embedding_repo;
use crate::db::llm_config_repo;
use crate::embedding::text::{expected_rows, hash_text, ChunkInput};
use crate::error::AppError;

/// The scope passed to [`compute_work_list`].
#[derive(Debug, Clone, Default)]
pub struct EmbeddingScope {
    /// Explicit article IDs. When non-empty, takes precedence over
    /// `status_filter`.
    pub article_ids: Option<Vec<String>>,
    /// Status filter (e.g. `"included"`). Ignored when `article_ids` is
    /// non-empty. Defaults to `"included"`.
    pub status_filter: Option<String>,
    /// When `true`, every expected row is marked for embedding regardless of
    /// the stored hash (used by the "Rebuild text chunks" cascade).
    pub force: bool,
}

/// One row that needs embedding.
#[derive(Debug, Clone)]
pub struct EmbedTask {
    pub article_id: String,
    /// `-1` for the title+abstract row; `>= 0` for a per-chunk row.
    pub chunk_index: i32,
    pub text: String,
    pub input_hash: String,
}

/// Why the director produced an empty work list (for the runner's report).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// LLM is not configured.
    LlmNotConfigured,
    /// Embeddings are disabled (`embedding_status == "disabled"`).
    Disabled,
    /// `embedding_status == "unknown"` and the caller did not probe first.
    /// The runner probes + retries; if this propagates, the probe failed.
    UnknownNotProbed,
    /// No target articles matched the scope.
    NoTargets,
    /// All rows are fresh (hashes match) and `force` is false.
    AllFresh,
}

/// The output of [`compute_work_list`].
#[derive(Debug, Clone)]
pub struct WorkList {
    pub rows: Vec<EmbedTask>,
    /// Number of target articles examined.
    pub total_articles: usize,
    /// Number of rows that were fresh (skipped).
    pub skipped_fresh: usize,
    /// `None` when rows is non-empty.
    pub skip_reason: Option<SkipReason>,
}

/// Compute the set of rows that need (re)embedding under `scope`.
///
/// Base-condition gates return an empty `WorkList` with a `SkipReason`:
/// - LLM unconfigured -> `SkipReason::LlmNotConfigured`.
/// - `embedding_status == Disabled` -> `SkipReason::Disabled`.
/// - `embedding_status == Unknown` -> `SkipReason::UnknownNotProbed` (the
///   runner should probe first, then call this again).
pub fn compute_work_list(conn: &Connection, scope: &EmbeddingScope) -> Result<WorkList, AppError> {
    // Base-condition gate 1: LLM configured.
    if !llm_config_repo::has_config(conn)? {
        return Ok(WorkList {
            rows: Vec::new(),
            total_articles: 0,
            skipped_fresh: 0,
            skip_reason: Some(SkipReason::LlmNotConfigured),
        });
    }

    // Base-condition gate 2: embedding status.
    let status = app_settings_repo::get_embedding_status(conn)?;
    match status {
        EmbeddingStatus::Disabled => {
            return Ok(WorkList {
                rows: Vec::new(),
                total_articles: 0,
                skipped_fresh: 0,
                skip_reason: Some(SkipReason::Disabled),
            });
        }
        EmbeddingStatus::Unknown => {
            return Ok(WorkList {
                rows: Vec::new(),
                total_articles: 0,
                skipped_fresh: 0,
                skip_reason: Some(SkipReason::UnknownNotProbed),
            });
        }
        EmbeddingStatus::Enabled => {}
    }

    // Resolve target article IDs.
    let target_ids: Vec<String> = match &scope.article_ids {
        Some(ids) if !ids.is_empty() => ids.clone(),
        _ => {
            let filter = scope.status_filter.as_deref().unwrap_or("included");
            article_repo::get_articles_by_status(conn, filter)?.into_iter().map(|a| a.id).collect()
        }
    };

    if target_ids.is_empty() {
        return Ok(WorkList {
            rows: Vec::new(),
            total_articles: 0,
            skipped_fresh: 0,
            skip_reason: Some(SkipReason::NoTargets),
        });
    }

    let total_articles = target_ids.len();
    let mut rows: Vec<EmbedTask> = Vec::new();
    let mut skipped_fresh = 0usize;

    for id in &target_ids {
        let article = match article_repo::get_article_by_id(conn, id) {
            Ok(a) => a,
            Err(_) => continue, // missing article; skip silently
        };

        // Fetch chunks if the article has full text.
        let chunks: Vec<ChunkInput> = if article.has_full_text {
            chunk_repo::list_chunks_for_article(conn, id)?
                .into_iter()
                .map(|c| ChunkInput { chunk_index: c.chunk_index as i32, body: c.text })
                .collect()
        } else {
            Vec::new()
        };

        let expected =
            expected_rows(&article.title, &article.abstract_text, &chunks, article.has_full_text);

        // Load stored hashes once per article.
        let stored: std::collections::HashMap<i32, String> = if scope.force {
            std::collections::HashMap::new()
        } else {
            embedding_repo::list_hashes_for_article(conn, id)?.into_iter().collect()
        };

        for (chunk_index, text) in expected {
            let input_hash = hash_text(&text);
            let needs = scope.force || (stored.get(&chunk_index) != Some(&input_hash));
            if needs {
                rows.push(EmbedTask { article_id: id.clone(), chunk_index, text, input_hash });
            } else {
                skipped_fresh += 1;
            }
        }
    }

    let skip_reason = if rows.is_empty() { Some(SkipReason::AllFresh) } else { None };

    Ok(WorkList { rows, total_articles, skipped_fresh, skip_reason })
}
