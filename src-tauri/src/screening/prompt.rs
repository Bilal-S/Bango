use crate::models::criterion::Priority;

pub const SYSTEM_PROMPT: &str = "You are a systematic literature review screening assistant. Evaluate the provided article abstract against the research aims, inclusion criteria, and exclusion criteria. Return your evaluation as structured JSON matching the required schema.";

pub struct AimEntry {
    pub text: String,
}

pub struct CriterionEntry {
    pub id: String,
    pub text: String,
    pub priority: Priority,
}

pub struct ScreeningPromptInput {
    pub aims: Vec<AimEntry>,
    pub inclusion_criteria: Vec<CriterionEntry>,
    pub exclusion_criteria: Vec<CriterionEntry>,
    pub article_title: String,
    pub article_authors: String,
    pub article_year: Option<i32>,
    pub article_abstract: String,
}

pub fn build_screening_prompt(input: &ScreeningPromptInput) -> String {
    let aims_list = if input.aims.is_empty() {
        "None defined.".to_string()
    } else {
        input
            .aims
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{}. {}", i + 1, a.text))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let inclusion_list = if input.inclusion_criteria.is_empty() {
        "None defined.".to_string()
    } else {
        input
            .inclusion_criteria
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!("{}. [{}] {} (priority: {})", i + 1, c.id, c.text, c.priority.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let exclusion_list = if input.exclusion_criteria.is_empty() {
        "None defined.".to_string()
    } else {
        input
            .exclusion_criteria
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!("{}. [{}] {} (priority: {})", i + 1, c.id, c.text, c.priority.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let year_str =
        input.article_year.map(|y| y.to_string()).unwrap_or_else(|| "Unknown".to_string());

    format!(
        r#"## Research Aims
{aims_list}

## Inclusion Criteria
{inclusion_list}

## Exclusion Criteria
{exclusion_list}

## Priority Rules
- Higher priority rules always outweigh lower priority rules.
- If inclusion and exclusion criteria of equal priority both match, favor inclusion.

## Article
Title: {title}
Authors: {authors}
Year: {year}
Abstract: {abstract}

## Response Format
Return JSON exactly matching this schema:
{{
  "decision": "include" | "exclude",
  "reasoning": "A paragraph citing specific sentences from the abstract to justify the decision.",
  "matched_inclusion_criteria": ["criteria-id-1", ...],
  "matched_exclusion_criteria": ["criteria-id-3", ...],
  "suggested_tags": ["tag-name-1", ...],
  "confidence": 0.0-1.0
}}"#,
        aims_list = aims_list,
        inclusion_list = inclusion_list,
        exclusion_list = exclusion_list,
        title = input.article_title,
        authors = input.article_authors,
        year = year_str,
        abstract = input.article_abstract,
    )
}
