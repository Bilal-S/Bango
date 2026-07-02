use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::sleep;

use tauri::Emitter;

/// Debug-only logging macro. Compiles to a no-op in release builds.
/// Prevents LLM response content (which may contain PII from article abstracts)
/// from leaking to stderr in production.
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        { eprintln!($($arg)*); }
    };
}

use crate::db::{
    app_settings_repo::ScreeningMode, article_repo, audit_repo, biblio_repo, chunk_repo,
    label_repo, tag_repo,
};
use crate::error::AppError;
use crate::llm::orchestrator::LlmRequestType;
use crate::models::biblio::{TermSource, TermType};
use crate::models::criterion::{Criterion, CriterionType, ResearchAim};
use crate::screening::chunk_retrieval::{
    rank_chunks_by_criteria, ScoredChunk, DEFAULT_MAX_CHUNK_WORDS,
};
use crate::screening::llm_client::LlmClient;
use crate::screening::prompt::{
    self, AimEntry, ArticleEntry, CriterionEntry, ScreeningPromptInput,
};
use crate::screening::resolution::{self, CriterionMatch};

/// Tier 3 screening configuration. Built by the command layer from `app_settings`
/// and passed into `run_sync`. Pure value type — no I/O.
#[derive(Debug, Clone)]
pub struct ScreeningConfig {
    pub mode: ScreeningMode,
    pub enhanced_top_k: usize,
    pub enhanced_sections: Vec<String>,
    pub two_stage_low: f64,
    pub two_stage_high: f64,
    pub chunk_budget_per_article: usize,
}

impl Default for ScreeningConfig {
    fn default() -> Self {
        Self {
            mode: ScreeningMode::Abstract,
            enhanced_top_k: crate::screening::chunk_retrieval::DEFAULT_TOP_K,
            enhanced_sections: vec!["Methods".to_string(), "Results".to_string()],
            two_stage_low: 0.4,
            two_stage_high: 0.7,
            chunk_budget_per_article:
                crate::screening::chunk_retrieval::DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreeningProgress {
    pub total: usize,
    pub completed: usize,
    pub included: usize,
    pub rejected: usize,
    pub errors: usize,
    pub is_running: bool,
    pub current_article_titles: Vec<String>,
    pub elapsed_ms: u64,
    pub estimated_remaining_ms: Option<u64>,
    /// Tier 3 two-stage: human-readable stage label for a single sub-line under
    /// the main count (e.g. `"Stage 2: 3/12 borderline (full text)"`). `None`
    /// for abstract + enhanced modes (single-stage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// Tier 3 two-stage: the per-stage total (e.g. the borderline article count
    /// for stage 2). `None` for single-stage modes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_total: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmScreeningResponse {
    pub decision: String,
    pub reasoning: String,
    #[serde(
        default,
        alias = "matched_inclusion_criteria",
        alias = "inclusionCriteria",
        alias = "inclusion_criteria"
    )]
    pub matched_inclusion_criteria: Vec<String>,
    #[serde(
        default,
        alias = "matched_exclusion_criteria",
        alias = "exclusionCriteria",
        alias = "exclusion_criteria"
    )]
    pub matched_exclusion_criteria: Vec<String>,
    #[serde(default, alias = "suggested_tags", alias = "tags")]
    pub suggested_tags: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default, alias = "extracted_terms", alias = "extractedTerms")]
    pub extracted_terms: Vec<String>,
}

pub struct ScreeningEngine {
    progress: Arc<Mutex<ScreeningProgress>>,
    cancel_token: Arc<Mutex<bool>>,
    pause_token: Arc<Mutex<bool>>,
    batch_size: usize,
}

impl Default for ScreeningEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreeningEngine {
    pub fn new() -> Self {
        Self {
            progress: Arc::new(Mutex::new(ScreeningProgress::default())),
            cancel_token: Arc::new(Mutex::new(false)),
            pause_token: Arc::new(Mutex::new(false)),
            batch_size: 1,
        }
    }

    /// Create engine with a specific batch size.
    pub fn with_batch_size(batch_size: usize) -> Self {
        Self {
            progress: Arc::new(Mutex::new(ScreeningProgress::default())),
            cancel_token: Arc::new(Mutex::new(false)),
            pause_token: Arc::new(Mutex::new(false)),
            batch_size: batch_size.max(1),
        }
    }

    pub async fn get_progress(&self) -> ScreeningProgress {
        self.progress.lock().await.clone()
    }

    pub async fn cancel(&self) {
        *self.cancel_token.lock().await = true;
    }

    pub async fn pause(&self) {
        *self.pause_token.lock().await = true;
    }

    pub async fn resume(&self) {
        *self.pause_token.lock().await = false;
    }

    /// Run the screening engine using the std::sync::Mutex<Connection> from DbState.
    /// If `app_handle` is provided, emits `screening:progress` events after each article.
    /// Accepts an `LlmClient` trait object so tests can inject mocks.
    ///
    /// `config` (Tier 3) selects abstract / enhanced / two-stage behavior. Pass
    /// `ScreeningConfig::default()` for the legacy abstract-only path.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_sync(
        &self,
        conn_mutex: &std::sync::Mutex<Connection>,
        llm: &dyn LlmClient,
        request_delay_ms: u64,
        criteria: Vec<Criterion>,
        aims: Vec<ResearchAim>,
        config: ScreeningConfig,
        app_handle: Option<tauri::AppHandle>,
    ) -> Result<(), AppError> {
        // Reset state
        *self.cancel_token.lock().await = false;
        *self.pause_token.lock().await = false;

        // Get total count of unscreened working articles for progress tracking
        let total = {
            let c = conn_mutex.lock().map_err(|e| {
                AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
            })?;
            article_repo::count_unscreened_working(&c)?
        };

        if total == 0 {
            return Ok(());
        }

        let inclusion_criteria: Vec<&Criterion> = criteria
            .iter()
            .filter(|c| matches!(c.criterion_type, CriterionType::Inclusion))
            .collect();
        let exclusion_criteria: Vec<&Criterion> = criteria
            .iter()
            .filter(|c| matches!(c.criterion_type, CriterionType::Exclusion))
            .collect();

        // Build global criterion numbering: inclusion [1]..[N], then exclusion [N+1]..[N+M]
        let global_numbering =
            build_global_criterion_numbering(&inclusion_criteria, &exclusion_criteria);

        // Fetch existing tags and labels for the prompt so the LLM prefers matching them
        let (existing_tag_names, existing_label_names) = {
            let c = conn_mutex.lock().map_err(|e| {
                AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
            })?;
            let tags = tag_repo::get_all_tags(&c)?;
            let labels = label_repo::get_all_labels(&c)?;
            (
                tags.into_iter().map(|t| t.name).collect::<Vec<_>>(),
                labels.into_iter().map(|l| l.name).collect::<Vec<_>>(),
            )
        };

        // Build shared prompt parts (aims, criteria)
        let aim_entries: Vec<AimEntry> =
            aims.iter().map(|a| AimEntry { text: a.text.clone() }).collect();
        let inc_entries: Vec<CriterionEntry> = inclusion_criteria
            .iter()
            .map(|c| CriterionEntry {
                id: c.id.clone(),
                text: c.text.clone(),
                priority: c.priority,
            })
            .collect();
        let exc_entries: Vec<CriterionEntry> = exclusion_criteria
            .iter()
            .map(|c| CriterionEntry {
                id: c.id.clone(),
                text: c.text.clone(),
                priority: c.priority,
            })
            .collect();

        // Tier 3: pre-build criterion text vectors for evidence retrieval
        // (rank_chunks_by_criteria scores chunks against these tokens). Built once;
        // reused per article inside the loop.
        let inclusion_texts: Vec<String> =
            inclusion_criteria.iter().map(|c| c.text.clone()).collect();
        let exclusion_texts: Vec<String> =
            exclusion_criteria.iter().map(|c| c.text.clone()).collect();
        let enhanced_mode = config.mode == ScreeningMode::Enhanced;
        let two_stage_mode = config.mode == ScreeningMode::TwoStage;

        // Tier 3: two-stage emits a stage label on the progress sub-line.
        if two_stage_mode {
            let mut progress = self.progress.lock().await;
            progress.stage = Some("Stage 1: abstract".to_string());
            progress.stage_total = None;
        }

        // Initialize progress
        {
            let mut progress = self.progress.lock().await;
            progress.total = total;
            progress.completed = 0;
            progress.included = 0;
            progress.rejected = 0;
            progress.errors = 0;
            progress.is_running = true;
        }

        let start = Instant::now();
        let max_retries: u32 = 3;

        loop {
            // Check cancellation
            if *self.cancel_token.lock().await {
                break;
            }

            // Wait while paused
            while *self.pause_token.lock().await {
                sleep(Duration::from_millis(200)).await;
                if *self.cancel_token.lock().await {
                    break;
                }
            }
            if *self.cancel_token.lock().await {
                break;
            }

            // 1. Fetch next batch from DB
            let batch = {
                let c = conn_mutex.lock().map_err(|e| {
                    AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
                })?;
                article_repo::get_next_unscreened_working_batch(&c, self.batch_size)?
            };

            if batch.is_empty() {
                break;
            }

            // Set all current article titles and emit immediately so the UI
            // shows what is being screened before the LLM call completes.
            {
                let mut progress = self.progress.lock().await;
                progress.current_article_titles = batch.iter().map(|a| a.title.clone()).collect();
                self.emit_progress(&app_handle, &progress);
            }

            // Build prompt with batch of articles.
            //
            // Tier 3 enhanced mode: for articles with `has_full_text`, retrieve
            // the top-K criteria-targeted chunks and attach as evidence so the
            // single batched LLM call sees Methods/Results context. Articles
            // without full text (or with no surviving chunks) keep
            // `full_text_evidence = None` and screen abstract-only.
            //
            // Two-stage mode: stage 1 is always abstract-only (no evidence);
            // borderline articles get a second evidence-bearing call below.
            // Tier 3 Gap 7: collect the precise evidence-sections label per
            // article so the audit trail names the sections that *actually*
            // matched (e.g. `"§Methods"` when only a Methods chunk survived
            // ranking), not the configured allow-list (e.g. `"§Methods,
            // §Results"`) regardless of what was retrieved.
            let mut enhanced_evidence_labels: HashMap<String, String> = HashMap::new();

            let article_entries: Vec<ArticleEntry> = {
                let mut entries: Vec<ArticleEntry> = batch
                    .iter()
                    .map(|a| ArticleEntry {
                        title: a.title.clone(),
                        authors: a.authors.join("; "),
                        year: a.publication_year,
                        abstract_text: a.abstract_text.clone(),
                        full_text_evidence: None,
                    })
                    .collect();
                if enhanced_mode {
                    let c = conn_mutex.lock().map_err(|e| {
                        AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
                    })?;
                    for (entry, article) in entries.iter_mut().zip(batch.iter()) {
                        if !article.has_full_text {
                            continue;
                        }
                        if let Some(ev) = retrieve_evidence_for_article(
                            &c,
                            &article.id,
                            &inclusion_texts,
                            &exclusion_texts,
                            &config,
                        ) {
                            entry.full_text_evidence = Some(ev.text);
                            enhanced_evidence_labels.insert(article.id.clone(), ev.sections_label);
                        }
                    }
                }
                entries
            };

            let prompt_input = ScreeningPromptInput {
                aims: aim_entries.clone(),
                inclusion_criteria: inc_entries.clone(),
                exclusion_criteria: exc_entries.clone(),
                articles: article_entries,
                existing_tags: existing_tag_names.clone(),
                existing_labels: existing_label_names.clone(),
            };

            let user_prompt = prompt::build_screening_prompt(&prompt_input);
            let system_prompt = prompt::SYSTEM_PROMPT;

            // Stage-1 / abstract / enhanced: categorize as Screening. Two-stage
            // stage-2 calls (below) use EnhancedScreening.
            let request_type = if enhanced_mode {
                LlmRequestType::EnhancedScreening
            } else {
                LlmRequestType::Screening
            };

            // Send to LLM with retry on 429
            let mut response_data = None;
            let mut retry_count: u32 = 0;

            while retry_count <= max_retries {
                match llm.send_with_type(system_prompt, &user_prompt, request_type.clone()).await {
                    Ok((text, tokens)) => {
                        response_data = Some((text, tokens));
                        break;
                    }
                    Err(e)
                        if e.to_string().contains("429")
                            || e.to_string().to_lowercase().contains("rate limit") =>
                    {
                        retry_count += 1;
                        let delay_secs = 2u64.pow(retry_count);
                        sleep(Duration::from_secs(delay_secs)).await;
                    }
                    Err(e) => {
                        // Mark all articles in batch as errors
                        let c = conn_mutex.lock().map_err(|e2| {
                            AppError::Database(rusqlite::Error::InvalidParameterName(
                                e2.to_string(),
                            ))
                        })?;
                        for article in &batch {
                            set_screening_error(&c, &article.id, &e.to_string(), None)?;
                        }
                        // Log system error to audit trail
                        let _ = audit_repo::log_error(&c, &format!("LLM request failed: {}", e));
                        break;
                    }
                }
            }

            // Apply delay between requests
            sleep(Duration::from_millis(request_delay_ms)).await;

            let (response_text, total_tokens) = match response_data {
                Some(data) => data,
                None => {
                    let mut progress = self.progress.lock().await;
                    progress.errors += batch.len();
                    progress.completed += batch.len();
                    self.emit_progress(&app_handle, &progress);
                    continue;
                }
            };

            // Calculate pro-rated tokens per article
            let tokens_per_article = total_tokens / batch.len();

            // Parse array response
            match process_screening_responses(&response_text) {
                Ok(screenings) => {
                    // Validate response count matches batch size
                    if screenings.len() != batch.len() {
                        {
                            let c = conn_mutex.lock().map_err(|e2| {
                                AppError::Database(rusqlite::Error::InvalidParameterName(
                                    e2.to_string(),
                                ))
                            })?;
                            for article in &batch {
                                set_screening_error(
                                    &c,
                                    &article.id,
                                    &format!(
                                        "LLM returned {} results for {} articles",
                                        screenings.len(),
                                        batch.len()
                                    ),
                                    Some(&response_text),
                                )?;
                            }
                        }
                        let mut progress = self.progress.lock().await;
                        progress.errors += batch.len();
                        progress.completed += batch.len();
                        self.emit_progress(&app_handle, &progress);
                        continue;
                    }

                    // Process each article/result pair
                    for (article, screening) in batch.iter().zip(screenings.iter()) {
                        // Handle error decision from LLM
                        if screening.decision == "error" {
                            {
                                let c = conn_mutex.lock().map_err(|e2| {
                                    AppError::Database(rusqlite::Error::InvalidParameterName(
                                        e2.to_string(),
                                    ))
                                })?;
                                set_screening_error(&c, &article.id, &screening.reasoning, None)?;
                            }
                            let mut progress = self.progress.lock().await;
                            progress.errors += 1;
                            progress.completed += 1;
                            continue;
                        }

                        // Apply priority resolution - match by UUID or by criterion text
                        let inc_matches: Vec<CriterionMatch> = screening
                            .matched_inclusion_criteria
                            .iter()
                            .filter_map(|key| {
                                criteria.iter().find(|c| c.id == *key || c.text == *key)
                            })
                            .map(|c| CriterionMatch {
                                id: c.id.clone(),
                                criterion_type: c.criterion_type.clone(),
                                priority: c.priority,
                            })
                            .collect();

                        let exc_matches: Vec<CriterionMatch> = screening
                            .matched_exclusion_criteria
                            .iter()
                            .filter_map(|key| {
                                criteria.iter().find(|c| c.id == *key || c.text == *key)
                            })
                            .map(|c| CriterionMatch {
                                id: c.id.clone(),
                                criterion_type: c.criterion_type.clone(),
                                priority: c.priority,
                            })
                            .collect();

                        // Collect auto-label info before the matches are moved
                        let auto_label_criteria: Vec<(String, String)> = inc_matches
                            .iter()
                            .chain(exc_matches.iter())
                            .filter_map(|m| {
                                criteria.iter().find(|cr| cr.id == m.id).map(|cr| {
                                    (
                                        if matches!(cr.criterion_type, CriterionType::Inclusion) {
                                            "Inclusion"
                                        } else {
                                            "Exclusion"
                                        }
                                        .to_string(),
                                        cr.text.clone(),
                                    )
                                })
                            })
                            .collect();

                        let resolution_input = resolution::ScreeningInput {
                            inclusion_matches: inc_matches,
                            exclusion_matches: exc_matches,
                        };
                        let final_decision = resolution::resolve_decision(&resolution_input);

                        // Augment matched arrays with any criteria UUIDs mentioned in reasoning
                        // but missing from the LLM's matched arrays
                        let inclusion_count = inclusion_criteria.len();
                        let (augmented_inc, augmented_exc) = augment_matched_from_reasoning(
                            &screening.reasoning,
                            &screening.matched_inclusion_criteria,
                            &screening.matched_exclusion_criteria,
                            &global_numbering,
                            inclusion_count,
                        );

                        // Keep raw reasoning with UUIDs - frontend replaces dynamically at display time
                        let mut reasoning = screening.reasoning.clone();

                        // Check for override
                        let ai_decision_str = screening.decision.as_str();
                        if ai_decision_str != final_decision {
                            reasoning.push_str(&format!(
                                "\n\n[App override: {} favored due to priority resolution]",
                                if final_decision == "include" { "inclusion" } else { "exclusion" }
                            ));
                        }

                        // Tier 3 Gap 7: use the precise evidence-sections label
                        // captured during retrieval (the sections that *actually*
                        // matched), not the configured allow-list. Two-stage
                        // stage-1 stays `ai_screen` (stage 2 writes the
                        // `ai_screen_enhanced` entry below).
                        let evidence_sections = enhanced_evidence_labels.get(&article.id).cloned();

                        // Update article in DB
                        {
                            let c = conn_mutex.lock().map_err(|e2| {
                                AppError::Database(rusqlite::Error::InvalidParameterName(
                                    e2.to_string(),
                                ))
                            })?;
                            update_article_after_screening(
                                &c,
                                ScreeningUpdate {
                                    article_id: &article.id,
                                    decision: final_decision,
                                    reasoning: &reasoning,
                                    confidence: screening.confidence,
                                    matched_inc: &augmented_inc,
                                    matched_exc: &augmented_exc,
                                    actual_tokens: Some(tokens_per_article),
                                    evidence_sections: evidence_sections.as_deref(),
                                },
                            )?;

                            // Create/update tags from suggested_tags
                            for tag_name in &screening.suggested_tags {
                                let _ = create_or_match_tag(&c, tag_name, &article.id);
                            }

                            // Auto-label: apply labels from matched criteria
                            for (prefix, text) in &auto_label_criteria {
                                let label_name = format!("{}: {}", prefix, text);
                                let _ = create_or_match_label(&c, &label_name, &article.id);
                            }

                            // Save extracted terms to biblio tables for bibliometrics
                            if !screening.extracted_terms.is_empty() {
                                let terms: Vec<(String, TermType, TermSource)> = screening
                                    .extracted_terms
                                    .iter()
                                    .map(|t| {
                                        (t.clone(), TermType::NounPhrase, TermSource::AiExtracted)
                                    })
                                    .collect();
                                let _ = biblio_repo::save_article_terms(&c, &article.id, &terms);
                            }
                        }

                        // Update progress
                        let mut progress = self.progress.lock().await;
                        progress.completed += 1;
                        if final_decision == "include" {
                            progress.included += 1;
                        } else {
                            progress.rejected += 1;
                        }
                    }

                    // Tier 3 two-stage: stage 2. After stage 1 has written
                    // decisions for the whole batch, re-screen borderline
                    // articles (confidence in `[low, high)` AND has full text)
                    // with full-text evidence. The stage-2 decision overrides
                    // stage 1 and passes through `resolve_decision` again.
                    // Both passes are recorded: stage 1 already wrote
                    // `ai_screen`; stage 2 writes `ai_screen_enhanced`.
                    if two_stage_mode {
                        // Collect borderline articles from the batch (needs the
                        // stage-1 screening responses, which we still hold).
                        let borderline: Vec<(
                            &crate::models::article::Article,
                            &LlmScreeningResponse,
                        )> = batch
                            .iter()
                            .zip(screenings.iter())
                            .filter(|(a, s)| {
                                a.has_full_text
                                    && s.decision != "error"
                                    && s.confidence >= config.two_stage_low
                                    && s.confidence < config.two_stage_high
                            })
                            .collect();

                        if !borderline.is_empty() {
                            // Update progress sub-line for stage 2.
                            {
                                let mut progress = self.progress.lock().await;
                                progress.stage_total = Some(borderline.len());
                                progress.stage = Some(format!(
                                    "Stage 2: 0/{} borderline (full text)",
                                    borderline.len()
                                ));
                                self.emit_progress(&app_handle, &progress);
                            }

                            for (stage2_done, (article, _stage1)) in borderline.iter().enumerate() {
                                // Cancel/pause checks between stage-2 articles.
                                if *self.cancel_token.lock().await {
                                    break;
                                }
                                while *self.pause_token.lock().await {
                                    sleep(Duration::from_millis(200)).await;
                                    if *self.cancel_token.lock().await {
                                        break;
                                    }
                                }

                                // Retrieve evidence for this borderline article.
                                let evidence = {
                                    let c = conn_mutex.lock().map_err(|e| {
                                        AppError::Database(rusqlite::Error::InvalidParameterName(
                                            e.to_string(),
                                        ))
                                    })?;
                                    retrieve_evidence_for_article(
                                        &c,
                                        &article.id,
                                        &inclusion_texts,
                                        &exclusion_texts,
                                        &config,
                                    )
                                };

                                // If no evidence survived ranking, skip stage 2
                                // for this article (stage-1 decision stands).
                                // Update the progress sub-line first so the UI
                                // does not stall at the previous count.
                                let evidence = match evidence {
                                    Some(ev) => ev,
                                    None => {
                                        let mut progress = self.progress.lock().await;
                                        progress.stage = Some(format!(
                                            "Stage 2: {}/{} borderline (full text)",
                                            stage2_done + 1,
                                            borderline.len()
                                        ));
                                        self.emit_progress(&app_handle, &progress);
                                        continue;
                                    }
                                };

                                // Build a single-article stage-2 prompt.
                                let entry = ArticleEntry {
                                    title: article.title.clone(),
                                    authors: article.authors.join("; "),
                                    year: article.publication_year,
                                    abstract_text: article.abstract_text.clone(),
                                    full_text_evidence: Some(evidence.text),
                                };
                                let prompt_input = ScreeningPromptInput {
                                    aims: aim_entries.clone(),
                                    inclusion_criteria: inc_entries.clone(),
                                    exclusion_criteria: exc_entries.clone(),
                                    articles: vec![entry],
                                    existing_tags: existing_tag_names.clone(),
                                    existing_labels: existing_label_names.clone(),
                                };
                                let user_prompt = prompt::build_screening_prompt(&prompt_input);
                                let system_prompt = prompt::SYSTEM_PROMPT;

                                // Stage-2 call: categorize as EnhancedScreening.
                                let stage2_response = llm
                                    .send_with_type(
                                        system_prompt,
                                        &user_prompt,
                                        LlmRequestType::EnhancedScreening,
                                    )
                                    .await;
                                sleep(Duration::from_millis(request_delay_ms)).await;

                                let (response_text, total_tokens) = match stage2_response {
                                    Ok(data) => data,
                                    Err(e) => {
                                        // Non-fatal: log + keep stage-1 decision.
                                        // Scope the connection guard so it drops
                                        // before the progress `.await` below.
                                        {
                                            let c = conn_mutex.lock().map_err(|e2| {
                                                AppError::Database(
                                                    rusqlite::Error::InvalidParameterName(
                                                        e2.to_string(),
                                                    ),
                                                )
                                            })?;
                                            let _ = audit_repo::log_error(
                                                &c,
                                                &format!(
                                                    "Stage-2 screening failed for {}: {}",
                                                    article.id, e
                                                ),
                                            );
                                        }
                                        // Update progress sub-line and continue.
                                        let mut progress = self.progress.lock().await;
                                        progress.stage = Some(format!(
                                            "Stage 2: {}/{} borderline (full text)",
                                            stage2_done + 1,
                                            borderline.len()
                                        ));
                                        self.emit_progress(&app_handle, &progress);
                                        continue;
                                    }
                                };

                                // Parse the single-article stage-2 response.
                                match process_screening_responses(&response_text) {
                                    Ok(mut stage2_screenings) if stage2_screenings.len() == 1 => {
                                        let stage2 = stage2_screenings.swap_remove(0);
                                        if stage2.decision == "error" {
                                            // Keep stage-1 decision; don't override with error.
                                            let mut progress = self.progress.lock().await;
                                            progress.stage = Some(format!(
                                                "Stage 2: {}/{} borderline (full text)",
                                                stage2_done + 1,
                                                borderline.len()
                                            ));
                                            self.emit_progress(&app_handle, &progress);
                                            continue;
                                        }

                                        // Resolve stage-2 matches through the
                                        // priority layer (per Appendix decision).
                                        let inc_matches: Vec<CriterionMatch> = stage2
                                            .matched_inclusion_criteria
                                            .iter()
                                            .filter_map(|key| {
                                                criteria
                                                    .iter()
                                                    .find(|c| c.id == *key || c.text == *key)
                                            })
                                            .map(|c| CriterionMatch {
                                                id: c.id.clone(),
                                                criterion_type: c.criterion_type.clone(),
                                                priority: c.priority,
                                            })
                                            .collect();
                                        let exc_matches: Vec<CriterionMatch> = stage2
                                            .matched_exclusion_criteria
                                            .iter()
                                            .filter_map(|key| {
                                                criteria
                                                    .iter()
                                                    .find(|c| c.id == *key || c.text == *key)
                                            })
                                            .map(|c| CriterionMatch {
                                                id: c.id.clone(),
                                                criterion_type: c.criterion_type.clone(),
                                                priority: c.priority,
                                            })
                                            .collect();
                                        let resolution_input = resolution::ScreeningInput {
                                            inclusion_matches: inc_matches.clone(),
                                            exclusion_matches: exc_matches.clone(),
                                        };
                                        let final_decision =
                                            resolution::resolve_decision(&resolution_input);

                                        // Auto-label from stage-2 matches.
                                        let auto_label_criteria: Vec<(String, String)> =
                                            inc_matches
                                                .iter()
                                                .chain(exc_matches.iter())
                                                .filter_map(|m| {
                                                    criteria.iter().find(|cr| cr.id == m.id).map(
                                                        |cr| {
                                                            (
                                                                if matches!(
                                                                    cr.criterion_type,
                                                                    CriterionType::Inclusion
                                                                ) {
                                                                    "Inclusion"
                                                                } else {
                                                                    "Exclusion"
                                                                }
                                                                .to_string(),
                                                                cr.text.clone(),
                                                            )
                                                        },
                                                    )
                                                })
                                                .collect();

                                        let (augmented_inc, augmented_exc) =
                                            augment_matched_from_reasoning(
                                                &stage2.reasoning,
                                                &stage2.matched_inclusion_criteria,
                                                &stage2.matched_exclusion_criteria,
                                                &global_numbering,
                                                inclusion_criteria.len(),
                                            );

                                        let mut reasoning = stage2.reasoning.clone();
                                        if stage2.decision.as_str() != final_decision {
                                            reasoning.push_str(&format!(
                                                "\n\n[App override: {} favored due to priority resolution]",
                                                if final_decision == "include" {
                                                    "inclusion"
                                                } else {
                                                    "exclusion"
                                                }
                                            ));
                                        }

                                        // Adjust progress counters: the stage-1
                                        // include/exclude tallies already counted
                                        // this article; correct them to the
                                        // stage-2 (final) decision.
                                        //
                                        // The connection guard is scoped tightly
                                        // so it drops before the `progress`
                                        // `.await` below (the `std::sync`
                                        // MutexGuard is not `Send`).
                                        let stage1_was_include = {
                                            let c = conn_mutex.lock().map_err(|e2| {
                                                AppError::Database(
                                                    rusqlite::Error::InvalidParameterName(
                                                        e2.to_string(),
                                                    ),
                                                )
                                            })?;
                                            // Read the stage-1 decision so we can
                                            // fix up the progress tallies.
                                            let stage1_status: Option<String> = c
                                                .query_row(
                                                    "SELECT status FROM articles WHERE id = ?1",
                                                    rusqlite::params![&article.id],
                                                    |row| row.get(0),
                                                )
                                                .ok();
                                            let stage1_was_include =
                                                stage1_status.as_deref() == Some("included");

                                            update_article_after_screening(
                                                &c,
                                                ScreeningUpdate {
                                                    article_id: &article.id,
                                                    decision: final_decision,
                                                    reasoning: &reasoning,
                                                    confidence: stage2.confidence,
                                                    matched_inc: &augmented_inc,
                                                    matched_exc: &augmented_exc,
                                                    actual_tokens: Some(total_tokens),
                                                    evidence_sections: Some(
                                                        &evidence.sections_label,
                                                    ),
                                                },
                                            )?;

                                            for tag_name in &stage2.suggested_tags {
                                                let _ =
                                                    create_or_match_tag(&c, tag_name, &article.id);
                                            }
                                            for (prefix, text) in &auto_label_criteria {
                                                let label_name = format!("{}: {}", prefix, text);
                                                let _ = create_or_match_label(
                                                    &c,
                                                    &label_name,
                                                    &article.id,
                                                );
                                            }
                                            stage1_was_include
                                        };

                                        // Fix up progress include/exclude tallies
                                        // if the decision flipped.
                                        {
                                            let mut progress = self.progress.lock().await;
                                            let now_include = final_decision == "include";
                                            if now_include != stage1_was_include {
                                                if now_include {
                                                    progress.included += 1;
                                                    progress.rejected =
                                                        progress.rejected.saturating_sub(1);
                                                } else {
                                                    progress.rejected += 1;
                                                    progress.included =
                                                        progress.included.saturating_sub(1);
                                                }
                                            }
                                            progress.stage = Some(format!(
                                                "Stage 2: {}/{} borderline (full text)",
                                                stage2_done + 1,
                                                borderline.len()
                                            ));
                                            self.emit_progress(&app_handle, &progress);
                                        }
                                    }
                                    _ => {
                                        // Mismatched count / parse error: keep stage-1 decision.
                                        let mut progress = self.progress.lock().await;
                                        progress.stage = Some(format!(
                                            "Stage 2: {}/{} borderline (full text)",
                                            stage2_done + 1,
                                            borderline.len()
                                        ));
                                        self.emit_progress(&app_handle, &progress);
                                    }
                                }
                            }
                        }
                    }

                    // Update elapsed time after entire batch
                    let mut progress = self.progress.lock().await;
                    let elapsed = start.elapsed().as_millis() as u64;
                    progress.elapsed_ms = elapsed;
                    if progress.completed > 0 {
                        let avg_per_article = elapsed / progress.completed as u64;
                        let remaining = (total - progress.completed) as u64;
                        progress.estimated_remaining_ms = Some(avg_per_article * remaining);
                    }
                    self.emit_progress(&app_handle, &progress);
                }
                Err(parse_err) => {
                    {
                        let c = conn_mutex.lock().map_err(|e2| {
                            AppError::Database(rusqlite::Error::InvalidParameterName(
                                e2.to_string(),
                            ))
                        })?;
                        for article in &batch {
                            set_screening_error(
                                &c,
                                &article.id,
                                &format!("Malformed LLM response: {parse_err}"),
                                Some(&response_text),
                            )?;
                        }
                        // Log system error to audit trail
                        let _ = audit_repo::log_error(
                            &c,
                            &format!("Malformed LLM response: {parse_err}"),
                        );
                    }
                    let mut progress = self.progress.lock().await;
                    progress.errors += batch.len();
                    progress.completed += batch.len();
                    self.emit_progress(&app_handle, &progress);
                }
            }
        }

        // Mark as done and emit final event
        {
            let mut progress = self.progress.lock().await;
            progress.is_running = false;
            progress.current_article_titles = vec![];
            self.emit_progress(&app_handle, &progress);
        }

        Ok(())
    }

    /// Emit a `screening:progress` event if an app handle is available.
    fn emit_progress(&self, handle: &Option<tauri::AppHandle>, snapshot: &ScreeningProgress) {
        if let Some(h) = handle {
            let _ = h.emit("screening:progress", snapshot);
        }
    }
}

/// Parse the LLM response as a JSON array of screening results.
pub fn process_screening_responses(raw: &str) -> Result<Vec<LlmScreeningResponse>, AppError> {
    debug_log!("[screening] process_screening_responses received {} bytes", raw.len());
    debug_log!("[screening] raw first 300 chars: {}", &raw[..raw.len().min(300)]);

    let json_str = extract_json(raw);

    debug_log!("[screening] extract_json produced {} bytes", json_str.len());
    debug_log!(
        "[screening] extracted JSON first 300 chars: {}",
        &json_str[..json_str.len().min(300)]
    );

    match serde_json::from_str::<Vec<LlmScreeningResponse>>(&json_str) {
        Ok(mut results) => {
            // M9: Validate and normalize LLM decision values
            for r in &mut results {
                let d = r.decision.to_lowercase();
                match d.as_str() {
                    "include" | "exclude" | "error" => {
                        r.decision = d;
                    }
                    _ => {
                        debug_log!(
                            "[screening] Unexpected decision '{}', treating as error",
                            r.decision
                        );
                        r.reasoning = format!(
                            "Unexpected LLM decision: '{}'. Original reasoning: {}",
                            r.decision, r.reasoning
                        );
                        r.decision = "error".to_string();
                    }
                }
            }
            debug_log!("[screening] successfully parsed {} screening results", results.len());
            Ok(results)
        }
        Err(e) => {
            debug_log!("[screening] FAILED to parse screening response: {e}");
            debug_log!(
                "[screening] attempted JSON (first 500 chars): {}",
                &json_str[..json_str.len().min(500)]
            );

            // Try truncated JSON repair: find last complete `}` and add missing `]`
            if let Some(repaired) = repair_truncated_json_array(&json_str) {
                debug_log!("[screening] attempting truncated JSON repair...");
                match serde_json::from_str::<Vec<LlmScreeningResponse>>(&repaired) {
                    Ok(mut results) => {
                        // M9: Validate repaired results too
                        for r in &mut results {
                            let d = r.decision.to_lowercase();
                            match d.as_str() {
                                "include" | "exclude" | "error" => {
                                    r.decision = d;
                                }
                                _ => {
                                    r.reasoning = format!(
                                        "Unexpected LLM decision: '{}'. Original reasoning: {}",
                                        r.decision, r.reasoning
                                    );
                                    r.decision = "error".to_string();
                                }
                            }
                        }
                        debug_log!(
                            "[screening] repair succeeded! Recovered {} results",
                            results.len()
                        );
                        return Ok(results);
                    }
                    Err(_repair_err) => {
                        debug_log!("[screening] repair also failed: {_repair_err}");
                    }
                }
            }

            Err(AppError::Import(format!("Malformed LLM response: {e}")))
        }
    }
}

/// Attempt to repair a truncated JSON array by finding the last complete object
/// and closing the array with `]`.
#[must_use]
pub fn repair_truncated_json_array(json: &str) -> Option<String> {
    // Only attempt if it looks like an incomplete array
    let trimmed = json.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    // If it already ends with `]`, it's not truncated (or is valid)
    if trimmed.ends_with(']') {
        return None;
    }

    // Find the last occurrence of `}` - the end of the last complete object
    let last_brace = trimmed.rfind('}')?;
    let candidate = &trimmed[..=last_brace];

    // Close the array
    let repaired = format!("{}]", candidate);

    // Quick sanity: must still start with `[`
    if !repaired.starts_with('[') {
        return None;
    }

    Some(repaired)
}

pub fn extract_json(raw: &str) -> String {
    let trimmed = raw.trim();

    // Strategy 1: Code-fence stripping
    if trimmed.starts_with("```") {
        let without_start = trimmed.trim_start_matches("```json").trim_start_matches("```");
        let without_end = without_start.trim_end_matches("```");
        let inner = without_end.trim();
        // If the code-fence content is already a bare array, return it
        if inner.starts_with('[') {
            return inner.to_string();
        }
        // If it's a JSON object, try to extract an embedded array
        if inner.starts_with('{') {
            if let Some(arr) = extract_array_from_object(inner) {
                return arr;
            }
        }
        // LLMs may omit the opening `{` - repair brace balance before returning
        return balance_braces(inner);
    }

    // Strategy 2: Bare array (already correct)
    if trimmed.starts_with('[') {
        return trimmed.to_string();
    }

    // Strategy 3: JSON object wrapping an array - extract the array
    if trimmed.starts_with('{') {
        if let Some(arr) = extract_array_from_object(trimmed) {
            return arr;
        }
    }

    // Strategy 4: Try to find a JSON array anywhere in the text
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if end > start {
                let candidate = &trimmed[start..=end];
                // Validate it parses as JSON
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return candidate.to_string();
                }
            }
        }
    }

    // Strategy 5: Try to find a JSON object anywhere in the text
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                let candidate = &trimmed[start..=end];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return candidate.to_string();
                }
            }
        }
    }

    // Final fallback: repair brace balance (e.g. missing opening `{`)
    balance_braces(trimmed)
}

/// Repair missing opening `{` or closing `}` in a JSON-like string.
/// LLMs sometimes omit the opening brace, producing e.g. `"field": "value" ... }`.
#[must_use]
pub fn balance_braces(s: &str) -> String {
    // Count structural braces (ignoring those inside JSON string literals)
    let mut open = 0usize;
    let mut close = 0usize;
    let mut in_string = false;
    let mut escape_next = false;

    for ch in s.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            if ch == '{' {
                open += 1;
            } else if ch == '}' {
                close += 1;
            }
        }
    }

    let mut result = s.to_string();
    // Missing opening braces: prepend them
    if close > open {
        for _ in 0..(close - open) {
            result.insert(0, '{');
        }
    }
    // Missing closing braces: append them
    if open > close {
        for _ in 0..(open - close) {
            result.push('}');
        }
    }
    result
}

/// Given a JSON object string, find and extract the first JSON array value
/// at the first two levels of nesting that contains objects (screening results).
fn extract_array_from_object(obj_str: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(obj_str).ok()?;
    extract_array_from_value(&value)
}

/// Recursively search for a non-empty array whose first element is an object.
fn extract_array_from_value(value: &serde_json::Value) -> Option<String> {
    if let Some(obj) = value.as_object() {
        // Level 1: scan top-level keys for arrays containing objects
        for (_, v) in obj {
            if let Some(arr) = v.as_array() {
                if arr.first().is_some_and(|el| el.is_object()) {
                    return Some(v.to_string());
                }
            }
        }
        // Level 2: scan nested objects
        for (_, v) in obj {
            if let Some(result) = extract_array_from_value(v) {
                return Some(result);
            }
        }
    }
    None
}

fn set_screening_error(
    conn: &Connection,
    article_id: &str,
    error_message: &str,
    raw_response: Option<&str>,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET screening_error = 1, screened_at = datetime('now'), changed_at = datetime('now') WHERE id = ?1",
        rusqlite::params![article_id],
    )?;

    let audit_id = uuid::Uuid::new_v4().to_string();
    let details = match raw_response {
        Some(raw) => {
            let truncated = &raw[..raw.len().min(300)];
            format!("Screening error: {error_message}\n\nRaw LLM response (first 300 chars): {truncated}")
        }
        None => format!("Screening error: {error_message}"),
    };
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'ai_screen', ?3, 'ai')",
        rusqlite::params![audit_id, article_id, details],
    )?;

    Ok(())
}

struct ScreeningUpdate<'a> {
    article_id: &'a str,
    decision: &'a str,
    reasoning: &'a str,
    confidence: f64,
    matched_inc: &'a [String],
    matched_exc: &'a [String],
    actual_tokens: Option<usize>,
    /// Tier 3: when `Some`, the audit detail line names the evidence sections
    /// used (e.g. `"§Methods, §Results"`), producing an `ai_screen_enhanced`
    /// audit action. When `None`, the audit action is the legacy `ai_screen`.
    evidence_sections: Option<&'a str>,
}

fn update_article_after_screening(
    conn: &Connection,
    update: ScreeningUpdate,
) -> Result<(), AppError> {
    let new_status = if update.decision == "include" { "included" } else { "rejected" };
    let matched_inc_json = serde_json::to_string(update.matched_inc)?;
    let matched_exc_json = serde_json::to_string(update.matched_exc)?;

    // Tier 3 Gap 6: two-stage screening calls this twice for borderline
    // articles (stage 1 then stage 2). The flat `actual_tokens = ?7` write
    // previously discarded the stage-1 token count. Accumulate atomically via
    // `COALESCE(actual_tokens, 0) + ?7` so the column reflects the full cost
    // (stage 1 starts from NULL → `COALESCE(NULL,0)+t == t`, unchanged).
    conn.execute(
        "UPDATE articles SET status = ?1, ai_decision = ?2, ai_reasoning = ?3, ai_confidence = ?4, \
         matched_inclusion_criteria = ?5, matched_exclusion_criteria = ?6, screened_at = datetime('now'), changed_at = datetime('now'), \
         actual_tokens = COALESCE(actual_tokens, 0) + ?7 \
         WHERE id = ?8",
        rusqlite::params![
            new_status,
            update.decision,
            update.reasoning,
            update.confidence,
            matched_inc_json,
            matched_exc_json,
            update.actual_tokens,
            update.article_id
        ],
    )?;

    let audit_id = uuid::Uuid::new_v4().to_string();
    // Tier 3: enhanced / two-stage stage-2 entries use the `ai_screen_enhanced`
    // action and name the evidence sections in the details so decision flips
    // are visible in the audit trail. Abstract / stage-1 entries stay `ai_screen`.
    let (action, details) = match update.evidence_sections {
        Some(sections) => (
            "ai_screen_enhanced",
            format!(
                "AI screened (enhanced) with {} evidence: {} (confidence: {:.2})",
                sections, update.decision, update.confidence
            ),
        ),
        None => (
            "ai_screen",
            format!("AI screened: {} (confidence: {:.2})", update.decision, update.confidence),
        ),
    };
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, action, from_status, to_status, details, source) \
         VALUES (?1, ?2, ?3, 'working', ?4, ?5, 'ai')",
        rusqlite::params![audit_id, update.article_id, action, new_status, details],
    )?;

    Ok(())
}

/// Format scored chunks as the `## Supporting Evidence from Full Text` body for
/// one article.
///
/// **Delegate.** The canonical implementation lives in
/// `chunk_retrieval::format_chunks_as_evidence` (the lowest-level module both
/// this engine and `evidence::resolve_evidence` depend on). This thin wrapper
/// is kept `pub` for backward compatibility with external tests
/// (`tests/evidence_test.rs` references `engine::format_chunks_as_evidence`)
/// and routes straight through so behavior is byte-identical.
#[must_use]
pub fn format_chunks_as_evidence(chunks: &[ScoredChunk]) -> Option<String> {
    crate::screening::chunk_retrieval::format_chunks_as_evidence(chunks)
}

/// The evidence body string plus the deduped section labels that survived
/// ranking (e.g. `"§Methods, §Results"`), for the audit trail.
#[derive(Debug, Clone)]
struct ArticleEvidence {
    /// The formatted `[§Methods] ...` block (the `full_text_evidence` value).
    text: String,
    /// Section labels actually present in the retrieved chunks, joined for the
    /// audit detail (e.g. `"§Methods, §Results"`). Stable order: deduped,
    /// preserved in retrieval (highest-ranked-first) order.
    sections_label: String,
}

/// Rank the given chunks against the criteria text, returning the top-K
/// scored chunks (no DB, no formatting). Pure helper extracted from
/// `retrieve_evidence_for_article` so it can be tested without a DB.
///
/// Tier 4.1: this is now the rank-only half; the formatting lives in
/// `evidence::resolve_evidence` (which picks between summary+chunk, summary
/// alone, or chunks-only formatting).
fn rank_evidence_chunks(
    chunks: Vec<crate::utils::chunking::Chunk>,
    inclusion_texts: &[String],
    exclusion_texts: &[String],
    config: &ScreeningConfig,
) -> Vec<ScoredChunk> {
    // Filter to the section allow-list (default Methods/Results). Chunks whose
    // section is NULL/unknown are kept (they may carry signal; the allow-list
    // only restricts named sections that are out of scope like Discussion).
    let allow = &config.enhanced_sections;
    let filtered: Vec<_> = chunks
        .into_iter()
        .filter(|c| match c.section.as_deref() {
            Some(s) => allow.iter().any(|a| a.eq_ignore_ascii_case(s)),
            None => true,
        })
        .collect();
    if filtered.is_empty() {
        return Vec::new();
    }
    rank_chunks_by_criteria(
        &filtered,
        inclusion_texts,
        exclusion_texts,
        config.enhanced_top_k,
        DEFAULT_MAX_CHUNK_WORDS,
        config.chunk_budget_per_article,
    )
}

/// Tier 3 + Tier 4.1: retrieve + rank + resolve the supporting evidence for one
/// article. Reads the AI-summary blob AND chunks from `article_chunks` (populated
/// at attach time), ranks the chunks, then delegates to
/// `evidence::resolve_evidence` (Q1=B complementarity: summary facts + top-1
/// verbatim chunk when both exist). Returns `None` when neither candidate
/// yields evidence (caller leaves evidence as `None`).
///
/// Tier 3 chunks-only behavior is preserved byte-for-byte when no AI summary
/// exists (covered by `resolve_evidence_chunks_path_unchanged_from_tier3`).
fn retrieve_evidence_for_article(
    conn: &Connection,
    article_id: &str,
    inclusion_texts: &[String],
    exclusion_texts: &[String],
    config: &ScreeningConfig,
) -> Option<ArticleEvidence> {
    // Tier 4.1: fetch the AI-summary blob (Option<String>).
    let ai_summary_json: Option<String> = conn
        .query_row(
            "SELECT full_text_ai_summary FROM articles WHERE id = ?1",
            rusqlite::params![article_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    // Rank the chunks (may be empty).
    let chunks = chunk_repo::list_chunks_for_article(conn, article_id).ok()?;
    let scored = rank_evidence_chunks(chunks, inclusion_texts, exclusion_texts, config);
    // Pure resolution (Q1 = B complementarity).
    let evidence =
        crate::screening::evidence::resolve_evidence(ai_summary_json.as_deref(), &scored);
    if evidence.source_type == crate::screening::evidence::EvidenceSource::None {
        return None;
    }
    Some(ArticleEvidence { text: evidence.text, sections_label: evidence.sections_label })
}

/// Maximum character length for newly created tags or labels that don't match existing ones.
const MAX_NEW_TAG_LABEL_LEN: usize = 30;

pub fn create_or_match_tag(
    conn: &Connection,
    tag_name: &str,
    article_id: &str,
) -> Result<(), AppError> {
    let tag_name_lower = tag_name.to_lowercase();

    // Check if tag exists (case-insensitive)
    let existing_id: Option<String> = conn
        .query_row("SELECT id FROM tags WHERE LOWER(name) = ?1", [&tag_name_lower], |row| {
            row.get(0)
        })
        .ok();

    let tag_id = match existing_id {
        Some(id) => id,
        None => {
            // Trim new tag name to MAX_NEW_TAG_LABEL_LEN characters
            let trimmed: String = tag_name_lower.chars().take(MAX_NEW_TAG_LABEL_LEN).collect();
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO tags (id, name, source) VALUES (?1, ?2, 'ai_suggested')",
                rusqlite::params![id, trimmed],
            )?;
            id
        }
    };

    // Link tag to article (ignore if already linked)
    conn.execute(
        "INSERT OR IGNORE INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, tag_id],
    )?;

    Ok(())
}

pub fn create_or_match_label(
    conn: &Connection,
    label_name: &str,
    article_id: &str,
) -> Result<(), AppError> {
    // Check if label exists (case-insensitive)
    let label_name_lower = label_name.to_lowercase();
    let existing_id: Option<String> = conn
        .query_row("SELECT id FROM labels WHERE LOWER(name) = ?1", [&label_name_lower], |row| {
            row.get(0)
        })
        .ok();

    let label_id = match existing_id {
        Some(id) => id,
        None => {
            // Trim new label name to MAX_NEW_TAG_LABEL_LEN characters
            let trimmed: String = label_name_lower.chars().take(MAX_NEW_TAG_LABEL_LEN).collect();
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO labels (id, name, source) VALUES (?1, ?2, 'ai_generated')",
                rusqlite::params![id, trimmed],
            )?;
            id
        }
    };

    // Link label to article (ignore if already linked)
    conn.execute(
        "INSERT OR IGNORE INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, label_id],
    )?;

    Ok(())
}

/// Build a global criterion numbering map: UUID → 1-based index.
///
/// Inclusion criteria are numbered `[1]..[N]`, then exclusion criteria continue `[N+1]..[N+M]`.
/// This ensures `[3]` always refers to the same criterion regardless of which article is displayed.
pub fn build_global_criterion_numbering(
    inclusion_criteria: &[&Criterion],
    exclusion_criteria: &[&Criterion],
) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    let mut n = 1usize;
    for c in inclusion_criteria {
        map.insert(c.id.clone(), n);
        n += 1;
    }
    for c in exclusion_criteria {
        map.insert(c.id.clone(), n);
        n += 1;
    }
    map
}

/// Scan reasoning text for criterion UUIDs mentioned but missing from matched arrays,
/// and return augmented (inclusion, exclusion) tuples.
///
/// The LLM sometimes references criteria in reasoning without listing them in the
/// matched arrays. This ensures every referenced criterion appears in the UI table.
///
/// `inclusion_count` is the number of inclusion criteria in the global numbering,
/// used to distinguish inclusion UUIDs (indices 1..N) from exclusion UUIDs (N+1..M).
pub fn augment_matched_from_reasoning(
    reasoning: &str,
    matched_inclusion_ids: &[String],
    matched_exclusion_ids: &[String],
    global_map: &HashMap<String, usize>,
    inclusion_count: usize,
) -> (Vec<String>, Vec<String>) {
    let inc_set: HashSet<&str> = matched_inclusion_ids.iter().map(|s| s.as_str()).collect();
    let exc_set: HashSet<&str> = matched_exclusion_ids.iter().map(|s| s.as_str()).collect();

    let mut extra_inclusion = Vec::new();
    let mut extra_exclusion = Vec::new();

    for (uuid, &idx) in global_map {
        if inc_set.contains(uuid.as_str()) || exc_set.contains(uuid.as_str()) {
            continue; // Already in matched arrays
        }
        if reasoning.contains(uuid.as_str()) {
            // Inclusion criteria have indices 1..inclusion_count
            if idx <= inclusion_count {
                extra_inclusion.push(uuid.clone());
            } else {
                extra_exclusion.push(uuid.clone());
            }
        }
    }

    let mut augmented_inc = matched_inclusion_ids.to_vec();
    let mut augmented_exc = matched_exclusion_ids.to_vec();
    augmented_inc.extend(extra_inclusion);
    augmented_exc.extend(extra_exclusion);

    (augmented_inc, augmented_exc)
}
