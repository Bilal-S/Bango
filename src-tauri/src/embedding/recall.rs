//! Bounded cosine recall for the citation-finding feature.
//!
//! Given a query string, embed it, then max-pool cosine similarity across each
//! article's rows, returning the top-K article IDs. The candidate pool is
//! bounded by the `included` corpus (default) and filtered to rows matching the
//! current model's dimensions so a provider switch doesn't mix incompatible
//! vectors.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::db::connection::{lock_conn, DbState};
use crate::db::embedding_repo;
use crate::db::llm_config_repo;
use crate::embedding::text::cosine_similarity;
use crate::error::AppError;
use crate::llm::orchestrator::LlmOrchestrator;

/// One recall hit: an article ID + its max-pooled similarity score.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingHit {
    pub article_id: String,
    pub score: f32,
}

/// Recall the top-`top_k` articles whose embeddings are most similar to `query`.
///
/// - Embeds the query via the orchestrator (one call).
/// - Loads all same-dimension rows from `article_embeddings` (optionally
///   filtered by article status, default `"included"`).
/// - Max-pools cosine similarity per article (best chunk wins).
/// - Returns the top-K sorted by score descending.
///
/// Returns an empty vec when embeddings are disabled, the table is empty, or
/// the query embedding fails (caller falls back to LIKE).
pub async fn recall(
    db_state: &State<'_, DbState>,
    orchestrator: &Arc<LlmOrchestrator>,
    query: &str,
    top_k: usize,
    status_filter: Option<&str>,
) -> Result<Vec<EmbeddingHit>, AppError> {
    let top_k = if top_k == 0 { 30 } else { top_k };

    // Read config + status + model + dimensions under one brief lock.
    let (config, status, model, dimensions) = {
        let conn = lock_conn(&db_state.conn)?;
        let cfg = llm_config_repo::get_config(&conn)?;
        let st = app_settings_repo::get_embedding_status(&conn)?;
        let m = app_settings_repo::get_embedding_model(&conn)?.unwrap_or_default();
        let d = app_settings_repo::get_embedding_dimensions(&conn)?;
        (cfg, st, m, d)
    };

    if status != EmbeddingStatus::Enabled || dimensions <= 0 {
        return Ok(Vec::new());
    }
    let Some(cfg) = config else {
        return Ok(Vec::new());
    };

    // Embed the query (no DB lock held).
    let query_vec = match orchestrator.send_embedding(&cfg, &[query.to_string()], &model).await {
        Ok((vectors, _)) => vectors.into_iter().next().unwrap_or_default(),
        Err(_) => return Ok(Vec::new()),
    };
    if query_vec.is_empty() {
        return Ok(Vec::new());
    }

    // Load candidate rows (brief lock).
    let rows = {
        let conn = lock_conn(&db_state.conn)?;
        embedding_repo::list_for_recall(&conn, dimensions, status_filter)?
    };

    // Max-pool per article. The seed is `NEG_INFINITY`, the identity element
    // for `max` over f32 — this is the most idiomatic choice and is robust to
    // any future broadening of the score range. (The previous `f32::MIN` seed
    // was technically also correct for cosine's `[-1.0, 1.0]` range since
    // `f32::MIN = -3.4e38 < -1.0`, but `NEG_INFINITY` communicates intent
    // more clearly and avoids any confusion about `f32::MIN` vs
    // `f32::MIN_POSITIVE`.)
    use std::collections::HashMap;
    let mut best: HashMap<String, f32> = HashMap::new();
    for row in rows {
        if row.embedding.len() != query_vec.len() {
            continue; // dimension guard (defense-in-depth)
        }
        let sim = cosine_similarity(&query_vec, &row.embedding);
        let entry = best.entry(row.article_id).or_insert(f32::NEG_INFINITY);
        if sim > *entry {
            *entry = sim;
        }
    }

    let mut hits: Vec<EmbeddingHit> =
        best.into_iter().map(|(article_id, score)| EmbeddingHit { article_id, score }).collect();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(top_k);
    Ok(hits)
}
