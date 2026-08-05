//! Embedding director: computes the (re)embedding work list from a scope.
//!
//! 1. Resolve target articles. 2. Gate on LLM-configured + embeddings enabled.
//! 3. Compute expected `(chunk_index, text)` rows + `input_hash` per article.
//! 4. Compare against stored hashes (missing/mismatch → embed; match → skip).
//! 5. Return `WorkList` for the runner.

use rusqlite::Connection;

use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::db::article_repo;
use crate::db::chunk_repo;
use crate::db::embedding_repo;
use crate::db::llm_config_repo;
use crate::embedding::text::{expected_rows, hash_text, ChunkInput};
use crate::error::AppError;

/// Scope controlling [`compute_work_list`].
#[derive(Debug, Clone, Default)]
pub struct EmbeddingScope {
    /// Explicit IDs (non-empty → overrides `status_filter`).
    pub article_ids: Option<Vec<String>>,
    /// Status filter (default: `"included"`). Ignored when `article_ids` is set.
    pub status_filter: Option<String>,
    /// Force re-embed all rows regardless of stored hash.
    pub force: bool,
}

/// One row needing embedding. `chunk_index`: `-1` = title+abstract; `>= 0` = chunk.
#[derive(Debug, Clone)]
pub struct EmbedTask {
    pub article_id: String,
    pub chunk_index: i32,
    pub text: String,
    pub input_hash: String,
}

/// Why the director returned an empty work list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    LlmNotConfigured,
    Disabled,
    /// `embedding_status == "unknown"` and caller didn't probe. Runner probes then retries.
    UnknownNotProbed,
    NoTargets,
    AllFresh,
}

/// Output of [`compute_work_list`].
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

/// Compute rows needing (re)embedding. Base-condition gates return empty
/// `WorkList` with `SkipReason`. (`UnknownNotProbed` → runner probes first.)
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

    /* Current embedding model name for model-mismatch staleness. Read once
    outside the per-article loop. `None` when `UnknownNotProbed` was already
    returned (this branch unreachable when `status == Unknown`). */
    let current_model = app_settings_repo::get_embedding_model(conn)?;

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

        /* Load stored (chunk_index → (input_hash, model_name)) per article.
        Tracking model_name alongside hash ensures model switches (e.g.
        small→large) mark rows stale even with unchanged hash. Without this,
        rows appear "fresh" but are invisible to recall (dimension mismatch),
        producing a silent zero-results bug. */
        let stored: std::collections::HashMap<i32, (String, String)> = if scope.force {
            std::collections::HashMap::new()
        } else {
            embedding_repo::list_hashes_and_model_for_article(conn, id)?
                .into_iter()
                .map(|(ci, h, m)| (ci, (h, m)))
                .collect()
        };

        for (chunk_index, text) in expected {
            let input_hash = hash_text(&text);
            /* Stale when: force | hash mismatch | model mismatch. Empty stored
            model (pre-feature/corrupt) treated as mismatch → regenerated. */
            let needs = scope.force
                || match stored.get(&chunk_index) {
                    None => true,
                    Some((stored_hash, stored_model)) => {
                        /* ASCII case-insensitive model comparison (embedding model
                        names are ASCII, e.g. text-embedding-3-small). Empty stored
                        model = mismatch → regenerated + backfilled. */
                        let model_mismatch = !stored_model
                            .eq_ignore_ascii_case(current_model.as_deref().unwrap_or(""))
                            || (current_model.is_some() && stored_model.is_empty());
                        *stored_hash != input_hash || model_mismatch
                    }
                };
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
