//! Bounded cosine recall: embed query → max-pool cosine per article → top-K IDs.
//! Candidate pool bounded by `included` corpus; filtered to current model's dimensions
//! so provider switches don't mix incompatible vectors.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::db::connection::{lock_conn, DbState};
use crate::db::embedding_repo::{self, EmbeddingRow};
use crate::db::llm_config_repo;
use crate::embedding::text::cosine_similarity;
use crate::error::AppError;
use crate::llm::orchestrator::LlmOrchestrator;

/// One recall hit: an article ID + its max-pooled similarity score + the
/// `chunk_index` of the embedding row that produced the max score
/// (`TITLE_ABSTRACT_CHUNK_INDEX (-1)` for the title+abstract row, `>= 0` for a
/// per-chunk row). The chunk provenance lets the Citation Finder passage layer
/// fall back to the cosine-best chunk when token containment ranks a different
/// (less relevant) chunk first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingHit {
    pub article_id: String,
    pub score: f32,
    /// Provenance of the winning row. `None` only for hand-built hits (tests).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<i32>,
}

/// Recall top-K articles by embedding cosine similarity to `query`.
///
/// Embeds query → max-pools cosine per article → top-K sorted descending.
/// `status_filter`: non-empty → scoped to those statuses; empty → all.
/// Returns empty vec on disabled/empty/failure (caller falls back to LIKE).
pub async fn recall(
    db_state: &State<'_, DbState>,
    orchestrator: &Arc<LlmOrchestrator>,
    query: &str,
    top_k: usize,
    status_filter: &[String],
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

    /* Max-pool per article via the pure helper (tracks the winning row's
    `chunk_index` for passage-layer provenance). Sorted by score desc with an
    article-id tiebreak for determinism. */
    let mut hits = pool_hits(&rows, &query_vec);
    hits.truncate(top_k);
    Ok(hits)
}

/// Max-pool cosine per article over embedding rows. Pure `#[must_use]`.
///
/// Each article keeps its highest-cosine row's score plus that row's
/// `chunk_index` (ties keep the first row encountered, deterministic under
/// `list_for_recall`'s stable ordering). Rows whose vector length differs
/// from `query_vec` are skipped (dimension guard, defense-in-depth). Output
/// is sorted by score descending with an article-id ascending tiebreak so
/// equal-score articles order deterministically.
#[must_use]
pub fn pool_hits(rows: &[EmbeddingRow], query_vec: &[f32]) -> Vec<EmbeddingHit> {
    /* `f32::NEG_INFINITY` is the identity for `max` over f32. `f32::MIN` was
    also correct for cosine `[-1, 1]` (`-3.4e38 < -1`), but `NEG_INFINITY`
    communicates intent more clearly. */
    let mut best: HashMap<String, (f32, i32)> = HashMap::new();
    for row in rows {
        if row.embedding.len() != query_vec.len() {
            continue; // dimension guard (defense-in-depth)
        }
        let sim = cosine_similarity(query_vec, &row.embedding);
        let entry =
            best.entry(row.article_id.clone()).or_insert((f32::NEG_INFINITY, row.chunk_index));
        if sim > entry.0 {
            *entry = (sim, row.chunk_index);
        }
    }
    let mut hits: Vec<EmbeddingHit> = best
        .into_iter()
        .map(|(article_id, (score, chunk_index))| EmbeddingHit {
            article_id,
            score,
            chunk_index: Some(chunk_index),
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.article_id.cmp(&b.article_id))
    });
    hits
}
