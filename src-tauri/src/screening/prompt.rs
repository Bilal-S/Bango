use crate::models::criterion::Priority;

pub const SYSTEM_PROMPT: &str = "\
Act as a systematic literature review screening assistant. \
Critically evaluate JSON array or article abstracts against research aims and criteria. \
Cite specific sentences from the text to justify your decision. \
Follow priority rules when criteria overlap or conflict. \
Return matching inclusion or exclusion criteria ids. Format response only as ordered JSON object that matches the required schema. \
Where supporting full-text evidence is provided, use it to verify criteria matches. The primary decision rests on the abstract. \
Evidence marked `[Source: AI Summary]` is a structured distillation - reliable for factual lookups \
(study design, sample size) but may contain hallucinations; cross-check any summary fact against \
the `[Source: Full Text - verbatim]` chunk when both are present. If the abstract, summary, and \
verbatim chunk conflict, note the discrepancy in your reasoning and prefer the verbatim chunk \
for specific sentences. \
If a Custom Screening Instructions section is provided, apply those rules strictly when deciding \
include/exclude. Reference criteria by their numbered position (inclusion is numbered 1..N, then \
exclusion continues at N+1..N+M, so every number is unique across both lists).

## Tag and Label Guidelines
- `suggested_tags` describe the article's content, topic, or methodology (e.g. \"machine-learning\", \"systematic-review\"). \
- Tags must be concise descriptors, NOT justifications or full criterion text. \
- Each tag must be at most 35 characters, lowercase, and hyphenated. \
- Do NOT prefix tags with \"inclusion:\" or \"exclusion:\" - those prefixes are for labels, not tags. \
- Tags should be short, meaningful, and reusable across articles.

Inside any JSON string value, represent line breaks, tabs, and other control characters as their two-character JSON escapes (\\n, \\t, \\r), never as literal newline/tab/control bytes. Literal control bytes inside string values make the JSON unparseable. \
Return a JSON array matching this schema, one object per article, in the same order as submitted:
[
  {
    \"decision\": \"include\" | \"exclude\" | \"error\",
    \"reasoning\": \"A paragraph citing specific sentences from the abstract to justify the decision.\",
    \"matched_inclusion_criteria\": [\"criteria-id\"],
    \"matched_exclusion_criteria\": [\"criteria-id\"],
    \"suggested_tags\": [\"tag-name\"],
    \"confidence\": 0.0-1.0,
    \"extracted_terms\": [\"noun-phrase from abstract\"]
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
    /// Globally unique 1-based number (inclusion `1..N`, exclusion continues
    /// `N+1..N+M`) matching the Criteria screen. The prompt formats each line
    /// as `{global_number}. [{id}] {text}` so user-authored combinatorial
    /// rules ("criterion 3 AND 5") resolve identically across the UI, the
    /// prompt, and the LLM's reasoning.
    pub global_number: usize,
}

#[derive(Clone)]
pub struct ArticleEntry {
    pub title: String,
    pub authors: String,
    pub year: Option<i32>,
    pub abstract_text: String,
    /// Tier 3: retrieved full-text chunks (enhanced / two-stage mode), formatted as
    /// `[§Methods] ...` / `[§Results] ...` lines. `None` in abstract mode (the
    /// evidence block is omitted entirely so abstract-mode prompts are unchanged).
    pub full_text_evidence: Option<String>,
}

impl ArticleEntry {
    /// Build an `ArticleEntry` without full-text evidence (the abstract-mode shape).
    /// Convenience constructor keeping existing call sites concise.
    pub fn new(title: String, authors: String, year: Option<i32>, abstract_text: String) -> Self {
        Self { title, authors, year, abstract_text, full_text_evidence: None }
    }
}

pub struct ScreeningPromptInput {
    pub aims: Vec<AimEntry>,
    pub inclusion_criteria: Vec<CriterionEntry>,
    pub exclusion_criteria: Vec<CriterionEntry>,
    pub articles: Vec<ArticleEntry>,
    pub existing_tags: Vec<String>,
    pub existing_labels: Vec<String>,
    /// Optional free-text combinatorial rules the LLM applies strictly
    /// (AND/OR gates, hard exclusions, conditional inclusion). References
    /// criteria by their `global_number`. `None` or whitespace-only omits the
    /// `## Custom Screening Instructions` section entirely (byte-identical to
    /// pre-feature prompts → backward-compatible).
    pub custom_logic: Option<String>,
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
            .map(|c| format!("{}. [{}] {}", c.global_number, c.id, c.text))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let exclusion_list = if sorted_exclusion.is_empty() {
        "None defined.".to_string()
    } else {
        sorted_exclusion
            .iter()
            .map(|c| format!("{}. [{}] {}", c.global_number, c.id, c.text))
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

    // Custom Screening Instructions section. Omitted entirely when the setting
    // is absent or trims to empty → byte-identical to pre-feature prompts
    // (backward-compat for existing prompt tests + cost).
    let custom_logic_section = match input.custom_logic.as_deref().map(str::trim) {
        Some(text) if !text.is_empty() => {
            format!("\n## Custom Screening Instructions\n{text}\n", text = text)
        }
        _ => String::new(),
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

    // Build the supporting-evidence section. Only articles carrying
    // `full_text_evidence = Some(...)` contribute; in abstract mode every entry
    // is `None`, the section is empty, and the prompt is byte-identical to the
    // pre-Tier-3 shape (backward compat for existing prompt tests + cost).
    let evidence_blocks: Vec<String> = input
        .articles
        .iter()
        .filter_map(|a| a.full_text_evidence.as_ref().map(|ev| format!("### {}\n{}", a.title, ev)))
        .collect();
    let evidence_section = if evidence_blocks.is_empty() {
        String::new()
    } else {
        format!(
            "\n## Supporting Evidence from Full Text \
             (use ONLY to verify criteria; primary decision from abstract)\n{}\n",
            evidence_blocks.join("\n")
        )
    };

    // Build existing tags/labels section
    let existing_tags_section = if input.existing_tags.is_empty() {
        String::new()
    } else {
        let tags_list = input.existing_tags.join(", ");
        format!(
            "## Existing Tags\n\
             The following tags already exist in the project. \
             Prefer selecting from these tags when they are relevant. \
             Only suggest new tags if no existing tag fits.\n\
             [{}]",
            tags_list
        )
    };

    let existing_labels_section = if input.existing_labels.is_empty() {
        String::new()
    } else {
        let labels_list = input.existing_labels.join(", ");
        format!(
            "## Existing Labels\n\
             The following labels already exist in the project. \
             Prefer selecting from these when applicable. \
             Only suggest new labels if no existing label fits.\n\
             [{}]",
            labels_list
        )
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
{custom_logic_section}{existing_tags_section}
{existing_labels_section}
## Articles
{articles_json}{evidence_section}"#,
        aims_list = aims_list,
        inclusion_header = inclusion_header,
        inclusion_list = inclusion_list,
        exclusion_header = exclusion_header,
        exclusion_list = exclusion_list,
        priority_rules = priority_rules,
        custom_logic_section = custom_logic_section,
        existing_tags_section = existing_tags_section,
        existing_labels_section = existing_labels_section,
        articles_json = articles_json,
        evidence_section = evidence_section,
    )
}

/// Escape a string for embedding inside a JSON string value (without surrounding quotes).
fn escape_json_str(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
    )
}
