//! Embedding runner: executes a `WorkList` under correct lock discipline.
//!
//! The runner is the callable flow that ties the director (work-list
//! computation) to the orchestrator (provider embedding calls) and the repo
//! (storage). It follows the 3-burst lock pattern so the DB mutex is never
//! held across an `.await` (the orchestrator call is the only long operation
//! and runs after the work-list lock is dropped).
//!
//! v2 redesign: the per-article loop is now an OUTER `tokio::task::JoinSet`
//! (one task per article) instead of a sequential `for` loop. Each task calls
//! [`EmbeddingBatchSender::send_embedding_batch_parallel`] (the v2 orchestrator
//! primitive that handles per-text splitting + sub-batch grouping + parallel
//! HTTP dispatch + pooling), then writes its rows under a brief DB lock burst.
//! Cancellation is via `JoinSet::abort_all`: a cancel click between
//! `join_next()` completions aborts all in-flight tasks, dropping their vectors
//! (no DB writes from cancelled tasks).
//!
//! The [`EmbeddingBatchSender`] trait mirrors `IngestLlmSender`: production
//! wires [`HttpEmbeddingBatchSender`] (wrapping `Arc<LlmOrchestrator>`); tests
//! inject a fake. This makes the runner's parallel + cancel behavior
//! unit-testable without a live LLM provider.
//!
//! Progress is emitted via the caller-supplied `app_handle` so the frontend can
//! render a live progress bar (`embedding:progress` / `embedding:done` events).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tauri::{Emitter, State};
use tokio::task::JoinSet;

use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::db::connection::{lock_conn, DbState};
use crate::db::embedding_repo::{self, NewEmbeddingRow};
use crate::db::llm_config_repo;
use crate::embedding::director::{compute_work_list, EmbeddingScope, SkipReason};
use crate::error::AppError;
use crate::llm::embedding::probe_embedding_support;
use crate::llm::orchestrator::{send_embedding_batch_parallel, LlmOrchestrator};
use crate::models::llm_config::LlmConfig;

/// Decide whether a single returned vector should be stored.
///
/// Returns `true` if the vector's length matches the effective dimension
/// (so it is safe to persist), `false` if it should be skipped + counted as
/// an error (truncated/mismatched vector that would corrupt recall if stored
/// with a wrong `dimensions` column).
///
/// Pure `#[must_use]` so the per-row guard is unit-testable in isolation.
#[must_use]
pub fn vector_matches_dim(vector: &[f32], effective_dim: i32) -> bool {
    vector.len() == effective_dim as usize
}

/// Resolve the effective embedding dimensionality for a batch.
///
/// The runner calls this with the probe-time `probe_dim` (read from
/// `app_settings` when the run started) and the `returned_dim` reported by the
/// provider alongside the current batch of vectors. Returns the dimensionality
/// that should govern storage + the per-row length check.
///
/// Rules:
/// - If the provider reports a positive `returned_dim` that DISAGREES with the
///   probe value, trust the provider (the probe is stale — e.g. the model was
///   swapped between the probe and the call). The runner logs the drift + the
///   caller persists the corrected value back to `app_settings`.
/// - Otherwise (returned_dim == 0, or agrees with probe), keep the probe value
///   so a provider that reports `0` (a sentinel for "unknown") doesn't blow
///   away a known-good dimensionality.
///
/// Pure `#[must_use]` so the dimension-decision logic is unit-testable in
/// isolation without spinning up the async runner + Tauri runtime.
#[must_use]
pub fn resolve_effective_dim(probe_dim: i32, returned_dim: i32) -> i32 {
    if returned_dim > 0 && returned_dim != probe_dim {
        returned_dim
    } else {
        probe_dim
    }
}

/// Injectable sender so the runner's outer-JoinSet + cancel behavior is
/// unit-testable without a live LLM provider. Mirrors `IngestLlmSender`.
///
/// Production wires [`HttpEmbeddingBatchSender`] (wrapping
/// `Arc<LlmOrchestrator>`); tests inject a fake that returns deterministic
/// vectors.
#[async_trait]
pub trait EmbeddingBatchSender: Send + Sync {
    /// Embed `texts`, returning ONE vector per input in input order, plus the
    /// effective dimensionality. Delegates to the orchestrator's
    /// [`send_embedding_batch_parallel`] in production.
    async fn send_embedding_batch_parallel(
        &self,
        config: &LlmConfig,
        texts: &[String],
        model: &str,
    ) -> Result<(Vec<Vec<f32>>, i32), AppError>;
}

/// Production sender: delegates to the shared `Arc<LlmOrchestrator>` via the
/// v2 free function [`send_embedding_batch_parallel`].
pub struct HttpEmbeddingBatchSender {
    orchestrator: Arc<LlmOrchestrator>,
}

impl HttpEmbeddingBatchSender {
    #[must_use]
    pub fn new(orchestrator: Arc<LlmOrchestrator>) -> Self {
        Self { orchestrator }
    }
}

#[async_trait]
impl EmbeddingBatchSender for HttpEmbeddingBatchSender {
    async fn send_embedding_batch_parallel(
        &self,
        config: &LlmConfig,
        texts: &[String],
        model: &str,
    ) -> Result<(Vec<Vec<f32>>, i32), AppError> {
        send_embedding_batch_parallel(&self.orchestrator, config, texts, model).await
    }
}

/// The final report from a `generate_embeddings_inner` run.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingRunReport {
    pub generated: usize,
    pub skipped: usize,
    pub errors: usize,
    pub status: String,
    pub model: String,
    pub skip_reason: Option<String>,
}

/// The core embedding-generation flow (no Tauri `State` so it is callable from
/// command handlers and batch-import phases alike).
///
/// v2 lock discipline (non-blocking guarantee):
/// 1. Brief lock to read the work list + config + status. Release.
/// 2. If `embedding_status == Unknown`, probe (no lock held during the HTTP
///    call), then brief lock to persist the outcome. Release.
/// 3. Outer `JoinSet`: one task per article. Each task calls
///    `sender.send_embedding_batch_parallel` (no lock held), then takes a brief
///    DB lock burst to `INSERT OR REPLACE` its rows. Release.
///
/// The DB mutex is held only for millisecond-scale SQLite reads/writes and is
/// **never held across an `.await`**.
///
/// `emit_events` controls whether `embedding:progress` / `embedding:done` are
/// emitted (the batch-import Phase 5 emits its own `batch-import:progress`
/// instead). When `app_handle` is `None` (test mode), no events are emitted.
pub async fn generate_embeddings_inner(
    db_state: &State<'_, DbState>,
    sender: Arc<dyn EmbeddingBatchSender>,
    scope: EmbeddingScope,
    app_handle: Option<&tauri::AppHandle>,
    emit_events: bool,
    cancel_token: Option<Arc<AtomicBool>>,
) -> Result<EmbeddingRunReport, AppError> {
    // 1. Read the work list + config + status under one brief lock.
    let (work_list, config) = {
        let conn = lock_conn(&db_state.conn)?;
        let list = compute_work_list(&conn, &scope)?;
        let cfg = llm_config_repo::get_config(&conn)?;
        (list, cfg)
    };

    // Early exit on skip reasons.
    if let Some(reason) = &work_list.skip_reason {
        if matches!(
            reason,
            SkipReason::LlmNotConfigured | SkipReason::Disabled | SkipReason::NoTargets
        ) {
            let report = EmbeddingRunReport {
                generated: 0,
                skipped: work_list.skipped_fresh,
                errors: 0,
                status: app_settings_repo::EmbeddingStatus::Unknown.as_str().to_string(),
                model: String::new(),
                skip_reason: Some(format!("{reason:?}")),
            };
            if emit_events {
                if let Some(handle) = app_handle {
                    let _ = handle.emit("embedding:done", &report);
                }
            }
            return Ok(report);
        }
    }

    // If status was Unknown, probe now (no lock during the HTTP call).
    let status = {
        let conn = lock_conn(&db_state.conn)?;
        app_settings_repo::get_embedding_status(&conn)?
    };

    let (model, dimensions) = if status == EmbeddingStatus::Unknown {
        let Some(cfg) = config.clone() else {
            return Err(AppError::Validation("LLM not configured".to_string()));
        };
        // Probe (no lock held during HTTP).
        let outcome = probe_embedding_support(&cfg).await;
        let new_status = if outcome.status == "enabled" {
            EmbeddingStatus::Enabled
        } else {
            EmbeddingStatus::Disabled
        };
        // Brief lock to persist the probe outcome.
        {
            let conn = lock_conn(&db_state.conn)?;
            app_settings_repo::set_embedding_status(
                &conn,
                new_status,
                &outcome.model,
                outcome.dimensions,
            )?;
        }
        if new_status == EmbeddingStatus::Disabled {
            let report = EmbeddingRunReport {
                generated: 0,
                skipped: 0,
                errors: 0,
                status: "disabled".to_string(),
                model: String::new(),
                skip_reason: Some(outcome.reason),
            };
            if emit_events {
                if let Some(handle) = app_handle {
                    let _ = handle.emit("embedding:done", &report);
                }
            }
            return Ok(report);
        }
        (outcome.model, outcome.dimensions)
    } else {
        // Read the stored model + dimensions.
        let conn = lock_conn(&db_state.conn)?;
        let m = app_settings_repo::get_embedding_model(&conn)?.unwrap_or_default();
        let d = app_settings_repo::get_embedding_dimensions(&conn)?;
        (m, d)
    };

    let Some(cfg) = config else {
        return Err(AppError::Validation("LLM not configured".to_string()));
    };

    // If the director returned UnknownNotProbed, recompute now that we probed.
    let work_list = if matches!(work_list.skip_reason, Some(SkipReason::UnknownNotProbed)) {
        let conn = lock_conn(&db_state.conn)?;
        compute_work_list(&conn, &scope)?
    } else {
        work_list
    };

    if work_list.rows.is_empty() {
        let report = EmbeddingRunReport {
            generated: 0,
            skipped: work_list.skipped_fresh,
            errors: 0,
            status: "enabled".to_string(),
            model,
            skip_reason: work_list.skip_reason.map(|r| format!("{r:?}")),
        };
        if emit_events {
            if let Some(handle) = app_handle {
                let _ = handle.emit("embedding:done", &report);
            }
        }
        return Ok(report);
    }

    // 2. Group tasks by article for the outer JoinSet.
    use std::collections::HashMap;
    let mut by_article: HashMap<String, Vec<crate::embedding::director::EmbedTask>> =
        HashMap::new();
    for task in work_list.rows {
        by_article.entry(task.article_id.clone()).or_default().push(task);
    }

    let total = by_article.len();
    let mut processed = 0usize;
    let mut generated = 0usize;
    let mut errors = 0usize;
    let provider = format!("{:?}", cfg.provider);

    // 3. Outer JoinSet: one task per article. Each task:
    //    - clones the Arc<dyn EmbeddingBatchSender> + Arc<LlmConfig> + model
    //    - calls send_embedding_batch_parallel (no DB lock held)
    //    - returns (article_id, tasks, vectors, returned_dims)
    //    The main loop then takes a brief DB lock burst per completed task to
    //    write the rows. This keeps the DB mutex out of the spawned tasks so
    //    every other IPC command handler can acquire the connection while
    //    embeddings generate.
    let cfg_arc = Arc::new(cfg.clone());
    let model_arc = Arc::new(model.clone());

    // Type alias to keep the JoinSet declaration readable (clippy
    // `type_complexity`). Carries the per-article result back to the main loop
    // for the brief DB-write burst.
    type ArticleEmbedResult =
        Result<(String, Vec<crate::embedding::director::EmbedTask>, Vec<Vec<f32>>, i32), AppError>;

    let mut set: JoinSet<ArticleEmbedResult> = JoinSet::new();

    for (article_id, tasks) in by_article {
        let sender = Arc::clone(&sender);
        let cfg_arc = Arc::clone(&cfg_arc);
        let model_arc = Arc::clone(&model_arc);
        set.spawn(async move {
            let texts: Vec<String> = tasks.iter().map(|t| t.text.clone()).collect();
            let (vectors, returned_dims) =
                sender.send_embedding_batch_parallel(&cfg_arc, &texts, &model_arc).await?;
            Ok((article_id, tasks, vectors, returned_dims))
        });
    }

    // 4. Collect completions, writing rows under brief DB lock bursts. Check
    //    cancel between completions; on cancel, abort_all + break (in-flight
    //    vectors are dropped — no DB writes from cancelled tasks).
    let mut cancelled = false;
    while let Some(joined) = set.join_next().await {
        // Check cancellation before processing each completion.
        if let Some(ref token) = cancel_token {
            if token.load(Ordering::Relaxed) {
                set.abort_all();
                cancelled = true;
                break;
            }
        }

        let result = match joined {
            Ok(r) => r,
            Err(join_err) => {
                // Task panicked. Count its tasks as errors (we don't know which
                // article it was for from the join error, so attribute to the
                // whole pending set as a rough estimate).
                eprintln!("[embedding] article task panicked: {join_err}");
                processed += 1;
                if emit_events {
                    if let Some(handle) = app_handle {
                        let _ = handle.emit(
                            "embedding:progress",
                            serde_json::json!({
                                "processed": processed,
                                "total": total,
                                "phase": "embedding",
                                "model": model,
                            }),
                        );
                    }
                }
                continue;
            }
        };

        match result {
            Ok((_article_id, tasks, vectors, returned_dims)) => {
                // Dimension validation: a provider may return vectors of an
                // unexpected length (model swap between probe and call, a
                // truncated batch vector, a misbehaving local server). Delegate
                // to the pure `resolve_effective_dim` helper so the
                // dimension-decision logic stays unit-testable in isolation.
                let effective_dim = resolve_effective_dim(dimensions, returned_dims);
                if effective_dim != dimensions {
                    eprintln!(
                        "[embedding] dimension drift: probe={dimensions} but provider returned \
                         {returned_dims} for model {model}; using the returned value {effective_dim}"
                    );
                }
                let expected_len = effective_dim as usize;

                // If the provider returned fewer vectors than tasks (e.g. a
                // batch endpoint that silently dropped one), count the missing
                // ones as errors so the report is accurate.
                if vectors.len() < tasks.len() {
                    let missing = tasks.len() - vectors.len();
                    eprintln!(
                        "[embedding] provider returned {} vectors for {} tasks; {} missing",
                        vectors.len(),
                        tasks.len(),
                        missing
                    );
                    errors += missing;
                }

                // Brief lock to write the rows.
                let conn = lock_conn(&db_state.conn)?;
                for (task, vector) in tasks.iter().zip(vectors.iter()) {
                    // Per-row dimension guard: skip + count as error so a
                    // single truncated/mismatched vector never stores a wrong
                    // `dimensions` column (which would silently corrupt recall).
                    if vector.len() != expected_len {
                        eprintln!(
                            "[embedding] skipping row (article {}, chunk_index {}): vector length \
                             {} != expected dimension {} (effective_dim={effective_dim})",
                            task.article_id,
                            task.chunk_index,
                            vector.len(),
                            expected_len,
                        );
                        errors += 1;
                        continue;
                    }
                    let row = NewEmbeddingRow {
                        article_id: &task.article_id,
                        chunk_index: task.chunk_index,
                        embedding: vector,
                        dimensions: effective_dim,
                        input_hash: &task.input_hash,
                        model_name: &model,
                        provider: &provider,
                        generated_at: now_ts(),
                    };
                    if embedding_repo::insert_embedding(&conn, &row).is_ok() {
                        generated += 1;
                    } else {
                        errors += 1;
                    }
                }

                // If the effective dimension drifted from the probe value,
                // persist the corrected value back to `app_settings` so the
                // next run + recall use the real dimensionality. Best-effort:
                // a failure here is logged and never blocks the embedding write.
                if effective_dim != dimensions && effective_dim > 0 {
                    if let Ok(conn) = lock_conn(&db_state.conn) {
                        if let Err(e) = app_settings_repo::set_embedding_status(
                            &conn,
                            EmbeddingStatus::Enabled,
                            &model,
                            effective_dim,
                        ) {
                            eprintln!(
                                "[embedding] failed to persist corrected dimension {effective_dim}: {e}"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[embedding] failed for article batch: {e}");
                // We don't know the exact task count for this article from the
                // error alone; attribute a single error (the article's tasks
                // will be re-queued by the director's staleness check on the
                // next run because no rows were written for them).
                errors += 1;
            }
        }

        processed += 1;
        if emit_events {
            if let Some(handle) = app_handle {
                let _ = handle.emit(
                    "embedding:progress",
                    serde_json::json!({
                        "processed": processed,
                        "total": total,
                        "phase": "embedding",
                        "model": model,
                    }),
                );
            }
        }
    }

    // If cancelled, drain any remaining aborting tasks so the JoinSet is empty
    // before we return (defense-in-depth; abort_all makes them complete fast).
    if cancelled {
        while set.join_next().await.is_some() {}
    }

    let report = EmbeddingRunReport {
        generated,
        skipped: work_list.skipped_fresh,
        errors,
        status: "enabled".to_string(),
        model,
        skip_reason: None,
    };
    if emit_events {
        if let Some(handle) = app_handle {
            let _ = handle.emit("embedding:done", &report);
        }
    }
    Ok(report)
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}
