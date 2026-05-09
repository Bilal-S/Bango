use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::db::article_repo;
use crate::error::AppError;
use crate::llm::client;
use crate::models::criterion::{Criterion, CriterionType, ResearchAim};
use crate::screening::prompt::{
    self, AimEntry, ArticleEntry, CriterionEntry, ScreeningPromptInput,
};
use crate::screening::resolution::{self, CriterionMatch};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreeningProgress {
    pub total: usize,
    pub completed: usize,
    pub included: usize,
    pub rejected: usize,
    pub errors: usize,
    pub is_running: bool,
    pub current_article_title: Option<String>,
    pub elapsed_ms: u64,
    pub estimated_remaining_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmScreeningResponse {
    pub decision: String,
    pub reasoning: String,
    pub matched_inclusion_criteria: Vec<String>,
    pub matched_exclusion_criteria: Vec<String>,
    pub suggested_tags: Vec<String>,
    pub confidence: f64,
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
    pub async fn run_sync(
        &self,
        conn_mutex: &std::sync::Mutex<Connection>,
        config: crate::models::llm_config::LlmConfig,
        criteria: Vec<Criterion>,
        aims: Vec<ResearchAim>,
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

            // Update current article title (show first in batch)
            {
                let mut progress = self.progress.lock().await;
                progress.current_article_title = Some(batch[0].title.clone());
            }

            // Build prompt with batch of articles
            let article_entries: Vec<ArticleEntry> = batch
                .iter()
                .map(|a| ArticleEntry {
                    title: a.title.clone(),
                    authors: a.authors.join("; "),
                    year: a.publication_year,
                    abstract_text: a.abstract_text.clone(),
                })
                .collect();

            let prompt_input = ScreeningPromptInput {
                aims: aim_entries.clone(),
                inclusion_criteria: inc_entries.clone(),
                exclusion_criteria: exc_entries.clone(),
                articles: article_entries,
            };

            let user_prompt = prompt::build_screening_prompt(&prompt_input);
            let system_prompt = prompt::SYSTEM_PROMPT;

            // Send to LLM with retry on 429
            let mut response_data = None;
            let mut retry_count: u32 = 0;

            while retry_count <= max_retries {
                match client::send_chat_completion(&config, system_prompt, &user_prompt).await {
                    Ok((text, tokens)) => {
                        response_data = Some((text, tokens));
                        break;
                    }
                    Err(e) if e.to_string().contains("429") => {
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
                            set_screening_error(&c, &article.id, &e.to_string())?;
                        }
                        break;
                    }
                }
            }

            // Apply delay between requests
            sleep(Duration::from_millis(config.request_delay_ms as u64)).await;

            let (response_text, total_tokens) = match response_data {
                Some(data) => data,
                None => {
                    let mut progress = self.progress.lock().await;
                    progress.errors += batch.len();
                    progress.completed += batch.len();
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
                                )?;
                            }
                        }
                        let mut progress = self.progress.lock().await;
                        progress.errors += batch.len();
                        progress.completed += batch.len();
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
                                set_screening_error(&c, &article.id, &screening.reasoning)?;
                            }
                            let mut progress = self.progress.lock().await;
                            progress.errors += 1;
                            progress.completed += 1;
                            continue;
                        }

                        // Apply priority resolution
                        let inc_matches: Vec<CriterionMatch> = screening
                            .matched_inclusion_criteria
                            .iter()
                            .filter_map(|id| criteria.iter().find(|c| c.id == *id))
                            .map(|c| CriterionMatch {
                                id: c.id.clone(),
                                criterion_type: c.criterion_type.clone(),
                                priority: c.priority,
                            })
                            .collect();

                        let exc_matches: Vec<CriterionMatch> = screening
                            .matched_exclusion_criteria
                            .iter()
                            .filter_map(|id| criteria.iter().find(|c| c.id == *id))
                            .map(|c| CriterionMatch {
                                id: c.id.clone(),
                                criterion_type: c.criterion_type.clone(),
                                priority: c.priority,
                            })
                            .collect();

                        let resolution_input = resolution::ScreeningInput {
                            inclusion_matches: inc_matches,
                            exclusion_matches: exc_matches,
                        };
                        let final_decision = resolution::resolve_decision(&resolution_input);

                        // Check for override
                        let ai_decision_str = screening.decision.as_str();
                        let mut reasoning = screening.reasoning.clone();
                        if ai_decision_str != final_decision {
                            reasoning.push_str(&format!(
                                "\n\n[App override: {} favored due to priority resolution]",
                                if final_decision == "include" { "inclusion" } else { "exclusion" }
                            ));
                        }

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
                                    matched_inc: &screening.matched_inclusion_criteria,
                                    matched_exc: &screening.matched_exclusion_criteria,
                                    actual_tokens: Some(tokens_per_article),
                                },
                            )?;

                            // Create/update tags from suggested_tags
                            for tag_name in &screening.suggested_tags {
                                let _ = create_or_match_tag(&c, tag_name, &article.id);
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

                    // Update elapsed time after entire batch
                    let mut progress = self.progress.lock().await;
                    let elapsed = start.elapsed().as_millis() as u64;
                    progress.elapsed_ms = elapsed;
                    if progress.completed > 0 {
                        let avg_per_article = elapsed / progress.completed as u64;
                        let remaining = (total - progress.completed) as u64;
                        progress.estimated_remaining_ms = Some(avg_per_article * remaining);
                    }
                }
                Err(_) => {
                    {
                        let c = conn_mutex.lock().map_err(|e2| {
                            AppError::Database(rusqlite::Error::InvalidParameterName(
                                e2.to_string(),
                            ))
                        })?;
                        for article in &batch {
                            set_screening_error(&c, &article.id, &response_text)?;
                        }
                    }
                    let mut progress = self.progress.lock().await;
                    progress.errors += batch.len();
                    progress.completed += batch.len();
                }
            }
        }

        // Mark as done
        {
            let mut progress = self.progress.lock().await;
            progress.is_running = false;
            progress.current_article_title = None;
        }

        Ok(())
    }
}

/// Parse the LLM response as a JSON array of screening results.
fn process_screening_responses(raw: &str) -> Result<Vec<LlmScreeningResponse>, AppError> {
    let json_str = extract_json(raw);
    serde_json::from_str::<Vec<LlmScreeningResponse>>(&json_str)
        .map_err(|e| AppError::Import(format!("Malformed LLM response: {e}")))
}

fn extract_json(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("```") {
        let without_start = trimmed.trim_start_matches("```json").trim_start_matches("```");
        let without_end = without_start.trim_end_matches("```");
        return without_end.trim().to_string();
    }
    trimmed.to_string()
}

fn set_screening_error(
    conn: &Connection,
    article_id: &str,
    raw_response: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET screening_error = 1 WHERE id = ?1",
        rusqlite::params![article_id],
    )?;

    let audit_id = uuid::Uuid::new_v4().to_string();
    let truncated = &raw_response[..raw_response.len().min(500)];
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'ai_screen', ?3, 'ai')",
        rusqlite::params![audit_id, article_id, format!("Screening error. Raw response: {truncated}")],
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
}

fn update_article_after_screening(
    conn: &Connection,
    update: ScreeningUpdate,
) -> Result<(), AppError> {
    let new_status = if update.decision == "include" { "included" } else { "rejected" };
    let matched_inc_json = serde_json::to_string(update.matched_inc)?;
    let matched_exc_json = serde_json::to_string(update.matched_exc)?;

    conn.execute(
        "UPDATE articles SET status = ?1, ai_decision = ?2, ai_reasoning = ?3, ai_confidence = ?4, \
         matched_inclusion_criteria = ?5, matched_exclusion_criteria = ?6, screened_at = datetime('now'), \
         actual_tokens = ?7 \
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
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, action, from_status, to_status, details, source) \
         VALUES (?1, ?2, 'ai_screen', 'working', ?3, ?4, 'ai')",
        rusqlite::params![
            audit_id,
            update.article_id,
            new_status,
            format!("AI screened: {} (confidence: {:.2})", update.decision, update.confidence)
        ],
    )?;

    Ok(())
}

fn create_or_match_tag(
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
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO tags (id, name, source) VALUES (?1, ?2, 'ai_suggested')",
                rusqlite::params![id, tag_name_lower],
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
