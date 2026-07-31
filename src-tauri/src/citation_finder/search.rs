//! The Citation Finder search pipeline (`citation_finder/AGENTS.md`).
//!
//! `find_citations_inner` is the spawn-safe core (no `tauri::State`). The
//! `commands::citation_finder::find_citations` Tauri command wraps it.
//!
//! Three-phase one-button flow (`citation_finder/AGENTS.md`):
//! - **Phase A:** readiness check (brief lock). Decide whether Phase B runs.
//! - **Phase B:** (conditional) auto-prepare embeddings by reusing
//!   `generate_embeddings_inner` with the same cancel token.
//! - **Phase C:** the search pipeline - claim-split (per-statement only) →
//!   recall (reuse) → containment passage → LLM classify → merge into
//!   `CitationResult[]`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
use super::similarity::{find_best_passage, tokenize_and_stem};
use crate::citation_finder::{
    filter_valid_statuses, CitationFinderMode, CitationFinderProgress, CitationMatch,
    CitationResult,
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

/// Injectable sender so the search pipeline's LLM call is unit-testable
/// without a live provider. Mirrors `EmbeddingBatchSender`.
#[async_trait::async_trait]
pub trait CitationLlmSender: Send + Sync {
    /// Run the main classification call. Returns the prepared JSON string
    /// (already passed through `prepare_llm_json`).
    async fn send_classification(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AppError>;

    /// Run the claim-split call (per-statement mode only). Returns the
    /// prepared JSON string.
    async fn send_claim_split(&self, text: &str) -> Result<String, AppError>;

    /// Run a single embedding recall for one query text. Returns the top-K
    /// article hits with their cosine scores.
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
        recall::recall(&db_state, &self.orchestrator, query, top_k, statuses).await
    }
}

/// One input claim (text + cosine recall hits + per-candidate best passage).
struct ClaimWork {
    text: String,
    hits: Vec<EmbeddingHit>,
    /// (article_id, passage, section, containment_score) per hit that had a
    /// usable passage. Articles whose best passage scored below
    /// `MIN_PASSAGE_SCORE` are absent.
    passages: Vec<(String, String, Option<String>, f64)>,
}

/// The pooled finalists across one or more claims: the union of article IDs
/// + the per-claim passage lists (used to build the prompt + merge LLM output).
struct Finalists {
    article_ids: Vec<String>,
    per_claim: Vec<ClaimWork>,
}

/// Bundles the user input + runtime params so `find_citations_inner` stays
/// under the clippy `too_many_arguments` threshold (8/7). Mirrors the
/// `RunSyncContext` pattern used by the screening engine.
pub struct FindCitationsContext<'a> {
    pub text: String,
    pub mode: CitationFinderMode,
    pub status_filter: Vec<String>,
    pub cancel_token: Arc<AtomicBool>,
    pub emit_progress: &'a (dyn Fn(CitationFinderProgress) + Send + Sync),
    /// `Some(app_handle)` in production so Phase B can forward the embedding
    /// runner's per-article `embedding:progress` events. The frontend listens
    /// for `embedding:progress` during Phase B + re-emits as
    /// `citation:progress`. `None` in tests (no events).
    pub app_handle: Option<tauri::AppHandle>,
}

/// The core spawn-safe search pipeline.
///
/// Returns `Vec<CitationResult>` - one entry per claim (per-statement) or a
/// single entry with `claim: None` (whole-block).
///
/// `ctx.emit_progress` is called with phase-appropriate payloads; the caller
/// owns the event-emission plumbing.
pub async fn find_citations_inner(
    db_state: &State<'_, DbState>,
    embedding_sender: Arc<dyn EmbeddingBatchSender>,
    llm_sender: Arc<dyn CitationLlmSender>,
    ctx: FindCitationsContext<'_>,
) -> Result<Vec<CitationResult>, AppError> {
    let FindCitationsContext { text, mode, status_filter, cancel_token, emit_progress, app_handle } =
        ctx;
    // Apply the status whitelist at the command boundary. The backend does NOT
    // assume a default - if the caller supplies no valid statuses, the search
    // returns the "No articles match the selected filters." empty result
    // rather than silently searching all articles. `duplicate` is always
    // dropped (never a citation candidate); typos/injection are filtered too.
    let status_filter = filter_valid_statuses(&status_filter);
    // ═══════════════════════════════════════════════════════════════════
    //  Phase A: readiness check (brief lock)
    // ═══════════════════════════════════════════════════════════════════
    let readiness = {
        let conn = lock_conn(&db_state.conn)?;
        compute_readiness(&conn, &status_filter)?
    };
    if !readiness.provider_supports_embeddings {
        return Err(AppError::Import(
            "Provider does not support embeddings. Configure an embedding-capable LLM provider."
                .to_string(),
        ));
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

        // NOTE: deliberately NO post-prepare 100%-coverage re-check. The
        // previous gate hard-failed whenever coverage stayed below 100%, but
        // coverage can legitimately plateau below 100% when some articles have
        // no embeddable content: `expected_rows` (embedding/text.rs) returns
        // zero rows for an article with an empty title + empty abstract + no
        // full-text chunks, so the director never produces an `EmbedTask` for
        // them and they permanently sit outside the numerator. That left the
        // search dead-ended ("Embedding preparation incomplete (87% coverage,
        // 13/15 articles). Retry.") even though the runner had done its job
        // correctly. The recall layer naturally handles partial coverage -
        // articles with no embedding rows are simply absent from the candidate
        // pool, which is the correct outcome (they have no semantic signal).
        // The standalone `generate_embeddings` command (Settings) has the same
        // no-gate behavior. Real errors (DB lock failures, provider outage)
        // still propagate via the `?` on `generate_embeddings_inner` above.
    }

    // ═══════════════════════════════════════════════════════════════════
    //  Phase C: search pipeline
    // ═══════════════════════════════════════════════════════════════════
    match mode {
        CitationFinderMode::WholeBlock => {
            run_whole_block(
                &text,
                &status_filter,
                &llm_sender,
                db_state,
                &cancel_token,
                emit_progress,
            )
            .await
        }
        CitationFinderMode::PerStatement => {
            run_per_statement(
                &text,
                &status_filter,
                &llm_sender,
                db_state,
                &cancel_token,
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
    db_state: &State<'_, DbState>,
    cancel_token: &Arc<AtomicBool>,
    emit_progress: &(dyn Fn(CitationFinderProgress) + Send + Sync),
) -> Result<Vec<CitationResult>, AppError> {
    emit_progress(searching_progress("embedding_query", "Embedding query…"));

    let hits = llm_sender.recall(text, 30, status_filter).await?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err(AppError::Import("Cancelled".to_string()));
    }
    if hits.is_empty() {
        return Ok(vec![CitationResult { claim: None, matches: vec![] }]);
    }

    emit_progress(searching_progress("ranking", "Ranking passages…"));
    let user_tokens = tokenize_and_stem(text);
    let work = build_claim_work(&user_tokens, text, hits, db_state).await?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err(AppError::Import("Cancelled".to_string()));
    }

    emit_progress(searching_progress("classifying", "Classifying…"));
    let finalists = pool_finalists(vec![work]);
    let metadata = load_metadata(db_state, &finalists.article_ids).await?;
    // Build one CandidatePassage per finalist (best passage per article).
    // Inlined here (mirrors the per-statement path) so the two modes share
    // one passage-building pattern.
    let passages: Vec<CandidatePassage> = finalists
        .per_claim
        .iter()
        .flat_map(|w| {
            w.passages.iter().map(|(aid, passage, section, _score)| CandidatePassage {
                article_id: aid.clone(),
                claim: None,
                passage: passage.clone(),
                section: section.clone(),
            })
        })
        .collect();
    let user_prompt = build_whole_block_prompt(text, &passages, &metadata);
    // Cancel check before the (up to 120s) classification call. Confines the
    // wait window to the actual HTTP round-trip: a Cancel during
    // `load_metadata`'s yields proceeds no further.
    if cancel_token.load(Ordering::Relaxed) {
        return Err(AppError::Import("Cancelled".to_string()));
    }
    let json = llm_sender.send_classification(CITATION_FINDER_SYSTEM_PROMPT, &user_prompt).await?;
    // Lenient parse: accepts snake_case (prompt contract) + camelCase (LLM
    // drift) field names, object-wrapped arrays, and isolates per-element
    // faults so one bad entry doesn't drop the whole batch.
    let llm_outputs = parse_citation_outputs(&json)
        .map_err(|e| AppError::Import(format!("Citation Finder LLM returned invalid JSON: {e}")))?;

    let matches = merge_outputs(&llm_outputs, &finalists, &metadata, None);
    Ok(vec![CitationResult { claim: None, matches }])
}

/// Per-statement pipeline: claim-split → per-claim recall + passage → pool →
/// one LLM call with per-(article, claim) entries → group by claim.
async fn run_per_statement(
    text: &str,
    status_filter: &[String],
    llm_sender: &Arc<dyn CitationLlmSender>,
    db_state: &State<'_, DbState>,
    cancel_token: &Arc<AtomicBool>,
    emit_progress: &(dyn Fn(CitationFinderProgress) + Send + Sync),
) -> Result<Vec<CitationResult>, AppError> {
    emit_progress(searching_progress("embedding_query", "Splitting claims…"));
    let split_json = llm_sender.send_claim_split(text).await?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err(AppError::Import("Cancelled".to_string()));
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

    emit_progress(searching_progress("ranking", "Ranking passages per claim…"));
    let mut works: Vec<ClaimWork> = Vec::with_capacity(claims.len());
    for claim in &claims {
        if cancel_token.load(Ordering::Relaxed) {
            return Err(AppError::Import("Cancelled".to_string()));
        }
        let hits = llm_sender.recall(claim, 30, status_filter).await?;
        let user_tokens = tokenize_and_stem(claim);
        let work = build_claim_work(&user_tokens, claim, hits, db_state).await?;
        works.push(work);
    }

    emit_progress(searching_progress("classifying", "Classifying…"));
    let finalists = pool_finalists(works);
    let metadata = load_metadata(db_state, &finalists.article_ids).await?;

    // Build one CandidatePassage per (article, claim). An article that matched
    // multiple claims gets multiple entries.
    let mut passages: Vec<CandidatePassage> = Vec::new();
    for per_claim in &finalists.per_claim {
        for (article_id, passage, section, _score) in &per_claim.passages {
            passages.push(CandidatePassage {
                article_id: article_id.clone(),
                claim: Some(per_claim.text.clone()),
                passage: passage.clone(),
                section: section.clone(),
            });
        }
    }
    let user_prompt = build_per_statement_prompt(&claims, &passages, &metadata);
    // Cancel check before the (up to 120s) classification call (mirrors the
    // whole-block guard). Confines the wait window to the actual HTTP
    // round-trip.
    if cancel_token.load(Ordering::Relaxed) {
        return Err(AppError::Import("Cancelled".to_string()));
    }
    let json = llm_sender.send_classification(CITATION_FINDER_SYSTEM_PROMPT, &user_prompt).await?;
    // Lenient parse: accepts snake_case (prompt contract) + camelCase (LLM
    // drift) field names, object-wrapped arrays, and isolates per-element
    // faults so one bad entry doesn't drop the whole batch.
    let llm_outputs = parse_citation_outputs(&json)
        .map_err(|e| AppError::Import(format!("Citation Finder LLM returned invalid JSON: {e}")))?;

    // Group LLM outputs by claim.
    let mut results: Vec<CitationResult> = Vec::with_capacity(claims.len());
    for claim in &claims {
        let matches = merge_outputs(&llm_outputs, &finalists, &metadata, Some(claim));
        results.push(CitationResult { claim: Some(claim.clone()), matches });
    }
    Ok(results)
}

/// Normalize a claim string for use as a lookup key in `merge_outputs`.
///
/// Trim + collapse internal whitespace runs + lowercase. This makes the
/// `(article_id, claim)` score lookup robust to the LLM lightly reformatting
/// the claim text (trailing punctuation drift, collapsed whitespace, case
/// changes) when it echoes the claim in its JSON output. Without this, a
/// cosmetic claim drift between the splitter and the classifier causes the
/// cosine-score lookup to miss and `confidence` silently falls back to 0.5
/// (the `(0+1)/2` midpoint of an unset cosine).
///
/// Pure `#[must_use]`. Public so the pipeline tests can pin the contract.
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

/// Build one `ClaimWork` from a claim's recall hits: load chunks per article,
/// run `find_best_passage`, drop articles whose passage scored below
/// `MIN_PASSAGE_SCORE`.
///
/// **Lock discipline**: each candidate's chunk + metadata read takes a brief
/// `lock_conn` burst, releasing between articles. This avoids holding the
/// `DbState` mutex across up to 30 chunk reads (150 in per-statement mode with
/// 5 claims), which would freeze every other DB-touching IPC command for the
/// whole pass - the same mutex-starvation anti-pattern the root `AGENTS.md`
/// flags for screening. The per-article cost is one short lock acquire +
/// release; `tokio::task::yield_now()` between articles lets the runtime
/// flush progress events + give queued commands a turn.
async fn build_claim_work(
    user_tokens: &[String],
    claim_text: &str,
    hits: Vec<EmbeddingHit>,
    db_state: &State<'_, DbState>,
) -> Result<ClaimWork, AppError> {
    let mut passages: Vec<(String, String, Option<String>, f64)> = Vec::new();
    for hit in &hits {
        // Brief lock burst per article: read chunks (and synthesize the
        // abstract fallback if needed), then release before the next article.
        let best = {
            let conn = lock_conn(&db_state.conn)?;
            let chunks = chunk_repo::list_chunks_for_article(&conn, &hit.article_id)?;
            if chunks.is_empty() {
                // Abstract-only article: synthesize a chunk from the abstract
                // with section: Some("Abstract") (`citation_finder/AGENTS.md`).
                let article = article_repo::get_article_by_id(&conn, &hit.article_id)?;
                let text = if article.abstract_text.is_empty() {
                    article.title.clone()
                } else {
                    format!("{}\n\n{}", article.title, article.abstract_text)
                };
                let tokens = tokenize_and_stem(&text);
                // Containment (query coverage), NOT Jaccard: the abstract is
                // typically much longer than the query, so Jaccard would be
                // diluted and drop exact-quote matches. Containment is
                // length-insensitive on the document side. See `similarity.rs`.
                let score = super::similarity::containment(user_tokens, &tokens);
                if score < super::similarity::MIN_PASSAGE_SCORE {
                    None
                } else {
                    Some((text, Some("Abstract".to_string()), score))
                }
            } else {
                find_best_passage(user_tokens, &chunks)
            }
        };
        if let Some((passage, section, score)) = best {
            passages.push((hit.article_id.clone(), passage, section, score));
        }
        // Yield between articles so the runtime can flush `citation:progress`
        // events and queued IPC commands get a turn at the mutex.
        tokio::task::yield_now().await;
    }
    Ok(ClaimWork { text: claim_text.to_string(), hits, passages })
}

/// Union the article IDs across works + keep the per-claim passages. Top-N
/// (15) by the best containment score across the pool.
fn pool_finalists(works: Vec<ClaimWork>) -> Finalists {
    let mut best_score: HashMap<String, f64> = HashMap::new();
    for work in &works {
        for (article_id, _passage, _section, score) in &work.passages {
            let entry = best_score.entry(article_id.clone()).or_insert(-1.0);
            if *score > *entry {
                *entry = *score;
            }
        }
    }
    let mut scored: Vec<(String, f64)> = best_score.into_iter().collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(15);
    let article_ids: Vec<String> = scored.into_iter().map(|(id, _)| id).collect();
    Finalists { article_ids, per_claim: works }
}

/// Load metadata (title, authors, year, journal, doi) for the finalist IDs.
///
/// **Lock discipline**: each article read takes a brief `lock_conn` burst,
/// releasing between articles. This mirrors `build_claim_work` so the `DbState`
/// mutex is never held across the full ≤15-read loop.
async fn load_metadata(
    db_state: &State<'_, DbState>,
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
    // Build a lookup: (article_id, normalized_claim_key) → cosine score (from
    // recall) + best passage. The claim key is NORMALIZED so cosmetic drift
    // between the splitter's claim text and the classifier's echoed claim
    // (punctuation, whitespace, case) does not cause a score-lookup miss.
    //
    // `ArticleBest::cosine` is seeded at `NEG_INFINITY` (NOT `Default::default`
    // = 0.0) so a hit with a negative cosine is recorded as the article's best
    // score instead of being silently discarded by the `> 0.0` guard. This
    // mirrors `embedding::recall::recall`'s own max-pool, which seeds with
    // `NEG_INFINITY` for the same reason. Without this, a true negative
    // cosine would be recorded as 0.0 and the user-facing confidence would be
    // 0.5 (neutral) instead of 0.0 (opposite direction).
    #[derive(Clone)]
    struct ArticleBest {
        cosine: f32,
        passage: String,
        section: Option<String>,
    }
    impl ArticleBest {
        fn new() -> Self {
            Self { cosine: f32::NEG_INFINITY, passage: String::new(), section: None }
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
        for (article_id, passage, section, _score) in &work.passages {
            let key = (article_id.clone(), claim_key.clone());
            let entry = best_by_article_claim.entry(key).or_insert_with(ArticleBest::new);
            entry.passage = passage.clone();
            entry.section = section.clone();
        }
    }

    // Pre-normalize the claim_filter once so the per-output grouping filter
    // is also drift-tolerant. Without this, an LLM that lightly reformats the
    // claim text would have its output dropped by the raw `!=` filter before
    // the normalized score lookup ever ran - the exact drift the normalized
    // key is supposed to tolerate.
    let normalized_filter = claim_filter.map(normalize_claim_key);

    let mut matches: Vec<CitationMatch> = Vec::new();
    for out in llm_outputs {
        if let Some(ref norm_filter) = normalized_filter {
            // Result grouping uses the NORMALIZED claim text on both sides so
            // cosmetic drift (whitespace, case) does not drop the output. The
            // raw claim text is preserved on the `CitationResult.claim` field
            // upstream (so the user's claim headings stay readable).
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
        // Normalize cosine from [-1, 1] → [0, 1] for the user-facing %. A
        // NEG_INFINITY seed (article in metadata but absent from recall hits)
        // maps to 0.0 via the `is_finite` guard.
        let confidence = if best.cosine.is_finite() {
            (best.cosine as f64 + 1.0) / 2.0
        } else {
            0.5 // neutral fallback: no recall signal (article is a finalist but
                // its claim-score was below the Jaccard threshold, OR an LLM
                // returned an article_id the recall layer never surfaced).
        };
        // Ground the LLM's justifying sentences against the actual passage so
        // paraphrases/hallucinations are dropped before display. Empty when
        // the LLM omitted the field or none grounded (UI falls back to the
        // full passage).
        let highlighted_sentences = ground_quotes(&out.justifying_sentences, &best.passage);
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

/// Phase B occupies the 0-90% range of the overall progress bar (Phase C uses
/// 90-100). Used by both the initial Phase B snapshot + the frontend's
/// `embedding:progress` listener (which translates each `{processed, total}`
/// into the same range).
const PHASE_B_MAX_PERCENT: usize = 90;

/// Phase-C fixed offsets within the 90-100% tail. `embedding_query` starts
/// where Phase B left off; `ranking` bumps 3%; `classifying` bumps another 3%.
const PHASE_C_EMBED_QUERY_PERCENT: usize = 90;
const PHASE_C_RANKING_PERCENT: usize = 93;
const PHASE_C_CLASSIFYING_PERCENT: usize = 96;

/// Map a Phase-B (done, total) pair to the overall 0-90% range.
///
/// Pure `#[must_use]`. `total == 0` → 0 (avoids division by zero; the caller
/// has already gated on `total_articles > 0` in Phase A, but this is
/// defense-in-depth for the frontend listener which may receive a stale
/// payload).
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

/// Map a Phase-C stage name to its fixed overall-percent offset.
///
/// Pure `#[must_use]`. The Phase-C stages have no item-counted operations
/// (each is a single LLM call or a pure in-memory pass), so fixed offsets
/// within the 90-100% tail communicate progress without a denominator.
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::citation_finder::MatchClassification;

    /// Build a `ClaimWork` for testing (the struct is private but inline tests
    /// can reach it).
    fn claim_work(
        claim: &str,
        hits: Vec<(&str, f32)>,
        passages: Vec<(&str, &str, Option<&str>, f64)>,
    ) -> ClaimWork {
        ClaimWork {
            text: claim.to_string(),
            hits: hits
                .into_iter()
                .map(|(id, score)| EmbeddingHit { article_id: id.to_string(), score })
                .collect(),
            passages: passages
                .into_iter()
                .map(|(id, passage, section, score)| {
                    (id.to_string(), passage.to_string(), section.map(str::to_string), score)
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
        // 20 distinct articles → top 15 by best score.
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
    fn pool_finalists_empty_works_yields_empty() {
        let finalists = pool_finalists(vec![]);
        assert!(finalists.article_ids.is_empty());
    }
}
