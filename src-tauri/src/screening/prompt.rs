use crate::models::criterion::Priority;

pub const SYSTEM_PROMPT: &str = "\
Act as a systematic literature review screening assistant. \
Critically evaluate JSON array or article abstracts against research aims and criteria. \
Cite specific sentences from the text to justify your decision. \
Follow priority rules when criteria overlap or conflict. \
Return matching inclusion or exclusion criteria ids. Format response only as ordered JSON object that matches the required schema.

Return a JSON array matching this schema, one object per article, in the same order as submitted:
[
  {
    \"decision\": \"include\" | \"exclude\" | \"error\",
    \"reasoning\": \"A paragraph citing specific sentences from the abstract to justify the decision.\",
    \"matched_inclusion_criteria\": [\"criteria-id\"],
    \"matched_exclusion_criteria\": [\"criteria-id\"],
    \"suggested_tags\": [\"tag-name\"],
    \"confidence\": 0.0-1.0
  }
]";

#[derive(Clone)]
pub struct AimEntry {
    pub text: String,
}

#[derive(Clone)]
pub struct CriterionEntry {
    pub id: String,
    pub text: String,
    pub priority: Priority,
}

#[derive(Clone)]
pub struct ArticleEntry {
    pub title: String,
    pub authors: String,
    pub year: Option<i32>,
    pub abstract_text: String,
}

pub struct ScreeningPromptInput {
    pub aims: Vec<AimEntry>,
    pub inclusion_criteria: Vec<CriterionEntry>,
    pub exclusion_criteria: Vec<CriterionEntry>,
    pub articles: Vec<ArticleEntry>,
}

/// Returns true when all criteria (both inclusion and exclusion) share the same priority,
/// or when there are zero or one criteria total.
fn all_same_priority(inclusion: &[CriterionEntry], exclusion: &[CriterionEntry]) -> bool {
    let all_priorities: Vec<Priority> =
        inclusion.iter().chain(exclusion.iter()).map(|c| c.priority).collect();

    if all_priorities.len() <= 1 {
        return true;
    }
    let first = all_priorities[0];
    all_priorities.iter().all(|p| *p == first)
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

    let same_priority = all_same_priority(&input.inclusion_criteria, &input.exclusion_criteria);

    // Sort criteria by priority descending when priorities differ
    let sorted_inclusion: Vec<&CriterionEntry> = if same_priority {
        input.inclusion_criteria.iter().collect()
    } else {
        let mut v: Vec<&CriterionEntry> = input.inclusion_criteria.iter().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.priority));
        v
    };

    let sorted_exclusion: Vec<&CriterionEntry> = if same_priority {
        input.exclusion_criteria.iter().collect()
    } else {
        let mut v: Vec<&CriterionEntry> = input.exclusion_criteria.iter().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.priority));
        v
    };

    let inclusion_header = if same_priority {
        "## Inclusion Criteria"
    } else {
        "## Inclusion Criteria (in order of priority)"
    };

    let exclusion_header = if same_priority {
        "## Exclusion Criteria"
    } else {
        "## Exclusion Criteria (in order of priority)"
    };

    let inclusion_list = if sorted_inclusion.is_empty() {
        "None defined.".to_string()
    } else {
        sorted_inclusion
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}. [{}] {}", i + 1, c.id, c.text))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let exclusion_list = if sorted_exclusion.is_empty() {
        "None defined.".to_string()
    } else {
        sorted_exclusion
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}. [{}] {}", i + 1, c.id, c.text))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let priority_rules = if same_priority {
        "If conflict between criteria favor inclusion rules.".to_string()
    } else {
        "Higher priority rules always outweigh lower priority rules.\n\
         - If inclusion and exclusion criteria of equal priority both match, favor inclusion."
            .to_string()
    };

    // Build articles JSON array
    let articles_json = if input.articles.is_empty() {
        "[]".to_string()
    } else {
        let entries: Vec<String> = input
            .articles
            .iter()
            .map(|a| {
                let year_val = a.year.map(|y| y.to_string()).unwrap_or_else(|| "null".to_string());
                format!(
                    r#"{{"title": {}, "authors": {}, "year": {}, "abstract": {}}}"#,
                    escape_json_str(&a.title),
                    escape_json_str(&a.authors),
                    year_val,
                    escape_json_str(&a.abstract_text),
                )
            })
            .collect();
        format!("[\n{}\n]", entries.join(",\n"))
    };

    format!(
        r#"## Research Aims
{aims_list}

{inclusion_header}
{inclusion_list}

{exclusion_header}
{exclusion_list}

## Priority Rules
{priority_rules}

## Articles
{articles_json}"#,
        aims_list = aims_list,
        inclusion_header = inclusion_header,
        inclusion_list = inclusion_list,
        exclusion_header = exclusion_header,
        exclusion_list = exclusion_list,
        priority_rules = priority_rules,
        articles_json = articles_json,
    )
}

/// Escape a string for embedding inside a JSON string value (without surrounding quotes).
fn escape_json_str(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
    )
}
