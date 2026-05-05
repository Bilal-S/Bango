#[derive(Clone)]
pub struct ArticleSummary {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub abstract_text: String,
    pub ai_reasoning: Option<String>,
}

pub struct SummaryPromptInput {
    pub aims: Vec<String>,
    pub target_length: usize,
    pub articles: Vec<ArticleSummary>,
}

pub const SYSTEM_PROMPT: &str = "You are a systematic literature review assistant. Generate a structured summary of the included articles in a systematic review.";

#[must_use]
pub fn build_summary_prompt(input: &SummaryPromptInput) -> String {
    let aims_list = if input.aims.is_empty() {
        "None defined.".to_string()
    } else {
        input
            .aims
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{}. {}", i + 1, a))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let articles_text = input
        .articles
        .iter()
        .map(|a| {
            let year_str = a.year.map(|y| y.to_string()).unwrap_or_else(|| "Unknown".to_string());
            let reasoning = a
                .ai_reasoning
                .as_ref()
                .map(|r| format!("\nAI Reasoning: {}", r))
                .unwrap_or_default();
            format!(
                "---\nTitle: {}\nAuthors: {}\nYear: {}\nAbstract: {}{}\n---",
                a.title,
                a.authors.join("; "),
                year_str,
                a.abstract_text,
                reasoning
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"## Task
Generate a structured summary of the included articles in a systematic literature review. Focus on the research aims provided.

## Research Aims
{aims}

## Target Length
Approximately {target_length} words.

## Included Articles
{articles}

## Response Format
Return JSON exactly matching this schema:
{{
  "key_themes": "A paragraph describing the main topics and findings across included studies.",
  "research_trends": "A paragraph describing patterns and directions in the literature vis-a-vis the research aims.",
  "methodological_strengths": "A paragraph describing common robust methodologies observed.",
  "common_weaknesses": "A paragraph describing limitations frequently cited across studies.",
  "gaps_in_literature": "A paragraph describing under-explored areas relative to the research aims."
}}"#,
        aims = aims_list,
        target_length = input.target_length,
        articles = articles_text,
    )
}
