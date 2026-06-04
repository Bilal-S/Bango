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

use crate::db::{article_repo, audit_repo, label_repo, tag_repo};
use crate::error::AppError;
use crate::models::criterion::{Criterion, CriterionType, ResearchAim};
use crate::screening::llm_client::LlmClient;
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
    pub current_article_titles: Vec<String>,
    pub elapsed_ms: u64,
    pub estimated_remaining_ms: Option<u64>,
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
    pub async fn run_sync(
        &self,
        conn_mutex: &std::sync::Mutex<Connection>,
        llm: &dyn LlmClient,
        request_delay_ms: u64,
        criteria: Vec<Criterion>,
        aims: Vec<ResearchAim>,
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
                existing_tags: existing_tag_names.clone(),
                existing_labels: existing_label_names.clone(),
            };

            let user_prompt = prompt::build_screening_prompt(&prompt_input);
            let system_prompt = prompt::SYSTEM_PROMPT;

            // Send to LLM with retry on 429
            let mut response_data = None;
            let mut retry_count: u32 = 0;

            while retry_count <= max_retries {
                match llm.send(system_prompt, &user_prompt).await {
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

                        // Keep raw reasoning with UUIDs — frontend replaces dynamically at display time
                        let mut reasoning = screening.reasoning.clone();

                        // Check for override
                        let ai_decision_str = screening.decision.as_str();
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
                                    matched_inc: &augmented_inc,
                                    matched_exc: &augmented_exc,
                                    actual_tokens: Some(tokens_per_article),
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
fn process_screening_responses(raw: &str) -> Result<Vec<LlmScreeningResponse>, AppError> {
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
fn repair_truncated_json_array(json: &str) -> Option<String> {
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

fn extract_json(raw: &str) -> String {
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
        return inner.to_string();
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

    trimmed.to_string()
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
         matched_inclusion_criteria = ?5, matched_exclusion_criteria = ?6, screened_at = datetime('now'), changed_at = datetime('now'), \
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_json tests ──

    #[test]
    fn test_extract_json_plain_array() {
        let input = r#"[{"decision":"include","reasoning":"ok","matched_inclusion_criteria":[],"matched_exclusion_criteria":[],"suggested_tags":[],"confidence":0.9}]"#;
        assert_eq!(extract_json(input), input.trim());
    }

    #[test]
    fn test_extract_json_json_code_fence() {
        let inner = r#"[{"decision":"include"}]"#;
        let input = format!("```json\n{inner}\n```");
        assert_eq!(extract_json(&input), inner);
    }

    #[test]
    fn test_extract_json_plain_code_fence() {
        let inner = r#"[{"decision":"include"}]"#;
        let input = format!("```\n{inner}\n```");
        assert_eq!(extract_json(&input), inner);
    }

    #[test]
    fn test_extract_json_whitespace() {
        let inner = r#"[{"decision":"include"}]"#;
        let input = format!("  \n{inner}\n  ");
        assert_eq!(extract_json(&input), inner);
    }

    #[test]
    fn test_extract_json_empty_string() {
        assert_eq!(extract_json(""), "");
    }

    // ── process_screening_responses tests ──

    #[test]
    fn test_parse_single_response() {
        let raw = r#"[
            {
                "decision": "include",
                "reasoning": "Meets inclusion criteria.",
                "matchedInclusionCriteria": ["c1"],
                "matchedExclusionCriteria": [],
                "suggestedTags": ["ml"],
                "confidence": 0.92
            }
        ]"#;
        let results = process_screening_responses(raw).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].decision, "include");
        assert_eq!(results[0].matched_inclusion_criteria, vec!["c1"]);
        assert_eq!(results[0].confidence, 0.92);
    }

    #[test]
    fn test_parse_batch_of_three() {
        let raw = r#"[
            {
                "decision": "include",
                "reasoning": "R1",
                "matchedInclusionCriteria": ["c1"],
                "matchedExclusionCriteria": [],
                "suggestedTags": ["ml"],
                "confidence": 0.9
            },
            {
                "decision": "exclude",
                "reasoning": "R2",
                "matchedInclusionCriteria": [],
                "matchedExclusionCriteria": ["c2"],
                "suggestedTags": [],
                "confidence": 0.85
            },
            {
                "decision": "include",
                "reasoning": "R3",
                "matchedInclusionCriteria": ["c1", "c3"],
                "matchedExclusionCriteria": ["c2"],
                "suggestedTags": ["dl", "medical"],
                "confidence": 0.75
            }
        ]"#;
        let results = process_screening_responses(raw).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].decision, "include");
        assert_eq!(results[1].decision, "exclude");
        assert_eq!(results[2].decision, "include");
        assert_eq!(results[2].matched_inclusion_criteria, vec!["c1", "c3"]);
        assert_eq!(results[2].suggested_tags, vec!["dl", "medical"]);
    }

    #[test]
    fn test_parse_batch_of_fifteen() {
        let items: Vec<String> = (0..15)
            .map(|i| {
                format!(
                    r#"{{"decision":"{}","reasoning":"Article {}","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.{:02}}}"#,
                    if i % 2 == 0 { "include" } else { "exclude" },
                    i,
                    50 + i
                )
            })
            .collect();
        let raw = format!("[{}]", items.join(","));
        let results = process_screening_responses(&raw).unwrap();
        assert_eq!(results.len(), 15);
        assert_eq!(results[0].decision, "include");
        assert_eq!(results[1].decision, "exclude");
        assert_eq!(results[14].decision, "include"); // 14 % 2 == 0 → include
    }

    #[test]
    fn test_parse_error_decision() {
        let raw = r#"[{
            "decision": "error",
            "reasoning": "Abstract too short to evaluate",
            "matchedInclusionCriteria": [],
            "matchedExclusionCriteria": [],
            "suggestedTags": [],
            "confidence": 0.0
        }]"#;
        let results = process_screening_responses(raw).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].decision, "error");
        assert_eq!(results[0].reasoning, "Abstract too short to evaluate");
    }

    #[test]
    fn test_parse_response_with_all_fields_populated() {
        let raw = r#"[{
            "decision": "include",
            "reasoning": "Meets criteria c1 and c3.",
            "matchedInclusionCriteria": ["c1", "c3"],
            "matchedExclusionCriteria": ["c2"],
            "suggestedTags": ["machine-learning", "healthcare", "systematic-review"],
            "confidence": 0.95
        }]"#;
        let results = process_screening_responses(raw).unwrap();
        assert_eq!(results[0].matched_inclusion_criteria, vec!["c1", "c3"]);
        assert_eq!(results[0].matched_exclusion_criteria, vec!["c2"]);
        assert_eq!(
            results[0].suggested_tags,
            vec!["machine-learning", "healthcare", "systematic-review"]
        );
    }

    #[test]
    fn test_parse_response_with_empty_arrays() {
        let raw = r#"[{
            "decision": "exclude",
            "reasoning": "No criteria matched",
            "matchedInclusionCriteria": [],
            "matchedExclusionCriteria": [],
            "suggestedTags": [],
            "confidence": 0.3
        }]"#;
        let results = process_screening_responses(raw).unwrap();
        assert!(results[0].matched_inclusion_criteria.is_empty());
        assert!(results[0].matched_exclusion_criteria.is_empty());
        assert!(results[0].suggested_tags.is_empty());
    }

    #[test]
    fn test_parse_response_wrapped_in_code_fence() {
        let inner = r#"[{"decision":"include","reasoning":"ok","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}]"#;
        let raw = format!("```json\n{inner}\n```");
        let results = process_screening_responses(&raw).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].decision, "include");
    }

    #[test]
    fn test_parse_response_with_surrounding_text() {
        // LLM sometimes wraps JSON in explanatory text - extract_json now handles this
        let raw = r#"Here are the screening results:
    [
    {"decision":"include","reasoning":"ok","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}
]
Hope this helps!"#;
        // extract_json finds the embedded JSON array even with surrounding text
        let result = process_screening_responses(raw);
        assert!(result.is_ok(), "Should extract JSON from surrounding text");
        let responses = result.unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].decision, "include");
    }

    #[test]
    fn test_parse_invalid_json_returns_error() {
        let raw = "this is not json";
        let result = process_screening_responses(raw);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Malformed LLM response"), "Got: {err_msg}");
    }

    #[test]
    fn test_parse_json_object_instead_of_array_returns_error() {
        let raw = r#"{"decision":"include","reasoning":"ok","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}"#;
        let result = process_screening_responses(raw);
        assert!(result.is_err(), "Single object should fail - engine expects array");
    }

    #[test]
    fn test_parse_missing_required_field_returns_error() {
        let raw = r#"[{"decision":"include"}]"#;
        let result = process_screening_responses(raw);
        assert!(result.is_err(), "Missing fields should fail deserialization");
    }

    #[test]
    fn test_parse_extra_unknown_fields_ignored() {
        let raw = r#"[{
            "decision": "include",
            "reasoning": "ok",
            "matchedInclusionCriteria": [],
            "matchedExclusionCriteria": [],
            "suggestedTags": [],
            "confidence": 0.9,
            "extra_field": "should be ignored"
        }]"#;
        let results = process_screening_responses(raw).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_parse_snake_case_field_names() {
        let raw = r#"[
            {
                "decision": "include",
                "reasoning": "Meets criteria.",
                "matched_inclusion_criteria": ["c1"],
                "matched_exclusion_criteria": [],
                "suggested_tags": ["ml"],
                "confidence": 0.9
            }
        ]"#;
        let results = process_screening_responses(raw).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_inclusion_criteria, vec!["c1"]);
    }

    #[test]
    fn test_parse_missing_optional_fields_default() {
        // decision + reasoning are required; everything else defaults
        let raw = r#"[{"decision":"include","reasoning":"ok"}]"#;
        let results = process_screening_responses(raw).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].matched_inclusion_criteria.is_empty());
        assert!(results[0].matched_exclusion_criteria.is_empty());
        assert!(results[0].suggested_tags.is_empty());
        assert_eq!(results[0].confidence, 0.0);
    }

    #[test]
    fn test_parse_empty_array() {
        let raw = "[]";
        let results = process_screening_responses(raw).unwrap();
        assert!(results.is_empty());
    }

    // ── ScreeningEngine::with_batch_size tests ──

    #[test]
    fn test_with_batch_size_zero_clamps_to_one() {
        let engine = ScreeningEngine::with_batch_size(0);
        assert_eq!(engine.batch_size, 1);
    }

    #[test]
    fn test_with_batch_size_one_stays() {
        let engine = ScreeningEngine::with_batch_size(1);
        assert_eq!(engine.batch_size, 1);
    }

    #[test]
    fn test_with_batch_size_five() {
        let engine = ScreeningEngine::with_batch_size(5);
        assert_eq!(engine.batch_size, 5);
    }

    #[test]
    fn test_with_batch_size_fifteen() {
        let engine = ScreeningEngine::with_batch_size(15);
        assert_eq!(engine.batch_size, 15);
    }

    #[test]
    fn test_default_batch_size_is_one() {
        let engine = ScreeningEngine::new();
        assert_eq!(engine.batch_size, 1);
    }

    // ── Response count mismatch validation ──
    // These simulate the engine's count check without needing a real DB.

    #[test]
    fn test_response_count_mismatch_detected() {
        // Simulate: 3 articles fetched, but LLM returns 2 results
        let raw = r#"[
            {"decision":"include","reasoning":"R1","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9},
            {"decision":"exclude","reasoning":"R2","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.8}
        ]"#;
        let results = process_screening_responses(raw).unwrap();
        let batch_len = 3;
        assert_ne!(results.len(), batch_len, "Should detect mismatch: 2 results for 3 articles");
    }

    #[test]
    fn test_response_count_matches_batch() {
        let raw = r#"[
            {"decision":"include","reasoning":"R1","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9},
            {"decision":"exclude","reasoning":"R2","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.8},
            {"decision":"include","reasoning":"R3","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.7}
        ]"#;
        let results = process_screening_responses(raw).unwrap();
        let batch_len = 3;
        assert_eq!(results.len(), batch_len, "Count should match");
    }

    #[test]
    fn test_response_more_results_than_articles() {
        let raw = r#"[
            {"decision":"include","reasoning":"R1","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9},
            {"decision":"exclude","reasoning":"R2","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.8}
        ]"#;
        let results = process_screening_responses(raw).unwrap();
        let batch_len = 1;
        assert_ne!(results.len(), batch_len, "Should detect: 2 results for 1 article");
    }

    // ── create_or_match_tag / create_or_match_label tests ──

    fn setup_test_db() -> Connection {
        let conn = crate::db::connection::create_connection().expect("DB connection failed");
        crate::db::migration::run_migrations(&conn).expect("Migration failed");
        conn
    }

    fn insert_test_article(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
             VALUES (?1, 'Test Article', 'Author', 'Abstract text', 'working', 'test.ris')",
            rusqlite::params![id],
        )
        .expect("Insert article failed");
    }

    #[test]
    fn test_tag_matches_existing_case_insensitive() {
        let conn = setup_test_db();
        let article_id = "art-tag-match";
        insert_test_article(&conn, article_id);

        // Pre-create a tag
        conn.execute(
            "INSERT INTO tags (id, name, source) VALUES ('t1', 'machine-learning', 'user_created')",
            [],
        )
        .unwrap();

        // LLM suggests same tag with different case
        create_or_match_tag(&conn, "Machine-Learning", article_id).unwrap();

        // Should NOT create a new tag — still only 1
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "Should reuse existing tag, not create a new one");

        // Article should be linked to the existing tag
        let linked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM article_tags WHERE article_id = ?1 AND tag_id = 't1'",
                rusqlite::params![article_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(linked, 1);
    }

    #[test]
    fn test_tag_creates_new_when_no_match() {
        let conn = setup_test_db();
        let article_id = "art-tag-new";
        insert_test_article(&conn, article_id);

        create_or_match_tag(&conn, "deep-learning", article_id).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "deep-learning");

        let linked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM article_tags WHERE article_id = ?1",
                rusqlite::params![article_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(linked, 1);
    }

    #[test]
    fn test_tag_trimmed_to_30_chars() {
        let conn = setup_test_db();
        let article_id = "art-tag-trim";
        insert_test_article(&conn, article_id);

        let long_tag = "this-is-a-very-long-tag-name-that-exceeds-thirty-chars";
        create_or_match_tag(&conn, long_tag, article_id).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name.len(), 30, "New tag should be trimmed to 30 chars");
        assert_eq!(name, "this-is-a-very-long-tag-name-t");
    }

    #[test]
    fn test_tag_exactly_30_chars_not_trimmed() {
        let conn = setup_test_db();
        let article_id = "art-tag-exact";
        insert_test_article(&conn, article_id);

        let exact_tag = "123456789012345678901234567890"; // exactly 30 chars
        assert_eq!(exact_tag.len(), 30);
        create_or_match_tag(&conn, exact_tag, article_id).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, exact_tag);
    }

    #[test]
    fn test_tag_short_name_unchanged() {
        let conn = setup_test_db();
        let article_id = "art-tag-short";
        insert_test_article(&conn, article_id);

        create_or_match_tag(&conn, "ml", article_id).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "ml");
    }

    #[test]
    fn test_label_matches_existing_case_insensitive() {
        let conn = setup_test_db();
        let article_id = "art-label-match";
        insert_test_article(&conn, article_id);

        conn.execute(
            "INSERT INTO labels (id, name, source) VALUES ('l1', 'priority-read', 'user_created')",
            [],
        )
        .unwrap();

        create_or_match_label(&conn, "Priority-Read", article_id).unwrap();

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM labels", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "Should reuse existing label, not create a new one");

        let linked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM article_labels WHERE article_id = ?1 AND label_id = 'l1'",
                rusqlite::params![article_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(linked, 1);
    }

    #[test]
    fn test_label_creates_new_when_no_match() {
        let conn = setup_test_db();
        let article_id = "art-label-new";
        insert_test_article(&conn, article_id);

        create_or_match_label(&conn, "strong-methodology", article_id).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM labels WHERE source = 'ai_generated'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "strong-methodology");
    }

    #[test]
    fn test_label_trimmed_to_30_chars() {
        let conn = setup_test_db();
        let article_id = "art-label-trim";
        insert_test_article(&conn, article_id);

        let long_label = "Inclusion: this is a very long criterion text that exceeds limit";
        create_or_match_label(&conn, long_label, article_id).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM labels WHERE source = 'ai_generated'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name.len(), 30, "New label should be trimmed to 30 chars");
    }
}

/// Maximum character length for newly created tags or labels that don't match existing ones.
const MAX_NEW_TAG_LABEL_LEN: usize = 30;

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

fn create_or_match_label(
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
fn build_global_criterion_numbering(
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
fn augment_matched_from_reasoning(
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
