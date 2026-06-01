use crate::error::AppError;
use crate::llm::client;
use crate::models::llm_config::LlmConfig;
use crate::summary::prompt::{self, ScreeningData, SummaryPromptInput};

pub use crate::summary::prompt::ArticleSummary;

/// Input data extracted from DB synchronously, passed to async processing.
pub struct SummaryInput {
    pub config: LlmConfig,
    pub aim_texts: Vec<String>,
    pub articles: Vec<ArticleSummary>,
    pub screening_data: ScreeningData,
    pub citation_style: String,
}

impl SummaryInput {
    pub fn new(
        config: LlmConfig,
        aim_texts: Vec<String>,
        articles: Vec<ArticleSummary>,
        screening_data: ScreeningData,
        citation_style: String,
    ) -> Self {
        Self { config, aim_texts, articles, screening_data, citation_style }
    }
}

pub async fn generate_summary(input: SummaryInput) -> Result<String, AppError> {
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
                + a.keywords.join(", ").len()
        })
        .sum();
    let estimated_tokens = total_chars / 4;

    let response = if estimated_tokens > context_limit {
        // Batch: split articles into chunks, summarize each, then synthesize
        let batch_size = (input.articles.len() / 2).max(1);
        let batch_a = &input.articles[..batch_size];
        let batch_b = &input.articles[batch_size..];

        let summary_a = summarize_batch(
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            batch_a,
        )
        .await?;
        let summary_b = summarize_batch(
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            batch_b,
        )
        .await?;

        // Synthesize
        synthesize_batches(
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &summary_a,
            &summary_b,
        )
        .await?
    } else {
        summarize_batch(
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &input.articles,
        )
        .await?
    };

    Ok(response)
}

async fn summarize_batch(
    config: &LlmConfig,
    aims: &[String],
    screening: &ScreeningData,
    citation_style: &str,
    articles: &[ArticleSummary],
) -> Result<String, AppError> {
    let input = SummaryPromptInput {
        aims: aims.to_vec(),
        screening_data: screening.clone(),
        citation_style: citation_style.to_string(),
        articles: articles.to_vec(),
    };
    let user_prompt = prompt::build_summary_prompt(&input);
    let (response, _tokens) =
        client::send_chat_completion(config, prompt::SYSTEM_PROMPT, &user_prompt).await?;
    Ok(response.trim().to_string())
}

async fn synthesize_batches(
    config: &LlmConfig,
    aims: &[String],
    screening: &ScreeningData,
    citation_style: &str,
    a: &str,
    b: &str,
) -> Result<String, AppError> {
    let aims_text = aims
        .iter()
        .enumerate()
        .map(|(i, aim)| format!("{}. {}", i + 1, aim))
        .collect::<Vec<_>>()
        .join("\n");

    let screening_summary = prompt::format_screening_summary(screening);

    let synthesis_prompt = format!(
        r#"## Task
Combine two partial literature reviews into a single coherent review. Maintain focus on the research aims.
Use {citation_style} citation style throughout. Do NOT invent references that do not appear in either section.

## Research Aims
{aims}

## Search Methodology Summary
{screening}

## Citation Style
{citation_style}

## Partial Review A
{a}

## Partial Review B
{b}

## Instructions
Produce a single unified literature review with these sections:
- Title
- Abstract
- Introduction
- Methodology
- Results
- Discussion
- Conclusion
- References

## Writing Style Rules
- Do NOT use em dashes anywhere. Use commas, parentheses, or split into separate sentences instead.
- Write in formal academic prose with varied sentence lengths. Mix shorter declarative sentences with longer complex ones to reflect natural academic writing.

Return only the plain text of the literature review. Do not wrap it in code fences."#,
        citation_style = citation_style,
        aims = aims_text,
        screening = screening_summary,
        a = a,
        b = b,
    );

    let (response, _tokens) =
        client::send_chat_completion(config, prompt::SYSTEM_PROMPT, &synthesis_prompt).await?;
    Ok(response.trim().to_string())
}
