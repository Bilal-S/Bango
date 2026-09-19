//! Citation Finder search pipeline.
//!
//! `find_citations_inner` is the spawn-safe core (no `tauri::State`).
//! Three-phase one-button flow:
//! - Phase A: readiness check (brief lock) → decide whether Phase B runs.
//! - Phase B: (conditional) auto-prepare embeddings via
//!   `generate_embeddings_inner` with the same cancel token.
//! - Phase C: claim-split (per-statement only) → recall → containment
//!   passage → LLM classify → merge into `CitationResult[]`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{Manager, State};

use super::claim_splitter::{
    build_claim_splitter_prompt, enforce_max_claims, CLAIM_SPLITTER_SYSTEM_PROMPT,
};
use super::prompt::{
    build_per_statement_prompt, build_whole_block_prompt, ground_quotes, parse_citation_outputs,
    parse_classification, CandidateMetadata, CandidatePassage, CitationLlmOutput,
    CITATION_FINDER_SYSTEM_PROMPT,
};
use super::readiness::compute_readiness;
use super::similarity::{containment, find_best_passage, tokenize_and_stem, MIN_PASSAGE_SCORE};
use crate::citation_finder::{
    filter_valid_statuses, CitationFinderMode, CitationFinderProgress, CitationFunnel,
    CitationMatch, CitationResult,
};
use crate::db::article_repo;
use crate::db::chunk_repo;
use crate::db::connection::{lock_conn, DbState};
use crate::db::llm_config_repo;
use crate::embedding::director::EmbeddingScope;
use crate::embedding::recall::{self, EmbeddingHit};
use crate::embedding::runner::{generate_embeddings_inner, EmbeddingBatchSender};
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::utils::chunking::Chunk;

/// Injectable sender trait (mirrors `EmbeddingBatchSender`).
#[async_trait::async_trait]
pub trait CitationLlmSender: Send + Sync {
    /// Run the main classification call. Returns prepared JSON string
    /// (already through `prepare_llm_json`).
    async fn send_classification(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AppError>;

    /// Run the claim-split call (per-statement only). Returns prepared JSON.
    async fn send_claim_split(&self, text: &str) -> Result<String, AppError>;

    /// Run embedding recall for one query text. Returns top-K hits with cosine scores.
    async fn recall(
        &self,
        query: &str,
        top_k: usize,
        statuses: &[String],
    ) -> Result<Vec<EmbeddingHit>, AppError>;
}

/// Production sender wrapping `Arc<LlmOrchestrator>` + `DbState` (via the
/// `AppHandle` so the sender is `'static` + cloneable into spawned tasks).
pub struct HttpCitationLlmSender {
    pub orchestrator: Arc<LlmOrchestrator>,
    pub app_handle: tauri::AppHandle,
}

#[async_trait::async_trait]
impl CitationLlmSender for HttpCitationLlmSender {
    async fn send_classification(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AppError> {
        let config = {
            let db = self.app_handle.state::<DbState>();
            let conn = lock_conn(&db.conn)?;
            llm_config_repo::get_config(&conn)?
        };
        let Some(cfg) = config else {
            return Err(AppError::Validation("LLM not configured".to_string()));
        };
        let (json, _tokens) = self
            .orchestrator
            .send_json(&cfg, system_prompt, user_prompt, LlmRequestType::CitationFinder)
            .await?;
        Ok(json)
    }

    async fn send_claim_split(&self, text: &str) -> Result<String, AppError> {
        let config = {
            let db = self.app_handle.state::<DbState>();
            let conn = lock_conn(&db.conn)?;
            llm_config_repo::get_config(&conn)?
        };
        let Some(cfg) = config else {
            return Err(AppError::Validation("LLM not configured".to_string()));
        };
        let user_prompt = build_claim_splitter_prompt(text);
        let (json, _tokens) = self
            .orchestrator
            .send_json(
                &cfg,
                CLAIM_SPLITTER_SYSTEM_PROMPT,
                &user_prompt,
                LlmRequestType::CitationFinderSplit,
            )
            .await?;
        Ok(json)
    }

    async fn recall(
        &self,
        query: &str,
        top_k: usize,
        statuses: &[String],
    ) -> Result<Vec<EmbeddingHit>, AppError> {
        let db_state = self.app_handle.state::<DbState>();
        let engine = self
            .app_handle
            .state::<Arc<crate::embedding::local::engine::LocalEngine>>()
            .inner()
            .clone();
        recall::recall(&db_state, &self.orchestrator, &engine, query, top_k, statuses).await
    }
}

/// One passage-evidence entry per surviving article (per claim). Exactly one
/// entry per (article, claim): the primary passage plus optional abstract
/// context for the LLM.
struct PassageEvidence {
    article_id: String,
    /// Primary passage shown to the LLM and quoted in the result card:
    /// the containment-best chunk, else the cosine-best chunk (embedding-
    /// vouched fallback), else the abstract.
    passage: String,
    section: Option<String>,
    /// Containment score of `passage`. May be < `MIN_PASSAGE_SCORE` for the
    /// cosine-chunk fallback (the embedding layer vouches for relevance);
    /// finalist ranking treats such articles accordingly.
    score: f64,
    /// Abstract context (`title + "\n\n" + abstract`) attached when `passage`
    /// is a chunk so the classifier also sees the paper's thesis. `None`
    /// when `passage` already is the abstract.
    abstract_text: Option<String>,
}

/// One input claim: text + recall hits + per-candidate best passage.
struct ClaimWork {
    text: String,
    hits: Vec<EmbeddingHit>,
    /// One `PassageEvidence` per hit article with usable evidence. Articles
    /// where no chunk, cosine chunk, or abstract cleared the gate are absent.
    passages: Vec<PassageEvidence>,
}

/// Pooled finalists across claims: union of article IDs + per-claim passages.
struct Finalists {
    article_ids: Vec<String>,
    per_claim: Vec<ClaimWork>,
}

/// Bundles input + runtime params (mirrors screening's `RunSyncContext`).
pub struct FindCitationsContext<'a> {
    pub text: String,
    pub mode: CitationFinderMode,
    pub status_filter: Vec<String>,
    pub cancel_token: Arc<AtomicBool>,
    pub emit_progress: &'a (dyn Fn(CitationFinderProgress) + Send + Sync),
    pub app_handle: Option<tauri::AppHandle>,
}

/// Cancel poll interval for long-running awaits.
const CANCEL_POLL_MS: u64 = 150;

/// The single cancellation error shape the frontend matches as "Cancelled".
fn cancelled_error() -> AppError {
    AppError::Import("Cancelled".to_string())
}

/// Await a fallible future, aborting with `Cancelled` as soon as `cancel` is
/// set (polled every [`CANCEL_POLL_MS`]). Dropping the future aborts the
/// underlying HTTP request instead of letting it run to completion.
async fn await_cancellable<T>(
    cancel: &Arc<AtomicBool>,
    fut: impl std::future::Future<Output = Result<T, AppError>>,
) -> Result<T, AppError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    tokio::pin!(fut);
    let watcher = async {
        loop {
            tokio::time::sleep(Duration::from_millis(CANCEL_POLL_MS)).await;
            if cancel.load(Ordering::Relaxed) {
                break;
            }
        }
    };
    tokio::select! {
        value = fut => value,
        () = watcher => Err(cancelled_error()),
    }
}

/// The core spawn-safe search pipeline. Returns `Vec<CitationResult>` —
/// one entry per claim (per-statement) or single entry with `claim: None`
/// (whole-block). `ctx.emit_progress` is called with phase-appropriate
/// payloads; the caller owns event-emission plumbing.
pub async fn find_citations_inner(
    db_state: &State<'_, DbState>,
    embedding_sender: Arc<dyn EmbeddingBatchSender>,
    llm_sender: Arc<dyn CitationLlmSender>,
    ctx: FindCitationsContext<'_>,
) -> Result<Vec<CitationResult>, AppError> {
    let FindCitationsContext { text, mode, status_filter, cancel_token, emit_progress, app_handle } =
        ctx;
    /* Apply the status whitelist at the command boundary. The backend does NOT
    assume a default — an empty filter returns "No articles match the selected
    filters." `duplicate` is always dropped (never a citation candidate). */
    let status_filter = filter_valid_statuses(&status_filter);
    // ═══════════════════════════════════════════════════════════════════
    //  Phase A: readiness check (brief lock)
    // ═══════════════════════════════════════════════════════════════════
    let readiness = {
        let conn = lock_conn(&db_state.conn)?;
        compute_readiness(&conn, &status_filter)?
    };
    if !readiness.provider_supports_embeddings {
        // Backend-aware message (T7): with `bango_local` selected, a Disabled
        // status means the local components are missing OR the last offline
        // probe's self-test failed - the chat provider is irrelevant either
        // way.
        return Err(AppError::Import(if readiness.embedding_backend == "bango_local" {
            "Bango Local embeddings are unavailable (not installed, or the last self-test \
                 failed). Open Settings - Embeddings to download or re-verify them, or \
                 switch back to your configured provider."
                .to_string()
        } else {
            "Provider does not support embeddings. Configure an embedding-capable LLM \
                 provider."
                .to_string()
        }));
    }
    if readiness.total_articles == 0 {
        emit_progress(CitationFinderProgress {
            phase: "searching".to_string(),
            stage: None,
            done: 0,
            total: 0,
            overall_percent: 100,
            message: "No articles match the selected filters.".to_string(),
            is_running: false,
            is_cancelled: false,
            funnel: None,
        });
        return Ok(vec![CitationResult { claim: None, matches: vec![] }]);
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Phase B: auto-prepare embeddings (conditional)
    // ═══════════════════════════════════════════════════════════════════
    if readiness.coverage_pct < 100.0 {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let embedded_count_us = readiness.embedded_count.max(0) as usize;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let total_articles_us = readiness.total_articles.max(0) as usize;
        emit_progress(CitationFinderProgress {
            phase: "preparing_embeddings".to_string(),
            stage: None,
            done: embedded_count_us,
            total: total_articles_us,
            // Phase B covers 0-90% of the overall bar. The initial snapshot
            // reflects the pre-prepare coverage; subsequent updates arrive via
            // the frontend's `embedding:progress` listener, which translates
            // each `{processed, total}` into the same 0-90 range.
            overall_percent: phase_b_overall_percent(embedded_count_us, total_articles_us),
            message: format!(
                "Preparing embeddings… {}/{} articles",
                readiness.embedded_count, readiness.total_articles
            ),
            is_running: true,
            is_cancelled: false,
            funnel: None,
        });
        // Reuse the embedding runner. The director's `EmbeddingScope.status_filter`
        // is `Option<String>` (single comma-joined value), matching the existing
        // `generate_embeddings` command contract.
        let scope = EmbeddingScope {
            article_ids: None,
            status_filter: Some(status_filter.join(",")),
            force: false,
        };
        // Pass `app_handle` + `emit_events=true` so the runner emits its
        // per-article `embedding:progress` events. The frontend's
        // `use-citation-finder.ts` subscribes during Phase B and translates
        // each into a `citation:progress` update (the 0-90% range), avoiding
        // the single-snapshot-then-freeze behavior. `None` (tests) → no events.
        let _report = generate_embeddings_inner(
            db_state,
            embedding_sender,
            scope,
            app_handle.as_ref(),
            app_handle.is_some(),
            Some(Arc::clone(&cancel_token)),
        )
        .await?;

        if cancel_token.load(Ordering::Relaxed) {
            return Err(AppError::Import("Cancelled".to_string()));
        }

        /* Deliberately NO post-prepare 100%-coverage re-check. The previous
        gate hard-failed when coverage stayed below 100%, but coverage can
        legitimately plateau: articles with no embeddable content (empty
        title + empty abstract + no full-text chunks) produce zero
        `EmbedTask`s, permanently staying outside the numerator. The recall
        layer naturally handles this — articles without embedding rows are
        absent from the candidate pool (correct: they have no semantic
        signal). Real errors (DB lock failures, provider outage) still
        propagate via the `?` on `generate_embeddings_inner` above. */
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Phase C: search pipeline
    // ═══════════════════════════════════════════════════════════════════
    run_phase_c(
        db_state.inner(),
        &text,
        mode,
        &status_filter,
        &llm_sender,
        &cancel_token,
        emit_progress,
    )
    .await
}

/// Phase C search core: claim-split (per-statement only) → recall → passage
/// evidence → LLM classify → merge into `CitationResult[]`.
///
/// Takes `&DbState` (NOT Tauri `State`) so integration tests can drive the
/// full pipeline with mock senders against a seeded temp DB
/// (`tests/citation_finder/citation_finder_pipeline_test.rs`). Phases A/B
/// need the `AppHandle` for config reads + event emission and stay in
/// [`find_citations_inner`].
pub async fn run_phase_c(
    db_state: &DbState,
    text: &str,
    mode: CitationFinderMode,
    status_filter: &[String],
    llm_sender: &Arc<dyn CitationLlmSender>,
    cancel_token: &Arc<AtomicBool>,
    emit_progress: &(dyn Fn(CitationFinderProgress) + Send + Sync),
) -> Result<Vec<CitationResult>, AppError> {
    match mode {
        CitationFinderMode::WholeBlock => {
            run_whole_block(text, status_filter, llm_sender, db_state, cancel_token, emit_progress)
                .await
        }
        CitationFinderMode::PerStatement => {
            run_per_statement(
                text,
                status_filter,
                llm_sender,
                db_state,
                cancel_token,
                emit_progress,
            )
            .await
        }
    }
}

/// Whole-block pipeline: one query, one LLM classification call.
async fn run_whole_block(
    text: &str,
    status_filter: &[String],
    llm_sender: &Arc<dyn CitationLlmSender>,
    db_state: &DbState,
    cancel_token: &Arc<AtomicBool>,
    emit_progress: &(dyn Fn(CitationFinderProgress) + Send + Sync),
) -> Result<Vec<CitationResult>, AppError> {
    eprintln!("[citation] stage=embedding_query start");
    emit_progress(searching_progress("embedding_query", "Embedding query…"));

    let hits = await_cancellable(cancel_token, llm_sender.recall(text, 30, status_filter)).await?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    if hits.is_empty() {
        /* Empty recall is also a funnel outcome (embedding miss, empty pool,
        or a swallowed embedding-API error) - report it instead of returning
        silently. */
        emit_funnel_progress(
            emit_progress,
            CitationFunnel { recalled: 0, ..CitationFunnel::default() },
        );
        return Ok(vec![CitationResult { claim: None, matches: vec![] }]);
    }

    eprintln!("[citation] stage=ranking start");
    emit_progress(searching_progress("ranking", "Ranking passages…"));
    let user_tokens = tokenize_and_stem(text);
    let work = build_claim_work(&user_tokens, text, hits, db_state).await?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }

    eprintln!("[citation] stage=classifying start");
    emit_progress(searching_progress("classifying", "Classifying…"));
    let finalists = pool_finalists(vec![work]);
    let metadata = load_metadata(db_state, &finalists.article_ids).await?;
    /* Build one CandidatePassage per finalist (best passage per article).
    Inlined here (mirrors per-statement path) - one passage-building pattern. */
    let passages: Vec<CandidatePassage> = finalists
        .per_claim
        .iter()
        .flat_map(|w| {
            w.passages.iter().map(|ev| CandidatePassage {
                article_id: ev.article_id.clone(),
                claim: None,
                passage: ev.passage.clone(),
                section: ev.section.clone(),
                abstract_text: ev.abstract_text.clone(),
            })
        })
        .collect();
    let user_prompt = build_whole_block_prompt(text, &passages, &metadata);
    /* Cancel check before the classification call, then the call itself
    races the cancel token (dropping the HTTP future on cancel). */
    if cancel_token.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    let classify_started = Instant::now();
    let json = await_cancellable(
        cancel_token,
        llm_sender.send_classification(CITATION_FINDER_SYSTEM_PROMPT, &user_prompt),
    )
    .await?;
    eprintln!(
        "[citation] stage=classifying end elapsed_ms={}",
        classify_started.elapsed().as_millis()
    );
    /* A classification that completed after the Cancel click is discarded. */
    if cancel_token.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    /* Lenient parse: snake_case + camelCase, object-wrapped arrays, per-element
    fault isolation (one bad entry doesn't drop the whole batch). */
    eprintln!("[citation] stage=parse start");
    let parse_started = Instant::now();
    let llm_outputs = parse_citation_outputs(&json)
        .map_err(|e| AppError::Import(format!("Citation Finder LLM returned invalid JSON: {e}")))?;
    eprintln!("[citation] stage=parse end elapsed_ms={}", parse_started.elapsed().as_millis());

    let matches = merge_outputs(&llm_outputs, &finalists, &metadata, None);
    emit_funnel_progress(emit_progress, funnel_from(&finalists, &llm_outputs, matches.len()));
    eprintln!("[citation] stage=done matches={}", matches.len());
    Ok(vec![CitationResult { claim: None, matches }])
}

/// Per-statement pipeline: claim-split → per-claim recall + passage → pool →
/// one LLM call with per-(article, claim) entries → group by claim.
async fn run_per_statement(
    text: &str,
    status_filter: &[String],
    llm_sender: &Arc<dyn CitationLlmSender>,
    db_state: &DbState,
    cancel_token: &Arc<AtomicBool>,
    emit_progress: &(dyn Fn(CitationFinderProgress) + Send + Sync),
) -> Result<Vec<CitationResult>, AppError> {
    eprintln!("[citation] stage=claim_split start");
    emit_progress(searching_progress("embedding_query", "Splitting claims…"));
    let split_json = await_cancellable(cancel_token, llm_sender.send_claim_split(text)).await?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    let raw_claims: Vec<String> = serde_json::from_str(&split_json)
        .map_err(|e| AppError::Import(format!("Claim splitter returned invalid JSON: {e}")))?;
    let claims = enforce_max_claims(raw_claims);
    if claims.is_empty() {
        // The splitter returned nothing usable; fall back to whole-block.
        return run_whole_block(
            text,
            status_filter,
            llm_sender,
            db_state,
            cancel_token,
            emit_progress,
        )
        .await;
    }

    eprintln!("[citation] stage=ranking start claims={}", claims.len());
    emit_progress(searching_progress("ranking", "Ranking passages per claim…"));
    let mut works: Vec<ClaimWork> = Vec::with_capacity(claims.len());
    for claim in &claims {
        if cancel_token.load(Ordering::Relaxed) {
            return Err(cancelled_error());
        }
        let hits =
            await_cancellable(cancel_token, llm_sender.recall(claim, 30, status_filter)).await?;
        let user_tokens = tokenize_and_stem(claim);
        let work = build_claim_work(&user_tokens, claim, hits, db_state).await?;
        works.push(work);
    }

    eprintln!("[citation] stage=classifying start");
    emit_progress(searching_progress("classifying", "Classifying…"));
    let finalists = pool_finalists(works);
    let metadata = load_metadata(db_state, &finalists.article_ids).await?;

    // Build one CandidatePassage per (article, claim). An article that matched
    // multiple claims gets multiple entries.
    let mut passages: Vec<CandidatePassage> = Vec::new();
    for per_claim in &finalists.per_claim {
        for ev in &per_claim.passages {
            passages.push(CandidatePassage {
                article_id: ev.article_id.clone(),
                claim: Some(per_claim.text.clone()),
                passage: ev.passage.clone(),
                section: ev.section.clone(),
                abstract_text: ev.abstract_text.clone(),
            });
        }
    }
    let user_prompt = build_per_statement_prompt(&claims, &passages, &metadata);
    /* Cancel check before the classification call, then the call itself
    races the cancel token (mirrors whole-block). */
    if cancel_token.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    let classify_started = Instant::now();
    let json = await_cancellable(
        cancel_token,
        llm_sender.send_classification(CITATION_FINDER_SYSTEM_PROMPT, &user_prompt),
    )
    .await?;
    eprintln!(
        "[citation] stage=classifying end elapsed_ms={}",
        classify_started.elapsed().as_millis()
    );
    /* A classification that completed after the Cancel click is discarded. */
    if cancel_token.load(Ordering::Relaxed) {
        return Err(cancelled_error());
    }
    /* Lenient parse: snake_case + camelCase, object-wrapped arrays, per-element
    fault isolation (one bad entry doesn't drop the whole batch). */
    eprintln!("[citation] stage=parse start");
    let parse_started = Instant::now();
    let llm_outputs = parse_citation_outputs(&json)
        .map_err(|e| AppError::Import(format!("Citation Finder LLM returned invalid JSON: {e}")))?;
    eprintln!("[citation] stage=parse end elapsed_ms={}", parse_started.elapsed().as_millis());

    // Group LLM outputs by claim.
    let mut results: Vec<CitationResult> = Vec::with_capacity(claims.len());
    let mut classified_total = 0usize;
    for claim in &claims {
        let matches = merge_outputs(&llm_outputs, &finalists, &metadata, Some(claim));
        classified_total += matches.len();
        results.push(CitationResult { claim: Some(claim.clone()), matches });
    }
    emit_funnel_progress(emit_progress, funnel_from(&finalists, &llm_outputs, classified_total));
    eprintln!("[citation] stage=done matches={classified_total}");
    Ok(results)
}

/// Normalize a claim string for lookup: trim + collapse internal whitespace
/// + lowercase. Makes the `(article_id, claim)` score lookup robust to
///   cosmetic LLM drift (whitespace, case) between splitter and classifier.
///
/// Pure `#[must_use]`.
#[must_use]
pub fn normalize_claim_key(claim: &str) -> String {
    let mut out = String::with_capacity(claim.len());
    let mut prev_was_space = false;
    for ch in claim.trim().chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                out.push(' ');
                prev_was_space = true;
            }
        } else {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_was_space = false;
        }
    }
    out
}

/// Build one `ClaimWork`: per recall hit, select passage evidence and drop
/// articles with none.
///
/// Evidence selection per article (first match wins):
/// 1. **Containment-best chunk** - `find_best_passage` over full-text chunks,
///    gated at `MIN_PASSAGE_SCORE` (0.3).
/// 2. **Cosine-best chunk fallback** - when no chunk clears the containment
///    gate but the recall hit carries chunk provenance (`EmbeddingHit.chunk_index`),
///    that chunk is used with its (possibly low) containment score. The
///    embedding layer vouches for semantic relevance where lexical overlap
///    fails (paraphrased claims). Caveat: stale chunk indexes after a
///    re-chunk can point elsewhere in the paper - acceptable, it is still a
///    real chunk of the right article and the abstract context mitigates.
/// 3. **Abstract** - `title + "\n\n" + abstract` gated at `MIN_PASSAGE_SCORE`
///    (abstract-only articles, or full-text articles whose chunks and abstract
///    both fail lexically but... only when 1 and 2 found nothing).
///
/// The abstract is additionally attached as `abstract_text` context whenever
/// the primary passage is a chunk, so the classifier always sees the paper's
/// thesis sentence.
///
/// **Lock discipline**: brief `lock_conn` per article, releasing between.
/// `tokio::task::yield_now()` between articles prevents mutex starvation
/// across up to 30 chunk reads.
async fn build_claim_work(
    user_tokens: &[String],
    claim_text: &str,
    hits: Vec<EmbeddingHit>,
    db_state: &DbState,
) -> Result<ClaimWork, AppError> {
    let mut passages: Vec<PassageEvidence> = Vec::new();
    for hit in &hits {
        /* Brief lock burst per article: read chunks + article (abstract),
        select evidence, then release. */
        let best = {
            let conn = lock_conn(&db_state.conn)?;
            let chunks = chunk_repo::list_chunks_for_article(&conn, &hit.article_id)?;
            let article = article_repo::get_article_by_id(&conn, &hit.article_id)?;
            let abstract_text = if article.abstract_text.trim().is_empty() {
                None
            } else {
                Some(format!("{}\n\n{}", article.title.trim(), article.abstract_text.trim()))
            };
            if chunks.is_empty() {
                // Abstract-only article: the title(+abstract) text IS the passage.
                let text = abstract_text.clone().unwrap_or_else(|| article.title.clone());
                let tokens = tokenize_and_stem(&text);
                /* Containment (query coverage), NOT Jaccard: the abstract is
                typically much longer than the query, so Jaccard would be
                diluted and drop exact-quote matches. Containment is
                length-insensitive on the document side. */
                let score = containment(user_tokens, &tokens);
                if score < MIN_PASSAGE_SCORE {
                    None
                } else {
                    Some(PassageEvidence {
                        article_id: hit.article_id.clone(),
                        passage: text,
                        section: Some("Abstract".to_string()),
                        score,
                        abstract_text: None,
                    })
                }
            } else {
                // 1) Containment-best chunk (gate 0.3).
                let by_containment = find_best_passage(user_tokens, &chunks);
                if let Some((passage, section, score)) = by_containment {
                    Some(PassageEvidence {
                        article_id: hit.article_id.clone(),
                        passage,
                        section,
                        score,
                        abstract_text: abstract_text.clone(),
                    })
                } else if let Some(chunk) = cosine_best_chunk(hit, &chunks) {
                    // 2) Cosine-best chunk fallback (embedding-vouched).
                    let score = containment(user_tokens, &tokenize_and_stem(&chunk.text));
                    Some(PassageEvidence {
                        article_id: hit.article_id.clone(),
                        passage: chunk.text.clone(),
                        section: chunk.section.clone(),
                        score,
                        abstract_text: abstract_text.clone(),
                    })
                } else if let Some(abs) = abstract_text {
                    // 3) Abstract fallback (gated).
                    let score = containment(user_tokens, &tokenize_and_stem(&abs));
                    if score < MIN_PASSAGE_SCORE {
                        None
                    } else {
                        Some(PassageEvidence {
                            article_id: hit.article_id.clone(),
                            passage: abs,
                            section: Some("Abstract".to_string()),
                            score,
                            abstract_text: None,
                        })
                    }
                } else {
                    None
                }
            }
        };
        if let Some(evidence) = best {
            passages.push(evidence);
        }
        // Yield between articles so the runtime can flush `citation:progress`
        // events and queued IPC commands get a turn at the mutex.
        tokio::task::yield_now().await;
    }
    Ok(ClaimWork { text: claim_text.to_string(), hits, passages })
}

/// Resolve the cosine-best chunk for a recall hit: `EmbeddingHit.chunk_index`
/// mapped into the article's current chunk list. Returns `None` for the
/// title+abstract row (`-1`), missing provenance, or out-of-range indexes
/// (stale rows after a re-chunk). Pure.
fn cosine_best_chunk<'a>(hit: &EmbeddingHit, chunks: &'a [Chunk]) -> Option<&'a Chunk> {
    let idx = hit.chunk_index?;
    if idx < 0 {
        return None; // title+abstract row, not a chunk
    }
    let idx = usize::try_from(idx).ok()?;
    chunks.get(idx)
}

/// Finalist-pool caps. Containment keeps its historical 15 slots; the cosine
/// union adds up to 5 semantically-strong articles that lexical overlap
/// ranked below the cut (paraphrased claims), capped at 20 total to bound
/// the classification prompt.
const FINALISTS_BY_CONTAINMENT: usize = 15;
const FINALISTS_BY_COSINE: usize = 5;
const FINALISTS_MAX: usize = 20;

/// Union the article IDs across works + keep the per-claim passages.
///
/// Ranking is a UNION of two orderings:
/// - top `FINALISTS_BY_CONTAINMENT` (15) by best containment score (lexical
///   overlap with the claim), and
/// - top `FINALISTS_BY_COSINE` (5) by best recall cosine (semantic
///   similarity), added while under `FINALISTS_MAX` (20).
///
/// Rationale: containment-only truncation let lexically-similar articles
/// evict semantically-right ones when the claim paraphrases the source
/// vocabulary (e.g. "decline in consumption" vs "reduction in household
/// purchasing"). The union guarantees strong semantic matches a slot.
///
/// Per-claim passages are FILTERED to the finalist set so the LLM prompt only
/// contains candidates that can survive `merge_outputs`' metadata check
/// (previously up to 30 raw passages were sent for 15 finalist slots).
fn pool_finalists(works: Vec<ClaimWork>) -> Finalists {
    // Best containment per article (passage survivors only).
    let mut containment: HashMap<String, f64> = HashMap::new();
    // Best cosine per article (all recall hits, including gate-dropped ones;
    // they carry no passage so they still cannot be prompted).
    let mut cosine: HashMap<String, f32> = HashMap::new();
    for work in &works {
        for ev in &work.passages {
            let entry = containment.entry(ev.article_id.clone()).or_insert(-1.0);
            if ev.score > *entry {
                *entry = ev.score;
            }
        }
        for hit in &work.hits {
            let entry = cosine.entry(hit.article_id.clone()).or_insert(f32::NEG_INFINITY);
            if hit.score > *entry {
                *entry = hit.score;
            }
        }
    }

    let mut by_containment: Vec<(String, f64)> = containment.into_iter().collect();
    by_containment.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0))
    });
    by_containment.truncate(FINALISTS_BY_CONTAINMENT);

    let mut by_cosine: Vec<(String, f32)> = cosine.into_iter().collect();
    by_cosine.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0))
    });
    by_cosine.truncate(FINALISTS_BY_COSINE);

    // Union: containment ranking first, then cosine-ranked additions.
    let mut article_ids: Vec<String> = by_containment.into_iter().map(|(id, _)| id).collect();
    for (id, _) in by_cosine {
        if article_ids.len() >= FINALISTS_MAX {
            break;
        }
        if !article_ids.contains(&id) {
            article_ids.push(id);
        }
    }

    // Filter per-claim passages to the finalist set (prompt hygiene).
    let finalists_set: std::collections::HashSet<&String> = article_ids.iter().collect();
    let per_claim = works
        .into_iter()
        .map(|mut w| {
            w.passages.retain(|ev| finalists_set.contains(&ev.article_id));
            w
        })
        .collect();
    Finalists { article_ids, per_claim }
}

/// Load metadata (title, authors, year, journal, doi) per finalist ID.
/// Brief `lock_conn` per article, releasing between. Mirrors
/// `build_claim_work`'s lock discipline.
async fn load_metadata(
    db_state: &DbState,
    article_ids: &[String],
) -> Result<HashMap<String, CandidateMetadata>, AppError> {
    let mut out = HashMap::new();
    for id in article_ids {
        let article = {
            let conn = lock_conn(&db_state.conn)?;
            article_repo::get_article_by_id(&conn, id)?
        };
        out.insert(
            id.clone(),
            CandidateMetadata {
                article_id: id.clone(),
                title: article.title,
                authors: article.authors,
                publication_year: article.publication_year,
                journal: article.journal,
                doi: article.doi,
            },
        );
        // Yield between articles so the runtime can flush progress events +
        // give queued IPC commands a turn at the mutex.
        tokio::task::yield_now().await;
    }
    Ok(out)
}

/// Merge the LLM outputs with the recall/passage data into the final
/// `CitationMatch` list.
///
/// `claim_filter`: in per-statement mode, the caller passes `Some(claim_text)`
/// to filter the LLM outputs to one claim. In whole-block mode, `None` keeps
/// all outputs.
fn merge_outputs(
    llm_outputs: &[CitationLlmOutput],
    finalists: &Finalists,
    metadata: &HashMap<String, CandidateMetadata>,
    claim_filter: Option<&str>,
) -> Vec<CitationMatch> {
    /* Build a lookup: (article_id, normalized_claim_key) → cosine score +
    best passage. The claim key is NORMALIZED so cosmetic drift (punctuation,
    whitespace, case) does not cause a score-lookup miss.

    `ArticleBest::cosine` is seeded at `NEG_INFINITY` (NOT `Default::default`
    = 0.0) so a hit with a negative cosine is recorded as the article's best
    score instead of being silently discarded. Mirrors
    `embedding::recall::recall`'s own max-pool. Without this, a true negative
    cosine surfaces as 0.5 (neutral) instead of 0.0 (opposite). */
    #[derive(Clone)]
    struct ArticleBest {
        cosine: f32,
        passage: String,
        section: Option<String>,
        /// Abstract context attached to the passage (for grounding only).
        abstract_text: Option<String>,
    }
    impl ArticleBest {
        fn new() -> Self {
            Self {
                cosine: f32::NEG_INFINITY,
                passage: String::new(),
                section: None,
                abstract_text: None,
            }
        }
    }
    let mut best_by_article_claim: HashMap<(String, String), ArticleBest> = HashMap::new();
    for work in &finalists.per_claim {
        let claim_key =
            if claim_filter.is_some() { normalize_claim_key(&work.text) } else { String::new() };
        for hit in &work.hits {
            let key = (hit.article_id.clone(), claim_key.clone());
            let entry = best_by_article_claim.entry(key).or_insert_with(ArticleBest::new);
            if hit.score > entry.cosine {
                entry.cosine = hit.score;
            }
        }
        for ev in &work.passages {
            let key = (ev.article_id.clone(), claim_key.clone());
            let entry = best_by_article_claim.entry(key).or_insert_with(ArticleBest::new);
            entry.passage = ev.passage.clone();
            entry.section = ev.section.clone();
            entry.abstract_text = ev.abstract_text.clone();
        }
    }

    /* Pre-normalize claim_filter once so the per-output grouping filter is
    also drift-tolerant. Without this, a cosmetic LLM reformat would drop
    the output by the raw `!=` filter before the normalized score lookup
    ever ran. */
    let normalized_filter = claim_filter.map(normalize_claim_key);

    let mut matches: Vec<CitationMatch> = Vec::new();
    for out in llm_outputs {
        if let Some(ref norm_filter) = normalized_filter {
            /* Result grouping uses NORMALIZED claim text on both sides so
            cosmetic drift does not drop the output. The raw claim text is
            preserved on `CitationResult.claim` upstream. */
            if normalize_claim_key(&out.claim) != *norm_filter {
                continue;
            }
        }
        let Some(classification) = parse_classification(&out.classification) else {
            continue; // LLM returned "unrelated" or garbage; drop.
        };
        let Some(meta) = metadata.get(&out.article_id) else {
            continue; // unknown article_id (LLM hallucinated); drop.
        };
        let claim_key = if normalized_filter.is_some() {
            normalize_claim_key(&out.claim)
        } else {
            String::new()
        };
        let best = best_by_article_claim
            .get(&(out.article_id.clone(), claim_key))
            .cloned()
            .unwrap_or_else(ArticleBest::new);
        /* Normalize cosine from [-1, 1] → [0, 1] for the user-facing %.
        NEG_INFINITY seed (article in metadata but absent from recall hits)
        maps to 0.0 via the `is_finite` guard. */
        let confidence = if best.cosine.is_finite() {
            (best.cosine as f64 + 1.0) / 2.0
        } else {
            0.5 // neutral fallback: no recall signal (article is a finalist but
                // its claim-score was below the Jaccard threshold, OR an LLM
                // returned an article_id the recall layer never surfaced).
        };
        /* Ground the LLM's justifying_sentences against the actual passage
        PLUS the abstract context when present (the classifier may quote the
        paper's thesis sentence from the abstract). Paraphrases/hallucinations
        are dropped before display. Empty when none grounded (UI falls back to
        full passage). */
        let grounding_source = match &best.abstract_text {
            Some(abs) if !abs.is_empty() => format!("{}\n\n{}", best.passage, abs),
            _ => best.passage.clone(),
        };
        let highlighted_sentences = ground_quotes(&out.justifying_sentences, &grounding_source);
        matches.push(CitationMatch {
            article_id: out.article_id.clone(),
            title: meta.title.clone(),
            authors: meta.authors.clone(),
            publication_year: meta.publication_year,
            journal: meta.journal.clone(),
            doi: meta.doi.clone(),
            matched_passage: best.passage,
            section_origin: best.section,
            classification,
            relevance_explanation: out.relevance_explanation.clone(),
            misrepresents_source: out.misrepresents_source,
            highlighted_sentences,
            confidence,
        });
    }
    matches.truncate(10);
    matches
}

/// Compute the Phase-C funnel counts from the pooled works + LLM outputs.
/// Pure.
///
/// `classified` is the caller's match count (whole-block: one merge over all
/// outputs; per-statement: summed across claim groups).
fn funnel_from(
    finalists: &Finalists,
    llm_outputs: &[CitationLlmOutput],
    classified: usize,
) -> CitationFunnel {
    let recalled: usize = finalists.per_claim.iter().map(|w| w.hits.len()).sum();
    let passage_survivors: usize = finalists.per_claim.iter().map(|w| w.passages.len()).sum();
    let dropped_unrelated =
        llm_outputs.iter().filter(|o| parse_classification(&o.classification).is_none()).count();
    CitationFunnel {
        recalled,
        passage_survivors,
        finalists: finalists.article_ids.len(),
        classified,
        dropped_unrelated,
    }
}

/// Emit the final Phase-C progress event carrying the funnel counts. The
/// message doubles as a one-line summary the progress UI shows ("Reviewed N
/// candidates: X matched, Y not related"), replacing the previous silent
/// drop of `unrelated` classifications.
fn emit_funnel_progress(
    emit_progress: &(dyn Fn(CitationFinderProgress) + Send + Sync),
    funnel: CitationFunnel,
) {
    emit_progress(CitationFinderProgress {
        phase: "searching".to_string(),
        stage: None,
        done: funnel.classified,
        total: funnel.finalists,
        overall_percent: 100,
        message: format!(
            "Reviewed {} candidates: {} matched, {} not related",
            funnel.finalists, funnel.classified, funnel.dropped_unrelated
        ),
        is_running: true,
        is_cancelled: false,
        funnel: Some(funnel),
    });
}

/// Phase B: 0-90% of the overall bar. Phase C uses 90-100%.
const PHASE_B_MAX_PERCENT: usize = 90;

/// Phase-C offsets within the 90-100% tail.
const PHASE_C_EMBED_QUERY_PERCENT: usize = 90;
const PHASE_C_RANKING_PERCENT: usize = 93;
const PHASE_C_CLASSIFYING_PERCENT: usize = 96;

/// Map a Phase-B (done, total) pair to the 0-90% range. `total == 0` → 0.
/// Pure `#[must_use]`.
#[must_use]
pub fn phase_b_overall_percent(done: usize, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let ratio = (done.min(total) as f64) / (total as f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pct = (ratio * PHASE_B_MAX_PERCENT as f64).round() as usize;
    pct.min(PHASE_B_MAX_PERCENT)
}

/// Map a Phase-C stage to its fixed overall-percent offset. Pure `#[must_use]`.
#[must_use]
pub fn phase_c_overall_percent(stage: &str) -> usize {
    match stage {
        "embedding_query" => PHASE_C_EMBED_QUERY_PERCENT,
        "ranking" => PHASE_C_RANKING_PERCENT,
        "classifying" => PHASE_C_CLASSIFYING_PERCENT,
        _ => PHASE_C_EMBED_QUERY_PERCENT,
    }
}

/// Helper: build a "searching" phase progress payload with the stage's
/// overall-percent offset.
fn searching_progress(stage: &str, message: &str) -> CitationFinderProgress {
    CitationFinderProgress {
        phase: "searching".to_string(),
        stage: Some(stage.to_string()),
        done: 0,
        total: 0,
        overall_percent: phase_c_overall_percent(stage),
        message: message.to_string(),
        is_running: true,
        is_cancelled: false,
        funnel: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::citation_finder::MatchClassification;

    /// Build a `ClaimWork` for testing (the struct is private but inline tests
    /// can reach it). Passages take `(article_id, passage, section, score)`;
    /// abstract context defaults to `None` (use `claim_work_with_abstract`
    /// when the grounding test needs it).
    fn claim_work(
        claim: &str,
        hits: Vec<(&str, f32)>,
        passages: Vec<(&str, &str, Option<&str>, f64)>,
    ) -> ClaimWork {
        ClaimWork {
            text: claim.to_string(),
            hits: hits
                .into_iter()
                .map(|(id, score)| EmbeddingHit {
                    article_id: id.to_string(),
                    score,
                    chunk_index: None,
                })
                .collect(),
            passages: passages
                .into_iter()
                .map(|(id, passage, section, score)| PassageEvidence {
                    article_id: id.to_string(),
                    passage: passage.to_string(),
                    section: section.map(str::to_string),
                    score,
                    abstract_text: None,
                })
                .collect(),
        }
    }

    /// Like [`claim_work`] but attaches abstract context to every passage
    /// entry (for grounding tests).
    fn claim_work_with_abstract(
        claim: &str,
        hits: Vec<(&str, f32)>,
        passages: Vec<(&str, &str, Option<&str>, f64, &str)>,
    ) -> ClaimWork {
        ClaimWork {
            text: claim.to_string(),
            hits: hits
                .into_iter()
                .map(|(id, score)| EmbeddingHit {
                    article_id: id.to_string(),
                    score,
                    chunk_index: None,
                })
                .collect(),
            passages: passages
                .into_iter()
                .map(|(id, passage, section, score, abs)| PassageEvidence {
                    article_id: id.to_string(),
                    passage: passage.to_string(),
                    section: section.map(str::to_string),
                    score,
                    abstract_text: Some(abs.to_string()),
                })
                .collect(),
        }
    }

    fn meta_map(ids: &[&str]) -> HashMap<String, CandidateMetadata> {
        ids.iter()
            .map(|id| {
                (
                    id.to_string(),
                    CandidateMetadata {
                        article_id: id.to_string(),
                        title: format!("Title {id}"),
                        authors: vec!["Author".to_string()],
                        publication_year: Some(2024),
                        journal: Some("Journal".to_string()),
                        doi: Some(format!("10.1000/{id}")),
                    },
                )
            })
            .collect()
    }

    fn llm_out(
        article_id: &str,
        claim: &str,
        classification: &str,
        misrepresents: bool,
    ) -> CitationLlmOutput {
        CitationLlmOutput {
            article_id: article_id.to_string(),
            claim: claim.to_string(),
            classification: classification.to_string(),
            relevance_explanation: "explanation".to_string(),
            misrepresents_source: misrepresents,
            justifying_sentences: Vec::new(),
        }
    }

    // ── normalize_claim_key ──────────────────────────────────────────────

    #[test]
    fn normalize_claim_key_trims_and_lowercases() {
        assert_eq!(normalize_claim_key("  Sugar Taxes  "), "sugar taxes");
    }

    #[test]
    fn normalize_claim_key_collapses_internal_whitespace() {
        assert_eq!(
            normalize_claim_key("Sugar   taxes\treduce\nobesity"),
            "sugar taxes reduce obesity"
        );
    }

    #[test]
    fn normalize_claim_key_empty_stays_empty() {
        assert_eq!(normalize_claim_key(""), "");
        assert_eq!(normalize_claim_key("   "), "");
    }

    #[test]
    fn normalize_claim_key_preserves_punctuation() {
        // Punctuation is NOT stripped (only whitespace + case). The drift we
        // guard against is whitespace/case, not trailing-period differences
        // (those still won't match, but that's a rarer drift than whitespace
        // collapse).
        assert_eq!(normalize_claim_key("Sugar taxes."), "sugar taxes.");
    }

    // ── merge_outputs: whole-block ───────────────────────────────────────

    #[test]
    fn merge_whole_block_uses_empty_claim_key() {
        // Whole-block: claim_filter = None → empty claim key. The cosine from
        // the recall hit should flow through to confidence.
        let work =
            claim_work("ignored", vec![("a1", 0.8)], vec![("a1", "passage", Some("Results"), 0.5)]);
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs = vec![llm_out("a1", "", "validating", false)];

        let matches = merge_outputs(&outputs, &finalists, &metadata, None);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].article_id, "a1");
        assert_eq!(matches[0].classification, MatchClassification::Validating);
        // cosine 0.8 → (0.8 + 1) / 2 = 0.9
        assert!((matches[0].confidence - 0.9).abs() < 1e-5, "got {}", matches[0].confidence);
        assert_eq!(matches[0].section_origin.as_deref(), Some("Results"));
        assert!(!matches[0].misrepresents_source);
    }

    // ── merge_outputs: per-statement claim-key drift ─────────────────────

    #[test]
    fn merge_per_statement_handles_claim_whitespace_drift() {
        // The splitter produced "Sugar taxes reduce obesity." but the LLM
        // echoed "Sugar   taxes reduce obesity." (extra spaces). Without
        // normalize_claim_key the cosine lookup would miss and confidence
        // would silently fall to 0.5. With normalization the real cosine
        // (0.6 → 0.8 confidence) flows through.
        let splitter_claim = "Sugar taxes reduce obesity.";
        let llm_echoed_claim = "Sugar   taxes reduce obesity.";
        let work =
            claim_work(splitter_claim, vec![("a1", 0.6)], vec![("a1", "passage", None, 0.3)]);
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs = vec![llm_out("a1", llm_echoed_claim, "validating", false)];

        let matches = merge_outputs(&outputs, &finalists, &metadata, Some(splitter_claim));
        assert_eq!(matches.len(), 1, "claim drift must not drop the match");
        // cosine 0.6 → (0.6 + 1) / 2 = 0.8 (NOT the 0.5 fallback).
        assert!(
            (matches[0].confidence - 0.8).abs() < 1e-5,
            "normalized key should recover the real cosine; got {}",
            matches[0].confidence
        );
    }

    #[test]
    fn merge_per_statement_handles_claim_case_drift() {
        let splitter_claim = "Sugar taxes reduce obesity.";
        let llm_echoed_claim = "SUGAR TAXES REDUCE OBESITY.";
        let work = claim_work(splitter_claim, vec![("a1", 0.4)], vec![("a1", "p", None, 0.2)]);
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs = vec![llm_out("a1", llm_echoed_claim, "opposing", true)];

        let matches = merge_outputs(&outputs, &finalists, &metadata, Some(splitter_claim));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].classification, MatchClassification::Opposing);
        assert!(matches[0].misrepresents_source);
        // cosine 0.4 → (0.4 + 1) / 2 = 0.7
        assert!((matches[0].confidence - 0.7).abs() < 1e-5);
    }

    #[test]
    fn merge_per_statement_filters_to_claim_filter() {
        // Only outputs whose raw claim matches claim_filter are included.
        let claim_a = "Claim A.";
        let claim_b = "Claim B.";
        let work_a = claim_work(claim_a, vec![("a1", 0.5)], vec![("a1", "p", None, 0.1)]);
        let work_b = claim_work(claim_b, vec![("a2", 0.5)], vec![("a2", "p", None, 0.1)]);
        let finalists = Finalists {
            article_ids: vec!["a1".to_string(), "a2".to_string()],
            per_claim: vec![work_a, work_b],
        };
        let metadata = meta_map(&["a1", "a2"]);
        let outputs = vec![
            llm_out("a1", claim_a, "validating", false),
            llm_out("a2", claim_b, "validating", false),
        ];

        let only_a = merge_outputs(&outputs, &finalists, &metadata, Some(claim_a));
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].article_id, "a1");
    }

    // ── merge_outputs: drop paths ────────────────────────────────────────

    #[test]
    fn merge_drops_hallucinated_article_id() {
        // LLM invented an article_id not in metadata → dropped.
        let work = claim_work("text", vec![("a1", 0.5)], vec![("a1", "p", None, 0.1)]);
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs = vec![
            llm_out("a1", "", "validating", false),
            llm_out("ghost", "", "validating", false), // not in metadata
        ];
        let matches = merge_outputs(&outputs, &finalists, &metadata, None);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].article_id, "a1");
    }

    #[test]
    fn merge_drops_unrelated_and_garbage_classifications() {
        let work = claim_work("text", vec![("a1", 0.5)], vec![("a1", "p", None, 0.1)]);
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs = vec![
            llm_out("a1", "", "validating", false),
            llm_out("a1", "", "unrelated", false), // filtered by prompt, dropped here
            llm_out("a1", "", "maybe", false),     // garbage
        ];
        let matches = merge_outputs(&outputs, &finalists, &metadata, None);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn merge_truncates_to_ten() {
        // 12 outputs for the same article → truncated to 10.
        let work = claim_work("text", vec![("a1", 0.5)], vec![("a1", "p", None, 0.1)]);
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs: Vec<CitationLlmOutput> =
            (0..12).map(|_| llm_out("a1", "", "validating", false)).collect();
        let matches = merge_outputs(&outputs, &finalists, &metadata, None);
        assert_eq!(matches.len(), 10);
    }

    // ── merge_outputs: cosine normalization edge cases ───────────────────

    #[test]
    fn merge_confidence_negative_cosine_normalizes_correctly() {
        // cosine -1.0 (opposite direction) → (-1 + 1) / 2 = 0.0
        let work = claim_work("text", vec![("a1", -1.0)], vec![("a1", "p", None, 0.1)]);
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs = vec![llm_out("a1", "", "validating", false)];
        let matches = merge_outputs(&outputs, &finalists, &metadata, None);
        assert!((matches[0].confidence - 0.0).abs() < 1e-5, "got {}", matches[0].confidence);
    }

    #[test]
    fn merge_confidence_missing_cosine_falls_to_neutral() {
        // Article in metadata but NOT in recall hits (cosine unset / 0.0
        // default) → (0 + 1) / 2 = 0.5 neutral.
        let work = claim_work("text", vec![], vec![]); // no hits, no passages
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs = vec![llm_out("a1", "", "validating", false)];
        let matches = merge_outputs(&outputs, &finalists, &metadata, None);
        assert!((matches[0].confidence - 0.5).abs() < 1e-5, "got {}", matches[0].confidence);
    }

    // ── pool_finalists ───────────────────────────────────────────────────

    #[test]
    fn pool_finalists_dedups_article_ids_keeping_best_score() {
        // Same article appears in two claims with different scores; the union
        // keeps it once.
        let w1 = claim_work("c1", vec![("a1", 0.5)], vec![("a1", "p1", None, 0.3)]);
        let w2 = claim_work("c2", vec![("a1", 0.7)], vec![("a1", "p2", None, 0.6)]);
        let finalists = pool_finalists(vec![w1, w2]);
        assert_eq!(finalists.article_ids.len(), 1);
        assert_eq!(finalists.article_ids[0], "a1");
        // Both per-claim works are preserved (for per-statement grouping).
        assert_eq!(finalists.per_claim.len(), 2);
    }

    #[test]
    fn pool_finalists_truncates_to_fifteen() {
        // 20 distinct articles, containment and cosine perfectly correlated →
        // the cosine union adds nothing (all 5 cosine-best are already in the
        // containment top-15) → still exactly 15.
        let works: Vec<ClaimWork> = (0..20)
            .map(|i| {
                claim_work(
                    "c",
                    vec![(format!("a{i}").leak(), 0.1 * i as f32)],
                    vec![(format!("a{i}").leak(), "p", None, 0.1 * i as f64)],
                )
            })
            .collect();
        let finalists = pool_finalists(works);
        assert_eq!(finalists.article_ids.len(), 15);
    }

    #[test]
    fn pool_finalists_cosine_union_rescues_low_containment_article() {
        // 15 articles with high containment (0.9..0.76) fill the containment
        // slots; article "rescue" has weak containment (0.32, paraphrased
        // claim) but the top cosine (0.99). Containment-only truncation (the
        // pre-union behavior) would evict it; the union must keep it.
        let mut works: Vec<ClaimWork> = (0..15)
            .map(|i| {
                claim_work(
                    "c",
                    vec![(format!("f{i}").leak(), 0.2)],
                    vec![(format!("f{i}").leak(), "p", None, 0.9 - 0.01 * i as f64)],
                )
            })
            .collect();
        works.push(claim_work("c", vec![("rescue", 0.99)], vec![("rescue", "p", None, 0.32)]));
        let finalists = pool_finalists(works);
        assert!(
            finalists.article_ids.contains(&"rescue".to_string()),
            "cosine union must rescue the paraphrased match"
        );
        assert_eq!(finalists.article_ids.len(), 16);
    }

    #[test]
    fn pool_finalists_caps_union_at_twenty() {
        // 18 containment survivors + 5 high-cosine non-survivors → capped at 20.
        let mut works: Vec<ClaimWork> = (0..18)
            .map(|i| {
                claim_work(
                    "c",
                    vec![(format!("f{i}").leak(), 0.05)],
                    vec![(format!("f{i}").leak(), "p", None, 0.5)],
                )
            })
            .collect();
        for i in 0..5 {
            // No passage (gate-dropped): only present in the cosine ranking.
            works.push(claim_work("c", vec![(format!("x{i}").leak(), 0.9)], vec![]));
        }
        let finalists = pool_finalists(works);
        assert_eq!(finalists.article_ids.len(), 20);
    }

    #[test]
    fn pool_finalists_filters_passages_to_finalist_set() {
        // Works carrying 18 passage entries (only 15 can be finalists) must
        // have their per-claim passages filtered to the finalist set, so the
        // classification prompt never carries un-promptable candidates.
        // Cosine scores correlate with containment so the union adds nothing.
        let works: Vec<ClaimWork> = (0..18)
            .map(|i| {
                claim_work(
                    "c",
                    vec![(format!("a{i}").leak(), 0.1 * i as f32)],
                    vec![(format!("a{i}").leak(), "p", None, 0.1 * i as f64)],
                )
            })
            .collect();
        let finalists = pool_finalists(works);
        assert_eq!(finalists.article_ids.len(), 15);
        let survivors: usize = finalists.per_claim.iter().map(|w| w.passages.len()).sum();
        assert_eq!(survivors, 15, "per-claim passages must be filtered to finalists");
    }

    #[test]
    fn pool_finalists_empty_works_yields_empty() {
        let finalists = pool_finalists(vec![]);
        assert!(finalists.article_ids.is_empty());
    }

    // ── cosine_best_chunk ────────────────────────────────────────────────

    #[test]
    fn cosine_best_chunk_resolves_valid_index() {
        let chunks = vec![
            Chunk { section: None, chunk_index: 0, text: "intro".to_string(), word_count: 1 },
            Chunk {
                section: Some("Methods".to_string()),
                chunk_index: 1,
                text: "methods".to_string(),
                word_count: 1,
            },
        ];
        let hit = EmbeddingHit { article_id: "a1".to_string(), score: 0.5, chunk_index: Some(1) };
        assert_eq!(cosine_best_chunk(&hit, &chunks).map(|c| c.text.as_str()), Some("methods"));
    }

    #[test]
    fn cosine_best_chunk_title_abstract_row_is_none() {
        // chunk_index = -1 is the title+abstract embedding row, not a chunk.
        let chunks =
            vec![Chunk { section: None, chunk_index: 0, text: "intro".to_string(), word_count: 1 }];
        let hit = EmbeddingHit { article_id: "a1".to_string(), score: 0.5, chunk_index: Some(-1) };
        assert!(cosine_best_chunk(&hit, &chunks).is_none());
    }

    #[test]
    fn cosine_best_chunk_out_of_range_is_none() {
        // Stale chunk provenance after a re-chunk: index beyond the list.
        let chunks =
            vec![Chunk { section: None, chunk_index: 0, text: "intro".to_string(), word_count: 1 }];
        let hit = EmbeddingHit { article_id: "a1".to_string(), score: 0.5, chunk_index: Some(7) };
        assert!(cosine_best_chunk(&hit, &chunks).is_none());
    }

    #[test]
    fn cosine_best_chunk_missing_provenance_is_none() {
        let chunks =
            vec![Chunk { section: None, chunk_index: 0, text: "intro".to_string(), word_count: 1 }];
        let hit = EmbeddingHit { article_id: "a1".to_string(), score: 0.5, chunk_index: None };
        assert!(cosine_best_chunk(&hit, &chunks).is_none());
    }

    // ── merge_outputs: abstract-context grounding ─────────────────────────

    #[test]
    fn merge_grounds_against_abstract_context() {
        // The classifier quoted the paper's thesis sentence from the abstract
        // context (not present in the chunk passage). The grounding gate must
        // accept it (passage + abstract is the source), otherwise abstract
        // evidence could never surface as a highlighted sentence.
        let work = claim_work_with_abstract(
            "text",
            vec![("a1", 0.6)],
            vec![(
                "a1",
                "Methods paragraph about regression models.",
                None,
                0.5,
                "Title\n\nThe sugar tax reduced purchases of sugary drinks.",
            )],
        );
        let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
        let metadata = meta_map(&["a1"]);
        let outputs = vec![CitationLlmOutput {
            article_id: "a1".to_string(),
            claim: String::new(),
            classification: "validating".to_string(),
            relevance_explanation: "expl".to_string(),
            misrepresents_source: false,
            justifying_sentences: vec![
                "The sugar tax reduced purchases of sugary drinks.".to_string()
            ],
        }];
        let matches = merge_outputs(&outputs, &finalists, &metadata, None);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].highlighted_sentences.len(),
            1,
            "abstract-context quote must survive grounding"
        );
    }
}
