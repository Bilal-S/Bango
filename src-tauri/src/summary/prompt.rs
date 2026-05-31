#[derive(Clone)]
pub struct ArticleSummary {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub abstract_text: String,
    pub ai_reasoning: Option<String>,
}

#[derive(Clone)]
pub struct ScreeningData {
    pub records_identified: usize,
    pub duplicates_removed: usize,
    pub records_screened: usize,
    pub records_excluded: usize,
    pub records_excluded_with_reasons: usize,
    pub records_assessed: usize,
    pub records_in_progress: usize,
    pub studies_included: usize,
    pub ai_screened: usize,
    pub manual_reviewed: usize,
    pub exclusion_reasons: Vec<(String, usize)>,
}

pub struct SummaryPromptInput {
    pub aims: Vec<String>,
    pub screening_data: ScreeningData,
    pub citation_style: String,
    pub articles: Vec<ArticleSummary>,
}

pub const SYSTEM_PROMPT: &str = "You are an expert academic literature review writer. You produce well-structured, scholarly literature reviews with proper in-text citations and a complete references section. You only cite sources that are explicitly provided. You never fabricate references. You write in formal academic English with natural variation in sentence length. You never use em dashes.";

/// Format screening statistics into a human-readable summary for the prompt.
#[must_use]
pub fn format_screening_summary(data: &ScreeningData) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Total records identified: {}", data.records_identified));

    if data.duplicates_removed > 0 {
        lines.push(format!(
            "Duplicates removed: {} ({} unique records after deduplication)",
            data.duplicates_removed,
            data.records_screened
        ));
    }

    lines.push(format!("Records screened: {}", data.records_screened));

    if data.ai_screened > 0 || data.manual_reviewed > 0 {
        lines.push(format!(
            "Screening method: {} articles were screened using AI-assisted review, {} underwent manual review by the researcher.",
            data.ai_screened, data.manual_reviewed
        ));
    }

    if data.records_excluded > 0 {
        lines.push(format!("Records excluded: {}", data.records_excluded));
        if data.records_excluded_with_reasons > 0 {
            lines.push(format!(
                "Excluded with specific criteria: {}",
                data.records_excluded_with_reasons
            ));
        }
    }

    lines.push(format!("Records assessed for eligibility: {}", data.records_assessed));

    if data.records_in_progress > 0 {
        lines.push(format!("Records still in progress: {}", data.records_in_progress));
    }

    lines.push(format!("Studies included in final review: {}", data.studies_included));

    if !data.exclusion_reasons.is_empty() {
        lines.push("Top exclusion reasons:".to_string());
        for (text, count) in &data.exclusion_reasons {
            lines.push(format!("  - {} ({} articles)", text, count));
        }
    }

    lines.join("\n")
}

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

    let screening_summary = format_screening_summary(&input.screening_data);

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
Write a comprehensive, well-structured literature review based on the included articles below.
Focus on the research aims and synthesize findings across studies.

## Research Aims
{aims}

## Search and Screening Methodology (use this data to write the Methodology section)
{screening}

## Citation Style
Use **{citation_style}** citation style for all in-text citations and the references section.

## Included Articles
{articles}

## Instructions
Write a literature review with the following sections. Each section should be substantive and scholarly.
Use **Markdown formatting** with proper headings as shown below:

1. **Title** - An H1 heading (`# Title`) with a descriptive title for the literature review.
2. **Abstract** - An H2 heading (`## Abstract`) followed by a concise summary (150 to 250 words) of the review's scope, key findings, and conclusions.
3. **Introduction** - An H2 heading (`## Introduction`) introducing the research context, stating the research aims, and outlining the review's scope and structure.
4. **Methodology** - An H2 heading (`## Methodology`) describing the search and screening process using the data provided above. Explain how records were identified, how many were screened, the screening approach (AI-assisted and/or manual review), exclusion criteria applied, and how many studies were ultimately included. Write this as a narrative description of the systematic process.
5. **Results** - An H2 heading (`## Results`) presenting the key findings from the included studies, organized by themes. Use H3 subheadings (`### Theme`) for each thematic group. Cite relevant studies using {citation_style} style.
6. **Discussion** - An H2 heading (`## Discussion`) interpreting the findings, identifying patterns and contradictions across studies, and relating them back to the research aims.
7. **Conclusion** - An H2 heading (`## Conclusion`) summarizing the main takeaways, stating implications, and suggesting directions for future research.
8. **References** - An H2 heading (`## References`) followed by a numbered list of all cited works in proper {citation_style} format. Only include references from the provided articles. Do NOT invent references.

## Writing Style Rules
- Do NOT use em dashes (the long dash character) anywhere in the text. Use commas, parentheses, colons, or split into separate sentences instead.
- Write in formal academic prose with natural variation in sentence length. Mix shorter declarative sentences with longer, more complex ones to reflect human academic writing.
- Avoid repetitive sentence structures. Vary how you open paragraphs and transition between ideas.
- Only cite articles that are listed above. Never fabricate or invent references.
- Use proper {citation_style} in-text citations throughout (e.g., author-date, numeric, etc. as appropriate for the style).
- Synthesize findings across studies rather than summarizing each study individually.
- Use Markdown formatting: headings (`#`, `##`, `###`), **bold** for emphasis where appropriate, and proper list formatting for the References section.
- Return only the Markdown text. Do not wrap it in code fences (no ``` markers) or JSON."#,
        aims = aims_list,
        screening = screening_summary,
        citation_style = input.citation_style,
        articles = articles_text,
    )
}