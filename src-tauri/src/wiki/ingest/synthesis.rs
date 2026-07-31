//! Synthesis page pre-seed (Phase 2).
//!
//! Pre-seeds `wiki/synthesis/{article_id}.md` for every included article that
//! has an AI summary, using the article UUID as its slug.

use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};

use super::slugs::{concept_slug, sanitize_slug};

/// A parsed AI summary JSON blob - the deterministic source for synthesis pages.
/// All fields are optional; the pre-seeder skips articles whose summary is
/// missing or unparseable.
#[derive(Debug, Default)]
pub struct ParsedAiSummary {
    pub summary: Option<String>,
    pub key_insights: Vec<String>,
    pub keywords: Vec<String>,
    pub field: Option<String>,
    pub subfield: Option<String>,
    /// T1.3: per-section summaries (Methods/Results/Discussion). Present only
    /// on `schema_version >= 2` blobs produced by the section-aware path. Old
    /// blobs (no `section_summaries`) keep this empty and render via the
    /// legacy synthesis shape.
    pub section_summaries: Vec<ParsedSectionSummary>,
}

/// One element of the `section_summaries` array in a v2 AI-summary blob.
///
/// Typed facts (`study_design`, `sample_size`, `effect_size`,
/// `confidence_interval`) are optional and only meaningful for specific section
/// kinds; `summary` + `key_points` are always present when the section exists.
#[derive(Debug, Default, Clone)]
pub struct ParsedSectionSummary {
    pub section: String,
    pub summary: String,
    pub key_points: Vec<String>,
    pub study_design: Option<String>,
    pub sample_size: Option<String>,
    pub effect_size: Option<String>,
    pub confidence_interval: Option<String>,
}

/// Parse an article's `full_text_ai_summary` JSON blob into a `ParsedAiSummary`.
/// Returns `None` when the blob is empty or unparseable (the caller skips that
/// article gracefully).
pub fn parse_ai_summary(raw: &str) -> Option<ParsedAiSummary> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let get_str = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let get_str_array = |key: &str| -> Vec<String> {
        value
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        if let Some(s) = v.as_str() {
                            let t = s.trim().to_string();
                            if t.is_empty() {
                                None
                            } else {
                                Some(t)
                            }
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let section_summaries = parse_section_summaries(&value);
    Some(ParsedAiSummary {
        summary: get_str("summary_150_250_words"),
        key_insights: get_str_array("key_insights"),
        keywords: get_str_array("keywords"),
        field: get_str("field"),
        subfield: get_str("subfield"),
        section_summaries,
    })
}

/// Parse the `section_summaries` array out of a v2 AI-summary blob.
///
/// Each element is an object with `section`, `summary`, `key_points`, and
/// optional typed facts. Malformed elements are skipped (no panic). Returns an
/// empty `Vec` when the blob has no `section_summaries` array (v1 blobs).
fn parse_section_summaries(value: &serde_json::Value) -> Vec<ParsedSectionSummary> {
    let Some(arr) = value.get("section_summaries").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(arr.len());
    for elem in arr {
        let Some(obj) = elem.as_object() else { continue };
        let get_str = |key: &str| {
            obj.get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let section = match get_str("section") {
            Some(s) => s,
            None => continue, // a section summary without a section name is useless
        };
        let summary = get_str("summary").unwrap_or_default();
        let key_points = obj
            .get("key_points")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        out.push(ParsedSectionSummary {
            section,
            summary,
            key_points,
            study_design: get_str("study_design"),
            sample_size: get_str("sample_size"),
            effect_size: get_str("effect_size"),
            confidence_interval: get_str("confidence_interval"),
        });
    }
    out
}

/// An included article row with its AI summary, for synthesis pre-seeding.
struct ArticleWithSummary {
    id: String,
    title: String,
    year: Option<i32>,
    ai_summary_json: Option<String>,
}

/// Query included articles that have an AI summary, plus their title/year.
fn fetch_articles_with_summaries(conn: &Connection) -> Result<Vec<ArticleWithSummary>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, publication_year, full_text_ai_summary \
         FROM articles \
         WHERE status = 'included' AND full_text_ai_summary IS NOT NULL AND full_text_ai_summary != ''",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ArticleWithSummary {
            id: row.get(0)?,
            title: row.get(1)?,
            year: row.get(2)?,
            ai_summary_json: row.get(3)?,
        })
    })?;
    let articles: Vec<ArticleWithSummary> = rows.filter_map(Result::ok).collect();
    Ok(articles)
}

/// Pre-seed `wiki/synthesis/{article_id}.md` for every included article that
/// has an AI summary. Each page uses the article UUID as its slug (matching the
/// existing `[[uuid]]` / `[^art-uuid]` convention) and its `source_articles`
/// frontmatter is the singleton `[article_id]`.
///
/// The body is the `summary_150_250_words` digest + a "Key Insights" bulleted
/// section (when present). Keywords become `[[concept-slug]]` candidates in the
/// `tags` frontmatter so the graph connects to the Phase-3 concept hubs.
///
/// Reviewed (user-edited) synthesis pages are preserved. Articles without an AI
/// summary (or with unparseable JSON) are skipped - the LLM can still produce a
/// synthesis page for them.
///
/// Returns the count of pages written.
pub fn preseed_synthesis_from_ai_summaries(
    conn: &Connection,
    root: &Path,
) -> Result<usize, AppError> {
    let synth_dir = root.join("wiki").join("synthesis");
    std::fs::create_dir_all(&synth_dir)?;
    let articles = fetch_articles_with_summaries(conn)?;
    let mut written = 0;
    for article in articles {
        let path = synth_dir.join(format!("{}.md", sanitize_slug(&article.id)));
        // Respect reviewed pages (user has edited them).
        if let Ok((existing_fm, _)) = frontmatter::read_file(&path) {
            if existing_fm.get("status") == Some("reviewed") {
                continue;
            }
        }
        let Some(parsed) = article.ai_summary_json.as_deref().and_then(parse_ai_summary) else {
            continue;
        };
        let Some(ref digest) = parsed.summary else {
            // AI summary exists but has no digest field - skip (let the LLM handle).
            continue;
        };
        let (fm, body) = render_synthesis_page(&article, &parsed, digest);
        frontmatter::write_file(&path, &fm, &body)?;
        written += 1;
    }
    Ok(written)
}

/// Render the frontmatter + body for a synthesis page from an article's AI
/// summary. Pure function (no I/O) so it is trivially testable.
fn render_synthesis_page(
    article: &ArticleWithSummary,
    parsed: &ParsedAiSummary,
    digest: &str,
) -> (Frontmatter, String) {
    let mut fm = Frontmatter::default();
    fm.set("id", &article.id);
    fm.set("title", &article.title);
    fm.set("type", "synthesis");
    fm.set("slug", &article.id);
    // Summary: the first sentence of the digest, truncated for the sidebar.
    let summary_preview = digest.split('.').next().unwrap_or(digest).trim().to_string();
    let summary_capped = if summary_preview.len() > 160 {
        format!("{}...", &summary_preview[..160])
    } else if summary_preview.is_empty() {
        format!("Synthesis of {}.", article.title)
    } else {
        format!("{}.", summary_preview)
    };
    fm.set("summary", &summary_capped);
    fm.set("status", "draft");
    fm.set("source_articles", &format!("[\"{}\"]", article.id));
    fm.set("content_source", "ai_summary");
    // tags + links: keywords as concept-slug candidates. `tags` drives FTS5 +
    // graph grouping; `links` (matching the concept-hub convention) ensures the
    // graph builder creates explicit synthesis→concept edges from frontmatter,
    // not just body [[wikilinks]].
    let concept_slugs: Vec<String> = parsed.keywords.iter().map(|k| concept_slug(k)).collect();
    let keyword_tags: Vec<String> = concept_slugs.iter().map(|s| format!("\"{}\"", s)).collect();
    fm.set("tags", &format!("[{}]", keyword_tags.join(", ")));
    let concept_links: Vec<String> =
        concept_slugs.iter().map(|s| format!("\"[[{}]]\"", s)).collect();
    fm.set("links", &format!("[{}]", concept_links.join(", ")));
    if let Some(ref field) = parsed.field {
        fm.set("field", field);
    }
    if let Some(ref subfield) = parsed.subfield {
        fm.set("subfield", subfield);
    }

    // NOTE: do NOT emit `# {title}` as the first body line. The page title
    // lives in frontmatter and is rendered separately by the wiki viewer's
    // header (`<h1>{{ page.title }}</h1>`); repeating it in the body would
    // show the title twice on the rendered page.
    let mut body = String::new();
    let year_str = article.year.map(|y| format!(" ({})", y)).unwrap_or_default();
    body.push_str(&format!("## Summary\n\n{}{}\n", digest, year_str));
    if !parsed.key_insights.is_empty() {
        body.push_str("\n## Key Insights\n\n");
        for insight in &parsed.key_insights {
            body.push_str(&format!("- {}\n", insight));
        }
    }
    if !parsed.keywords.is_empty() {
        body.push_str("\n## Keywords\n\n");
        let links: Vec<String> =
            parsed.keywords.iter().map(|k| format!("[[{}]]", concept_slug(k))).collect();
        body.push_str(&links.join(", "));
        body.push('\n');
    }
    // T1.3: render per-section subsections (Methods/Results/Discussion) when
    // the v2 blob carries `section_summaries`. Old (v1) blobs have an empty
    // list and skip this branch entirely (graceful backward compat).
    for ss in &parsed.section_summaries {
        let heading = match ss.section.to_lowercase().as_str() {
            "methods" | "methodology" | "materials and methods" => "Methods",
            "results" | "findings" => "Results",
            "discussion" => "Discussion",
            _ => &ss.section,
        };
        if ss.summary.is_empty() && ss.key_points.is_empty() {
            continue;
        }
        body.push_str(&format!("\n## {heading}\n\n"));
        if !ss.summary.is_empty() {
            body.push_str(&ss.summary);
            body.push_str("\n\n");
        }
        // Typed facts as labeled bullets (only when present).
        let mut facts: Vec<String> = Vec::new();
        if let Some(ref sd) = ss.study_design {
            facts.push(format!("**Study design:** {sd}"));
        }
        if let Some(ref n) = ss.sample_size {
            facts.push(format!("**Sample size:** {n}"));
        }
        if let Some(ref es) = ss.effect_size {
            facts.push(format!("**Effect size:** {es}"));
        }
        if let Some(ref ci) = ss.confidence_interval {
            facts.push(format!("**Confidence interval:** {ci}"));
        }
        if !facts.is_empty() {
            for f in &facts {
                body.push_str(&format!("- {f}\n"));
            }
            body.push('\n');
        }
        if !ss.key_points.is_empty() {
            body.push_str("**Key points:**\n");
            for kp in &ss.key_points {
                body.push_str(&format!("- {kp}\n"));
            }
        }
    }
    (fm, body)
}
