//! Bounded cosine recall: embed query → max-pool cosine per article → top-K IDs.
//! Candidate pool bounded by `included` corpus; filtered to current model's dimensions
//! so provider switches don't mix incompatible vectors.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::db::connection::{lock_conn, DbState};
use crate::db::embedding_repo::{self, EmbeddingRow};
use crate::db::llm_config_repo;
use crate::embedding::local::prompt::EmbeddingRole;
use crate::embedding::service::EmbeddingService;
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
/// Embeds query via the backend router → max-pools cosine per article →
/// top-K sorted descending. `status_filter`: non-empty → scoped to those
/// statuses; empty → all. Returns empty vec on disabled/empty/failure
/// (caller falls back to LIKE).
pub async fn recall(
    db_state: &State<'_, DbState>,
    orchestrator: &Arc<LlmOrchestrator>,
    engine: &Arc<crate::embedding::local::engine::LocalEngine>,
    query: &str,
    top_k: usize,
    status_filter: &[String],
) -> Result<Vec<EmbeddingHit>, AppError> {
    let top_k = if top_k == 0 { 30 } else { top_k };

    // Read config + status + model + dimensions + backend + storage root under
    // one brief lock. The storage-root read is side-effect-free (plain
    // `get_setting`: no lazy migration, no persist, no `create_dir_all`) so
    // the cloud path stays byte-identical and an unwritable root can never
    // fail recall; a missing root reads as "local not installed" downstream.
    // The backend + storage root feed the service router so query embedding
    // follows the user's backend selection (Configured Provider vs Bango
    // Local).
    let (config, status, model, dimensions, backend, storage_root) = {
        let conn = lock_conn(&db_state.conn)?;
        let cfg = llm_config_repo::get_config(&conn)?;
        let st = app_settings_repo::get_embedding_status(&conn)?;
        let m = app_settings_repo::get_embedding_model(&conn)?.unwrap_or_default();
        let d = app_settings_repo::get_embedding_dimensions(&conn)?;
        let b = app_settings_repo::get_embedding_backend(&conn)?;
        let root = app_settings_repo::get_setting(&conn, app_settings_repo::STORAGE_ROOT_KEY)?
            .filter(|s| !s.is_empty())
            .unwrap_or_default();
        (cfg, st, m, d, b, root)
    };

    if status != EmbeddingStatus::Enabled || dimensions <= 0 {
        return Ok(Vec::new());
    }
    let Some(cfg) = config else {
        return Ok(Vec::new());
    };

    // Embed the query through the backend router (no DB lock held). The cloud
    // branch is the previous direct `orchestrator.send_embedding` call, so the
    // default backend's behavior is unchanged.
    let service = EmbeddingService::new(Arc::clone(orchestrator), Arc::clone(engine));
    let query_vec = match service
        .embed(
            backend,
            Path::new(&storage_root),
            &cfg,
            &[query.to_string()],
            &model,
            EmbeddingRole::Query,
        )
        .await
    {
        Ok((vectors, _)) => vectors.into_iter().next().unwrap_or_default(),
        Err(e) => {
            /* Operational failure (cloud provider or local backend): keep the
            empty-result contract (LIKE fallback / funnel outcome), but make
            it observable - stderr + best-effort audit entry in its own lock
            burst (nothing is held across the awaited embed call). */
            eprintln!("[embedding] query embed failed: {e}");
            crate::db::audit_repo::log_error_best_effort(
                &db_state.conn,
                &format!("embedding query failed: {e}"),
            );
            return Ok(Vec::new());
        }
    };
    if query_vec.is_empty() {
        return Ok(Vec::new());
    }

    // Load candidate rows (brief lock). The MODEL filter (L2) is the
    // authoritative cross-backend guard: same-dimension models (Google
    // text-embedding-004 vs EmbeddingGemma, both 768) must never be compared.
    // An empty stored model keeps the legacy dims-only behavior.
    let rows = {
        let conn = lock_conn(&db_state.conn)?;
        embedding_repo::list_for_recall(&conn, dimensions, Some(&model), status_filter)?
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
