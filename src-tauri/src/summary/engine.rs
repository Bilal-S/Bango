use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::llm::client;
use crate::models::llm_config::LlmConfig;
use crate::summary::prompt::{self, SummaryPromptInput};

pub use crate::summary::prompt::ArticleSummary;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryOutput {
    pub key_themes: String,
    pub research_trends: String,
    pub methodological_strengths: String,
    pub common_weaknesses: String,
    pub gaps_in_literature: String,
}

/// Input data extracted from DB synchronously, passed to async processing.
pub struct SummaryInput {
    pub config: LlmConfig,
    pub aim_texts: Vec<String>,
    pub articles: Vec<ArticleSummary>,
    pub target_length: usize,
}

impl SummaryInput {
    pub fn new(
        config: LlmConfig,
        aim_texts: Vec<String>,
        articles: Vec<ArticleSummary>,
        target_length: usize,
    ) -> Self {
        Self { config, aim_texts, articles, target_length }
    }
}

pub async fn generate_summary(input: SummaryInput) -> Result<SummaryOutput, AppError> {
    if input.articles.is_empty() {
        return Err(AppError::Validation("No included articles to summarize".to_string()));
    }

    // Check if batching is needed (80% of context window)
    let context_limit = (input.config.context_window_tokens as f64 * 0.8) as usize;

    // Simple heuristic: estimate tokens for all articles combined
    let total_chars: usize = input
        .articles
        .iter()
        .map(|a| {
            a.title.len()
                + a.abstract_text.len()
                + a.authors.join("; ").len()
                + a.ai_reasoning.as_ref().map(|r| r.len()).unwrap_or(0)
        })
        .sum();
    let estimated_tokens = total_chars / 4;

    let response = if estimated_tokens > context_limit {
        // Batch: split articles into chunks, summarize each, then synthesize
        let batch_size = (input.articles.len() / 2).max(1);
        let batch_a = &input.articles[..batch_size];
        let batch_b = &input.articles[batch_size..];

        let summary_a =
            summarize_batch(&input.config, &input.aim_texts, input.target_length / 2, batch_a)
                .await?;
        let summary_b =
            summarize_batch(&input.config, &input.aim_texts, input.target_length / 2, batch_b)
                .await?;

        // Synthesize
        synthesize_batches(
            &input.config,
            &input.aim_texts,
            input.target_length,
            &summary_a,
            &summary_b,
        )
        .await?
    } else {
        summarize_batch(&input.config, &input.aim_texts, input.target_length, &input.articles)
            .await?
    };

    Ok(response)
}

async fn summarize_batch(
    config: &LlmConfig,
    aims: &[String],
    target_length: usize,
    articles: &[ArticleSummary],
) -> Result<SummaryOutput, AppError> {
    let input =
        SummaryPromptInput { aims: aims.to_vec(), target_length, articles: articles.to_vec() };
    let user_prompt = prompt::build_summary_prompt(&input);
    let response =
        client::send_chat_completion(config, prompt::SYSTEM_PROMPT, &user_prompt).await?;
    parse_summary_response(&response)
}

async fn synthesize_batches(
    config: &LlmConfig,
    aims: &[String],
    target_length: usize,
    a: &SummaryOutput,
    b: &SummaryOutput,
) -> Result<SummaryOutput, AppError> {
    let synthesis_prompt = format!(
        r#"## Task
Combine two partial summaries into a single coherent summary. Maintain focus on the research aims.

## Research Aims
{aims}

## Target Length
Approximately {target_length} words.

## Partial Summary A
Key Themes: {a_themes}
Research Trends: {a_trends}
Methodological Strengths: {a_methods}
Common Weaknesses: {a_weaknesses}
Gaps in Literature: {a_gaps}

## Partial Summary B
Key Themes: {b_themes}
Research Trends: {b_trends}
Methodological Strengths: {b_methods}
Common Weaknesses: {b_weaknesses}
Gaps in Literature: {b_gaps}

## Response Format
Return JSON exactly matching this schema:
{{
  "key_themes": "...",
  "research_trends": "...",
  "methodological_strengths": "...",
  "common_weaknesses": "...",
  "gaps_in_literature": "..."
}}"#,
        aims = aims
            .iter()
            .enumerate()
            .map(|(i, aim)| format!("{}. {}", i + 1, aim))
            .collect::<Vec<_>>()
            .join("\n"),
        target_length = target_length,
        a_themes = a.key_themes,
        a_trends = a.research_trends,
        a_methods = a.methodological_strengths,
        a_weaknesses = a.common_weaknesses,
        a_gaps = a.gaps_in_literature,
        b_themes = b.key_themes,
        b_trends = b.research_trends,
        b_methods = b.methodological_strengths,
        b_weaknesses = b.common_weaknesses,
        b_gaps = b.gaps_in_literature,
    );

    let response =
        client::send_chat_completion(config, prompt::SYSTEM_PROMPT, &synthesis_prompt).await?;
    parse_summary_response(&response)
}

fn parse_summary_response(raw: &str) -> Result<SummaryOutput, AppError> {
    let json_str = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str::<SummaryOutput>(json_str)
        .map_err(|e| AppError::Import(format!("Failed to parse summary response: {}", e)))
}
