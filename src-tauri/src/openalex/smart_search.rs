//! Smart Search: LLM-generated OpenAlex Boolean query from research aims + criteria.
//!
//! Reuses the `build_search_strategy_prompt` pattern from §8.4 but targets
//! OpenAlex syntax (which supports `AND`, `OR`, `NOT`, quoted phrases).

use serde::{Deserialize, Serialize};

use crate::db::criteria_repo;
use crate::error::AppError;
use crate::models::criterion::{Criterion, CriterionType, ResearchAim};
use crate::summary::prompt::strip_code_fences;

/// The LLM's parsed response: an OpenAlex Boolean query + suggested filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSearchQuery {
    pub search_query: String,
    pub suggested_filters: SmartSearchFilters,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SmartSearchFilters {
    pub publication_year: Option<String>,
    #[serde(default)]
    pub r#type: Vec<String>,
}

/// Pure: build the (system, user) prompt pair from aims + criteria.
#[must_use]
pub fn build_smart_search_prompt(
    aims: &[ResearchAim],
    inclusion: &[Criterion],
    exclusion: &[Criterion],
) -> (String, String) {
    let system = SYSTEM_PROMPT.to_string();

    let aims_list: Vec<String> =
        aims.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a.text)).collect();

    let inclusion_list: Vec<String> = inclusion
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. [{}] {}", i + 1, c.priority.as_str(), c.text))
        .collect();

    let exclusion_list: Vec<String> = exclusion
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. [{}] {}", i + 1, c.priority.as_str(), c.text))
        .collect();

    let inclusion_block = if inclusion_list.is_empty() {
        "None defined.".to_string()
    } else {
        inclusion_list.join("\n")
    };
    let exclusion_block = if exclusion_list.is_empty() {
        "None defined.".to_string()
    } else {
        exclusion_list.join("\n")
    };

    let user = format!(
        r#"## Task
You are building an OpenAlex search query for a systematic literature review.
Extract the PICO concepts from the research aims (and criteria, if present),
find synonyms and spelling variants for each concept, then compose a single
OpenAlex Boolean search string.

## Research Aims
{aims}

## Inclusion Criteria
{inclusion}

## Exclusion Criteria
{exclusion}

## Instructions
- Use `AND`, `OR`, `NOT` (uppercase) as Boolean operators.
- Use double quotes for exact phrases (e.g., "sugar-sweetened beverage").
- Map inclusion criteria to `OR`-groups joined by `AND`.
- Map exclusion criteria to `NOT`-groups.
- Do NOT include the `filter=` syntax - only the `search=` query.
- Do not use em dashes anywhere in the response.

## Response Format
Return ONLY a JSON object matching this schema (no prose, no markdown fences):
{{
  "searchQuery": "(\"sugar-sweetened beverage\" OR SSB OR \"soft drink\") AND (tax OR levy OR policy) AND (health OR consumption OR obesity OR diet)",
  "suggestedFilters": {{
    "publicationYear": "2010-2025",
    "type": ["article", "review"]
  }}
}}"#,
        aims = aims_list.join("\n"),
        inclusion = inclusion_block,
        exclusion = exclusion_block,
    );

    (system, user)
}

/// Pure: parse + lightly validate the LLM JSON response.
pub fn parse_smart_search_response(raw: &str) -> Result<SmartSearchQuery, AppError> {
    let cleaned = strip_code_fences(raw);
    let parsed: SmartSearchQuery = serde_json::from_str(&cleaned)
        .map_err(|e| AppError::Import(format!("Failed to parse smart search response: {e}")))?;
    Ok(parsed)
}

/// Read aims + criteria from the DB, grouped by type. Returns `(aims, inclusion, exclusion)`.
#[allow(clippy::type_complexity)]
pub fn read_aims_and_criteria(
    conn: &rusqlite::Connection,
) -> Result<(Vec<ResearchAim>, Vec<Criterion>, Vec<Criterion>), AppError> {
    let aims = criteria_repo::get_all_aims(conn)?;
    if aims.is_empty() {
        return Err(AppError::Validation(
            "Research aims must be defined before generating a smart search".to_string(),
        ));
    }
    let all_criteria = criteria_repo::get_all_criteria(conn)?;
    let inclusion: Vec<Criterion> = all_criteria
        .iter()
        .filter(|c| matches!(c.criterion_type, CriterionType::Inclusion))
        .cloned()
        .collect();
    let exclusion: Vec<Criterion> = all_criteria
        .iter()
        .filter(|c| matches!(c.criterion_type, CriterionType::Exclusion))
        .cloned()
        .collect();
    Ok((aims, inclusion, exclusion))
}

const SYSTEM_PROMPT: &str = "\
You are a systematic-review search strategist specializing in OpenAlex queries.\n\
You build OpenAlex Boolean search strings from a research question's PICO concepts.\n\
You return ONLY a JSON object, no prose, no markdown fences, no em dashes.\n\n\
OPENALEX SYNTAX RULES:\n\
- Operators: AND, OR, NOT (MUST be uppercase)\n\
- Exact phrases: use double quotes (e.g., \"sugar tax\")\n\
- Default operator between words is AND\n\
- The search covers title + abstract + fulltext\n\
- Do NOT include filter= syntax - only the search query string\n";
