//! Research Gap Analysis prompt builder. Pure functions for corpus-wide gap report user prompts.
//! Article block reuses `build_summary_prompt` renderer (Shape-A evidence included)
//! for byte-consistency with the literature-review prompt.

use crate::summary::prompt::ArticleSummary;

/// System prompt: expert research analyst, Markdown-only output, no em dashes.
pub const GAP_ANALYSIS_SYSTEM_PROMPT: &str = include_str!("gap_analysis_prompt.md");

/// Compact bibliometric context from `biblio_repo`. Grounds claims about temporal,
/// topical, and geographic distribution of the corpus.
#[derive(Debug, Clone, Default)]
pub struct BiblioContext {
    /// `(first_year, last_year)` span of included articles; `None` when empty.
    pub year_range: Option<(i32, i32)>,
    /// `(year, count)` publications-per-year, ascending by year.
    pub pubs_by_year: Vec<(i32, i32)>,
    /// `(journal_title, article_count)` top journals by included-article count.
    pub top_journals: Vec<(String, i32)>,
    /// `(normalized_term, article_count)` top terms (keywords / noun phrases).
    pub top_terms: Vec<(String, i32)>,
    /// `(country, article_count)` geographic distribution of affiliations.
    pub geographic_distribution: Vec<(String, i32)>,
}

/// Inputs to the gap-analysis user prompt builder.
pub struct GapPromptInput {
    pub aims: Vec<String>,
    /// Pre-rendered screening methodology summary (consistency with literature-review prompt).
    pub screening_summary: String,
    pub citation_style: String,
    pub articles: Vec<ArticleSummary>,
    pub biblio_context: BiblioContext,
    /// Full inclusion criteria. Same role as summary prompt.
    pub inclusion_criteria: Vec<String>,
    /// Full exclusion criterion definitions.
    pub exclusion_criteria: Vec<String>,
}

/// Render gap-analysis user prompt. Pure, no I/O. Mirrors `build_summary_prompt` structure.
#[must_use]
pub fn build_gap_analysis_prompt(input: &GapPromptInput) -> String {
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

    let articles_text = render_articles_block(&input.articles);

    let biblio_text = render_biblio_context(&input.biblio_context);

    let criteria_text = render_criteria(&input.inclusion_criteria, &input.exclusion_criteria);

    format!(
        r#"## Task
Analyze the corpus of included articles below and produce a Research Gap Analysis
as a single Markdown document. Identify thematic coverage, gaps, the methodological
landscape, and concrete future research directions. Ground every claim in the
cited articles.

## Research Aims
{aims}

## Search and Screening Methodology
{screening}

## Eligibility Criteria
{criteria}

## Citation Style
Use **{citation_style}** citation style for all in-text citations and the references list.

## Included Articles
{articles}

## Bibliometric Context
{biblio}

## Instructions
Produce the Markdown document with exactly these H2 sections in this order:
- `# Research Gaps and Future Directions` (single H1 title)
- `## Thematic Coverage` (bullets per theme with coverage level + article count)
- `## Identified Gaps` (bullets with category + grounded rationale)
- `## Methodological Landscape` (designs, sample-size range, geographic concentration)
- `## Future Research Directions` (priority-ranked, grounded directions)
- `## References` (numbered list of cited articles only, in {citation_style} style)

Return only the Markdown text. Do not wrap it in code fences. Do not return JSON.
Do not use em dashes. Never invent references."#,
        aims = aims_list,
        screening = input.screening_summary,
        criteria = criteria_text,
        citation_style = input.citation_style,
        articles = articles_text,
        biblio = biblio_text,
    )
}

/// Render included-articles block. Byte-consistent with `build_summary_prompt`.
fn render_articles_block(articles: &[ArticleSummary]) -> String {
    articles
        .iter()
        .map(|a| {
            let year_str = a.year.map(|y| y.to_string()).unwrap_or_else(|| "Unknown".to_string());
            let keywords = if a.keywords.is_empty() {
                String::new()
            } else {
                format!("\nKeywords: {}", a.keywords.join(", "))
            };
            let evidence = if let Some(ev) = &a.evidence {
                format!("\nEvidence: {ev}")
            } else {
                String::new()
            };
            format!(
                "---\nTitle: {}\nAuthors: {}\nYear: {}\nAbstract: {}{}{}\n---",
                a.title,
                a.authors.join("; "),
                year_str,
                a.abstract_text,
                keywords,
                evidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render bibliometric context. Empty sections omitted.
fn render_biblio_context(ctx: &BiblioContext) -> String {
    let mut lines: Vec<String> = Vec::new();

    if let Some((first, last)) = ctx.year_range {
        lines.push(format!("- Publication year span: {first}-{last}."));
    }

    if !ctx.pubs_by_year.is_empty() {
        let summary = ctx
            .pubs_by_year
            .iter()
            .map(|(y, c)| format!("{y}: {c}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("- Publications by year: {summary}."));
    }

    if !ctx.top_journals.is_empty() {
        let summary = ctx
            .top_journals
            .iter()
            .map(|(j, c)| format!("{j} ({c})"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("- Top journals: {summary}."));
    }

    if !ctx.top_terms.is_empty() {
        let summary =
            ctx.top_terms.iter().map(|(t, c)| format!("{t} ({c})")).collect::<Vec<_>>().join(", ");
        lines.push(format!("- Top terms/keywords: {summary}."));
    }

    if !ctx.geographic_distribution.is_empty() {
        let summary = ctx
            .geographic_distribution
            .iter()
            .map(|(country, c)| format!("{country} ({c})"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("- Geographic distribution: {summary}."));
    }

    if lines.is_empty() {
        "- No bibliometric aggregates available.".to_string()
    } else {
        lines.join("\n")
    }
}

/// Render inclusion/exclusion criteria as a bulleted block.
fn render_criteria(inclusion: &[String], exclusion: &[String]) -> String {
    let mut lines: Vec<String> = Vec::new();
    if inclusion.is_empty() && exclusion.is_empty() {
        return "None defined.".to_string();
    }
    if !inclusion.is_empty() {
        lines.push("Inclusion:".to_string());
        for c in inclusion {
            lines.push(format!("  - {c}"));
        }
    }
    if !exclusion.is_empty() {
        lines.push("Exclusion:".to_string());
        for c in exclusion {
            lines.push(format!("  - {c}"));
        }
    }
    lines.join("\n")
}

/// Render user prompt for merging two partial gap reports. Used when corpus >80%
/// context window. Pure, no I/O.
#[must_use]
pub fn build_gap_synthesis_prompt(
    aims: &[String],
    citation_style: &str,
    partial_a: &str,
    partial_b: &str,
) -> String {
    let aims_text = if aims.is_empty() {
        "None defined.".to_string()
    } else {
        aims.iter()
            .enumerate()
            .map(|(i, a)| format!("{}. {}", i + 1, a))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"## Task
Combine two partial Research Gap Analyses into a single coherent Markdown report.
Maintain focus on the research aims. Do NOT invent references that do not appear
in either section. De-duplicate themes and gaps that appear in both halves.

## Research Aims
{aims}

## Citation Style
Use **{citation_style}** citation style throughout.

## Partial Gap Analysis A
{a}

## Partial Gap Analysis B
{b}

## Instructions
Produce a single unified Markdown report with exactly these H2 sections in order:
- `# Research Gaps and Future Directions` (single H1 title)
- `## Thematic Coverage` (merged + de-duplicated themes)
- `## Identified Gaps` (merged + de-duplicated gaps)
- `## Methodological Landscape` (one consolidated paragraph + bullets)
- `## Future Research Directions` (merged + priority-ranked)
- `## References` (one de-duplicated numbered list)

Return only the Markdown text. Do not wrap it in code fences.
Do not use em dashes. Never invent references."#,
        aims = aims_text,
        citation_style = citation_style,
        a = partial_a,
        b = partial_b,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_article() -> ArticleSummary {
        ArticleSummary {
            title: "Sugar taxes and obesity".to_string(),
            authors: vec!["Smith J".to_string(), "Lee K".to_string()],
            year: Some(2020),
            abstract_text: "We studied the effect of SSB taxes.".to_string(),
            keywords: vec!["sugar tax".to_string(), "obesity".to_string()],
            evidence: None,
        }
    }

    #[test]
    fn system_prompt_is_not_empty_and_forbids_em_dash() {
        assert!(!GAP_ANALYSIS_SYSTEM_PROMPT.trim().is_empty());
        assert!(
            !GAP_ANALYSIS_SYSTEM_PROMPT.contains('\u{2014}'),
            "system prompt must not contain an em dash"
        );
    }

    #[test]
    fn system_prompt_requires_all_five_h2_sections() {
        for section in [
            "## Thematic Coverage",
            "## Identified Gaps",
            "## Methodological Landscape",
            "## Future Research Directions",
            "## References",
        ] {
            assert!(
                GAP_ANALYSIS_SYSTEM_PROMPT.contains(section),
                "system prompt must document the '{section}' section"
            );
        }
    }

    #[test]
    fn build_prompt_contains_aims_screening_articles_and_biblio() {
        let input = GapPromptInput {
            aims: vec!["Effect of SSB taxes on consumption".to_string()],
            screening_summary: "Records screened: 50.".to_string(),
            citation_style: "APA".to_string(),
            articles: vec![sample_article()],
            biblio_context: BiblioContext {
                year_range: Some((2018, 2022)),
                pubs_by_year: vec![(2018, 1), (2020, 1)],
                top_journals: vec![("The Lancet".to_string(), 2)],
                top_terms: vec![("sugar tax".to_string(), 3)],
                geographic_distribution: vec![("United Kingdom".to_string(), 2)],
            },
            inclusion_criteria: vec!["SSB tax studies".to_string()],
            exclusion_criteria: vec!["Non-English".to_string()],
        };
        let prompt = build_gap_analysis_prompt(&input);
        assert!(prompt.contains("Effect of SSB taxes on consumption"));
        assert!(prompt.contains("Records screened: 50."));
        assert!(prompt.contains("Sugar taxes and obesity"));
        assert!(prompt.contains("Keywords: sugar tax, obesity"));
        assert!(prompt.contains("Publication year span: 2018-2022"));
        assert!(prompt.contains("Top journals: The Lancet (2)"));
        assert!(prompt.contains("Geographic distribution: United Kingdom (2)"));
        assert!(prompt.contains("Inclusion:"));
        assert!(prompt.contains("SSB tax studies"));
    }

    #[test]
    fn build_prompt_includes_evidence_line_when_present() {
        let article = ArticleSummary {
            evidence: Some("study_type: RCT; sample_size: 1000".to_string()),
            ..sample_article()
        };
        let input = GapPromptInput {
            aims: vec!["Aim 1".to_string()],
            screening_summary: "n/a".to_string(),
            citation_style: "APA".to_string(),
            articles: vec![article],
            biblio_context: BiblioContext::default(),
            inclusion_criteria: Vec::new(),
            exclusion_criteria: Vec::new(),
        };
        let prompt = build_gap_analysis_prompt(&input);
        assert!(prompt.contains("Evidence: study_type: RCT; sample_size: 1000"));
    }

    #[test]
    fn build_prompt_empty_biblio_shows_placeholder() {
        let input = GapPromptInput {
            aims: vec!["Aim 1".to_string()],
            screening_summary: "n/a".to_string(),
            citation_style: "APA".to_string(),
            articles: vec![sample_article()],
            biblio_context: BiblioContext::default(),
            inclusion_criteria: Vec::new(),
            exclusion_criteria: Vec::new(),
        };
        let prompt = build_gap_analysis_prompt(&input);
        assert!(prompt.contains("No bibliometric aggregates available."));
    }

    #[test]
    fn synthesis_prompt_contains_both_partials_and_aims() {
        let prompt = build_gap_synthesis_prompt(
            &["Aim 1".to_string()],
            "APA",
            "PARTIAL A CONTENT",
            "PARTIAL B CONTENT",
        );
        assert!(prompt.contains("PARTIAL A CONTENT"));
        assert!(prompt.contains("PARTIAL B CONTENT"));
        assert!(prompt.contains("Aim 1"));
        assert!(prompt.contains("**APA**"));
    }
}
