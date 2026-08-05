use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::llm_config::LlmConfig;
use crate::summary::gap_analysis::{
    build_gap_analysis_prompt, build_gap_synthesis_prompt, BiblioContext, GapPromptInput,
    GAP_ANALYSIS_SYSTEM_PROMPT,
};
use crate::summary::prompt::{self, ScreeningData, SummaryPromptInput};

pub use crate::summary::prompt::ArticleSummary;

/// Input data extracted from DB synchronously, passed to async processing.
pub struct SummaryInput {
    pub config: LlmConfig,
    pub aim_texts: Vec<String>,
    pub articles: Vec<ArticleSummary>,
    pub screening_data: ScreeningData,
    pub citation_style: String,
    /// Full inclusion criteria (Shape 0). Threaded into each batch's prompt.
    pub inclusion_criteria: Vec<String>,
    /// Full exclusion criterion definitions (Shape 0).
    pub exclusion_criteria: Vec<String>,
}

impl SummaryInput {
    pub fn new(
        config: LlmConfig,
        aim_texts: Vec<String>,
        articles: Vec<ArticleSummary>,
        screening_data: ScreeningData,
        citation_style: String,
        inclusion_criteria: Vec<String>,
        exclusion_criteria: Vec<String>,
    ) -> Self {
        Self {
            config,
            aim_texts,
            articles,
            screening_data,
            citation_style,
            inclusion_criteria,
            exclusion_criteria,
        }
    }
}

pub async fn generate_summary(
    orchestrator: &LlmOrchestrator,
    input: SummaryInput,
) -> Result<String, AppError> {
    if input.articles.is_empty() {
        return Err(AppError::Validation("No included articles to summarize".to_string()));
    }

    // Check if batching is needed (80% of context window)
    let context_limit = (input.config.context_window_tokens as f64 * 0.8) as usize;

    /* Token heuristic: title + abstract + authors + keywords + evidence + criteria chars, /4.
    MUST see every field or batching silently underflows the context window on large projects.
    Criteria text is added once per batch (not per article). */
    let criteria_chars: usize = input
        .inclusion_criteria
        .iter()
        .chain(input.exclusion_criteria.iter())
        .map(|c| c.len() + 4)
        .sum();
    let total_chars: usize = input
        .articles
        .iter()
        .map(|a| {
            a.title.len()
                + a.abstract_text.len()
                + a.authors.join("; ").len()
                + a.keywords.join(", ").len()
                + a.evidence.as_ref().map_or(0, |e| e.len() + 12)
        })
        .sum();
    let estimated_tokens = (total_chars + criteria_chars) / 4;

    let response = if estimated_tokens > context_limit {
        // Batch: split articles into chunks, summarize each, then synthesize
        let batch_size = (input.articles.len() / 2).max(1);
        let batch_a = &input.articles[..batch_size];
        let batch_b = &input.articles[batch_size..];

        let summary_a = summarize_batch(
            orchestrator,
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &input.inclusion_criteria,
            &input.exclusion_criteria,
            batch_a,
        )
        .await?;
        let summary_b = summarize_batch(
            orchestrator,
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &input.inclusion_criteria,
            &input.exclusion_criteria,
            batch_b,
        )
        .await?;

        // Synthesize
        synthesize_batches(
            orchestrator,
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &input.inclusion_criteria,
            &input.exclusion_criteria,
            &summary_a,
            &summary_b,
        )
        .await?
    } else {
        summarize_batch(
            orchestrator,
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &input.inclusion_criteria,
            &input.exclusion_criteria,
            &input.articles,
        )
        .await?
    };

    Ok(response)
}

#[allow(clippy::too_many_arguments)]
async fn summarize_batch(
    orchestrator: &LlmOrchestrator,
    config: &LlmConfig,
    aims: &[String],
    screening: &ScreeningData,
    citation_style: &str,
    inclusion_criteria: &[String],
    exclusion_criteria: &[String],
    articles: &[ArticleSummary],
) -> Result<String, AppError> {
    let input = SummaryPromptInput {
        aims: aims.to_vec(),
        screening_data: screening.clone(),
        citation_style: citation_style.to_string(),
        articles: articles.to_vec(),
        inclusion_criteria: inclusion_criteria.to_vec(),
        exclusion_criteria: exclusion_criteria.to_vec(),
    };
    let user_prompt = prompt::build_summary_prompt(&input);
    let (response, _tokens) = orchestrator
        .send(config, prompt::SYSTEM_PROMPT, &user_prompt, LlmRequestType::SummaryGeneration)
        .await?;
    Ok(response.trim().to_string())
}

#[allow(clippy::too_many_arguments)]
async fn synthesize_batches(
    orchestrator: &LlmOrchestrator,
    config: &LlmConfig,
    aims: &[String],
    screening: &ScreeningData,
    citation_style: &str,
    inclusion_criteria: &[String],
    exclusion_criteria: &[String],
    a: &str,
    b: &str,
) -> Result<String, AppError> {
    let aims_text = aims
        .iter()
        .enumerate()
        .map(|(i, aim)| format!("{}. {}", i + 1, aim))
        .collect::<Vec<_>>()
        .join("\n");

    let screening_summary =
        prompt::format_screening_summary(screening, inclusion_criteria, exclusion_criteria);

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

    let (response, _tokens) = orchestrator
        .send(config, prompt::SYSTEM_PROMPT, &synthesis_prompt, LlmRequestType::SummaryGeneration)
        .await?;
    Ok(response.trim().to_string())
}

// ── Research Gap Analysis ──────────────────────────────────────────────────
//
// Mirrors `generate_summary`'s shape (batch + synthesize when the corpus
// exceeds 80% of the context window). Same token heuristic so the two paths
// do not diverge on the article axis.

/// Gap-analysis engine input. Mirrors `SummaryInput` + pre-rendered screening summary
/// + `BiblioContext`.
pub struct GapAnalysisInput {
    pub config: LlmConfig,
    pub aim_texts: Vec<String>,
    pub articles: Vec<ArticleSummary>,
    pub screening_data: ScreeningData,
    pub citation_style: String,
    pub inclusion_criteria: Vec<String>,
    pub exclusion_criteria: Vec<String>,
    pub biblio_context: BiblioContext,
}

impl GapAnalysisInput {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: LlmConfig,
        aim_texts: Vec<String>,
        articles: Vec<ArticleSummary>,
        screening_data: ScreeningData,
        citation_style: String,
        inclusion_criteria: Vec<String>,
        exclusion_criteria: Vec<String>,
        biblio_context: BiblioContext,
    ) -> Self {
        Self {
            config,
            aim_texts,
            articles,
            screening_data,
            citation_style,
            inclusion_criteria,
            exclusion_criteria,
            biblio_context,
        }
    }
}

/// Generate Research Gap Analysis report. Returns Markdown. Batches when estimated
/// tokens >80% context window (mirrors `generate_summary`).
pub async fn generate_gap_analysis(
    orchestrator: &LlmOrchestrator,
    input: GapAnalysisInput,
) -> Result<String, AppError> {
    if input.articles.is_empty() {
        return Err(AppError::Validation("No included articles to analyze".to_string()));
    }

    let context_limit = (input.config.context_window_tokens as f64 * 0.8) as usize;

    /* Same heuristic as `generate_summary`: title + abstract + authors + keywords +
    evidence + criteria chars, /4. MUST see every field or batching silently underflows. */
    let criteria_chars: usize = input
        .inclusion_criteria
        .iter()
        .chain(input.exclusion_criteria.iter())
        .map(|c| c.len() + 4)
        .sum();
    let total_chars: usize = input
        .articles
        .iter()
        .map(|a| {
            a.title.len()
                + a.abstract_text.len()
                + a.authors.join("; ").len()
                + a.keywords.join(", ").len()
                + a.evidence.as_ref().map_or(0, |e| e.len() + 12)
        })
        .sum();
    let estimated_tokens = (total_chars + criteria_chars) / 4;

    let response = if estimated_tokens > context_limit {
        // Batch: split articles in half, analyze each, then synthesize the two
        // partial gap reports into one coherent document.
        let batch_size = (input.articles.len() / 2).max(1);
        let batch_a = &input.articles[..batch_size];
        let batch_b = &input.articles[batch_size..];

        let gap_a = gap_batch(
            orchestrator,
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &input.inclusion_criteria,
            &input.exclusion_criteria,
            &input.biblio_context,
            batch_a,
        )
        .await?;
        // If the second batch is empty (only one article), skip the second call
        // and the synthesis; return the first partial directly.
        if batch_b.is_empty() {
            return Ok(gap_a);
        }
        let gap_b = gap_batch(
            orchestrator,
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &input.inclusion_criteria,
            &input.exclusion_criteria,
            &input.biblio_context,
            batch_b,
        )
        .await?;

        // Synthesize the two partial gap reports into one coherent document.
        let synthesis_prompt =
            build_gap_synthesis_prompt(&input.aim_texts, &input.citation_style, &gap_a, &gap_b);
        let (response, _tokens) = orchestrator
            .send(
                &input.config,
                GAP_ANALYSIS_SYSTEM_PROMPT,
                &synthesis_prompt,
                LlmRequestType::GapAnalysis,
            )
            .await?;
        response.trim().to_string()
    } else {
        gap_batch(
            orchestrator,
            &input.config,
            &input.aim_texts,
            &input.screening_data,
            &input.citation_style,
            &input.inclusion_criteria,
            &input.exclusion_criteria,
            &input.biblio_context,
            &input.articles,
        )
        .await?
    };

    Ok(response)
}

#[allow(clippy::too_many_arguments)]
async fn gap_batch(
    orchestrator: &LlmOrchestrator,
    config: &LlmConfig,
    aims: &[String],
    screening: &ScreeningData,
    citation_style: &str,
    inclusion_criteria: &[String],
    exclusion_criteria: &[String],
    biblio_context: &BiblioContext,
    articles: &[ArticleSummary],
) -> Result<String, AppError> {
    /* Pre-render screening summary once per batch. Reuses `format_screening_summary`
    so the gap prompt and the literature-review prompt stay consistent. */
    let screening_summary =
        prompt::format_screening_summary(screening, inclusion_criteria, exclusion_criteria);

    let input = GapPromptInput {
        aims: aims.to_vec(),
        screening_summary,
        citation_style: citation_style.to_string(),
        articles: articles.to_vec(),
        biblio_context: biblio_context.clone(),
        inclusion_criteria: inclusion_criteria.to_vec(),
        exclusion_criteria: exclusion_criteria.to_vec(),
    };
    let user_prompt = build_gap_analysis_prompt(&input);
    let (response, _tokens) = orchestrator
        .send(config, GAP_ANALYSIS_SYSTEM_PROMPT, &user_prompt, LlmRequestType::GapAnalysis)
        .await?;
    Ok(response.trim().to_string())
}
