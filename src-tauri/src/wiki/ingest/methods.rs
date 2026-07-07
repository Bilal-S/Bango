//! Method page pre-seed.
//!
//! Pre-seeds `wiki/methods/{method-slug}.md` for research methodologies found
//! in the corpus. Mirrors `preseed_concept_hubs` but groups by study design
//! instead of keyword.
//!
//! ## Data sources (two on-ramps)
//!
//! 1. **Primary** (when AI summaries exist): the typed `study_design` field on
//!    each included article's `section_summaries[].study_design` (the
//!    section-aware AI summary schema). The richest signal - exact study design
//!    strings like "Randomized Controlled Trial", "Difference-in-Differences".
//! 2. **Fallback** (abstracts-only corpora): the `biblio_terms` keyword index,
//!    which is mined from `keywords + title + abstract_text` by
//!    `biblio_repo::normalization`. We intersect with a small study-design
//!    lexicon so non-methodological terms ("obesity", "sugar-tax") are
//!    filtered out.
//!
//! Both paths converge on the same `MethodRow` shape and the same
//! `render_method_hub` renderer. When neither yields any rows (e.g. a corpus
//! with no method-related signal at all), the pre-seed gracefully writes zero
//! pages - the LLM ingest, running on the same abstracts via the `raw_export`
//! content fallback, can still create method pages from the prompt directive.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};

use super::slugs::squeeze_slug;
use super::synthesis::parse_ai_summary;

/// A canonical study-design lexicon. Each entry maps a recognized method label
/// (the page title) to the synonyms/variations that should fold into it. The
/// keys are the human-readable titles; the values are lowercased substrings
/// that, when found in either a free-text `study_design` field or a
/// `biblio_terms` row, resolve to the canonical entry.
///
/// Kept deliberately small + curated so the methods layer stays high-signal
/// (mirrors the concept-hub "cap at 25" rationale). Adding a new entry here is
/// the only change needed to teach the pre-seed a new study design.
const STUDY_DESIGN_LEXICON: &[(&str, &[&str])] = &[
    (
        "Randomized Controlled Trial",
        &[
            "randomized controlled trial",
            "rct",
            "randomised controlled trial",
            "randomised trial",
            "randomized trial",
        ],
    ),
    ("Systematic Review", &["systematic review"]),
    ("Meta-Analysis", &["meta-analysis", "meta analysis"]),
    ("Cohort Study", &["cohort study", "cohort"]),
    ("Cross-Sectional Study", &["cross-sectional", "cross sectional"]),
    ("Qualitative Study", &["qualitative"]),
    ("Simulation", &["simulation", "simulated", "computational model"]),
    (
        "Difference-in-Differences",
        &["difference-in-differences", "difference in differences", "difference-in-difference"],
    ),
    ("Interrupted Time Series", &["interrupted time series", "time series"]),
    ("Regression Discontinuity", &["regression discontinuity"]),
    ("Case-Control Study", &["case-control", "case control"]),
    ("Mixed Methods", &["mixed methods", "mixed-methods"]),
];

/// A method row: the canonical design label, its slug, the articles using it,
/// and the co-occurring designs (for the "Related Methods" section).
struct MethodRow {
    label: String,
    slug: String,
    article_ids: Vec<String>,
    co_methods: Vec<String>,
}

/// Resolve a free-text study-design string to a canonical `(label, slug)`
/// pair from the lexicon. Returns `None` when the text does not match any
/// recognized design.
///
/// Matching is case-insensitive substring: `"Parallel-group RCT"` resolves to
/// `Randomized Controlled Trial` because it contains `"rct"`.
#[must_use]
fn canonicalize_study_design(raw: &str) -> Option<(&'static str, String)> {
    let lower = raw.to_lowercase();
    for (label, synonyms) in STUDY_DESIGN_LEXICON {
        for syn in *synonyms {
            if lower.contains(syn) {
                return Some((*label, squeeze_slug(label)));
            }
        }
    }
    None
}

/// Resolve a `biblio_terms` normalized term to a canonical study design, using
/// the same lexicon. Mirrors `canonicalize_study_design` but operates on the
/// term text. Returns `Some((label, slug))` when the term matches a design.
#[must_use]
fn term_to_study_design(normalized_term: &str) -> Option<(&'static str, String)> {
    canonicalize_study_design(normalized_term)
}

/// Fetch method rows from AI-summary `study_design` fields.
///
/// Iterates the included articles that have an AI summary, parses each blob,
/// extracts the `study_design` from the Methods section summary (when present),
/// canonicalizes it via the lexicon, and aggregates articles per design.
fn fetch_methods_from_summaries(conn: &Connection) -> Result<Vec<MethodRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, full_text_ai_summary \
         FROM articles \
         WHERE status = 'included' AND full_text_ai_summary IS NOT NULL AND full_text_ai_summary != ''",
    )?;
    let rows =
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))?;
    // Map: canonical slug -> (label, set of article ids).
    let mut by_slug: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for (article_id, summary_json) in rows.filter_map(Result::ok) {
        let Some(parsed) = summary_json.as_deref().and_then(parse_ai_summary) else {
            continue;
        };
        // The Methods section summary carries the typed `study_design`.
        for ss in &parsed.section_summaries {
            let section_lower = ss.section.to_lowercase();
            if section_lower != "methods"
                && section_lower != "methodology"
                && section_lower != "materials and methods"
            {
                continue;
            }
            let Some(ref raw_design) = ss.study_design else {
                continue;
            };
            let Some((label, slug)) = canonicalize_study_design(raw_design) else {
                continue;
            };
            by_slug
                .entry(slug)
                .or_insert_with(|| (label.to_string(), Vec::new()))
                .1
                .push(article_id.clone());
        }
    }
    Ok(build_method_rows(by_slug))
}

/// Fetch method rows from `biblio_terms` (the abstracts-only fallback).
///
/// Mines the top-N terms by frequency (capped by `limit`), canonicalizes each
/// via the lexicon, and aggregates articles per design. Terms that don't match
/// any study design are skipped (so "obesity" / "sugar-tax" never produce
/// method pages). Uses the existing `concepts::fetch_top_terms` query so the
/// shape stays identical.
fn fetch_methods_from_terms(conn: &Connection, limit: usize) -> Result<Vec<MethodRow>, AppError> {
    let terms = super::concepts::fetch_top_terms(conn, limit)?;
    // Reuse the lexicon to filter to method-related terms only.
    let mut by_slug: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for term in terms {
        let Some((label, slug)) = term_to_study_design(&term.normalized_term) else {
            continue;
        };
        by_slug
            .entry(slug)
            .or_insert_with(|| (label.to_string(), Vec::new()))
            .1
            .extend(term.article_ids.iter().cloned());
    }
    Ok(build_method_rows(by_slug))
}

/// Convert the slug-keyed aggregation map into a sorted `Vec<MethodRow>` with
/// co-occurring methods populated. Pure function (no I/O).
fn build_method_rows(by_slug: HashMap<String, (String, Vec<String>)>) -> Vec<MethodRow> {
    let slugs: Vec<String> = by_slug.keys().cloned().collect();
    let mut rows: Vec<MethodRow> = by_slug
        .into_iter()
        .map(|(slug, (label, mut article_ids))| {
            // Dedup article ids (an article can cite the same design twice in
            // a free-text `study_design`; the summary path can't, but the
            // terms path can via duplicate term rows).
            article_ids.sort();
            article_ids.dedup();
            MethodRow {
                label,
                slug: slug.clone(),
                article_ids,
                co_methods: slugs.iter().filter(|s| **s != slug).cloned().collect(),
            }
        })
        .collect();
    // Sort by article count desc so the most-evidenced designs come first.
    rows.sort_by_key(|r| std::cmp::Reverse(r.article_ids.len()));
    rows
}

/// Render the frontmatter + body for a method hub page. Pure function.
fn render_method_hub(method: &MethodRow) -> (Frontmatter, String) {
    let mut fm = Frontmatter::default();
    fm.set("id", &method.slug);
    fm.set("title", &method.label);
    fm.set("type", "method");
    fm.set("slug", &method.slug);
    fm.set("summary", &format!("{} articles use {}.", method.article_ids.len(), method.label));
    fm.set("status", "draft");
    let source_ids: Vec<String> =
        method.article_ids.iter().map(|id| format!("\"{}\"", id)).collect();
    fm.set("source_articles", &format!("[{}]", source_ids.join(", ")));
    fm.set("content_source", "metadata");
    // tags + links: co-occurring method slugs (method-to-method links).
    let co_tags: Vec<String> = method.co_methods.iter().map(|s| format!("\"{}\"", s)).collect();
    fm.set("tags", &format!("[{}]", co_tags.join(", ")));
    let co_links: Vec<String> =
        method.co_methods.iter().map(|s| format!("\"[[{}]]\"", s)).collect();
    fm.set("links", &format!("[{}]", co_links.join(", ")));

    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", method.label));
    body.push_str(&format!("Used in {} included articles.\n", method.article_ids.len()));
    body.push_str("\n## Relevant Studies\n\n");
    for id in &method.article_ids {
        body.push_str(&format!("- [[{}]]\n", id));
    }
    if !method.co_methods.is_empty() {
        body.push_str("\n## Related Methods\n\n");
        let links: Vec<String> = method.co_methods.iter().map(|s| format!("[[{}]]", s)).collect();
        body.push_str(&links.join(", "));
        body.push('\n');
    }
    (fm, body)
}

/// Pre-seed `wiki/methods/{method-slug}.md` for research methodologies found
/// in the corpus.
///
/// Two on-ramps (first non-empty wins):
/// 1. **AI-summary `study_design`** (when articles have AI summaries with a
///    Methods section). The richest, most accurate signal.
/// 2. **`biblio_terms` fallback** (abstracts-only corpora). Intersects the
///    keyword index with a curated study-design lexicon so non-methodological
///    terms are filtered out.
///
/// When neither yields any rows, the pre-seed writes zero pages - the LLM
/// ingest can still create method pages from the prompt directive, and the
/// grounding gate catches any ungrounded LLM fabrications.
///
/// Reviewed (user-edited) method pages are preserved. Returns the count of
/// pages written.
pub fn preseed_methods(conn: &Connection, root: &Path, limit: usize) -> Result<usize, AppError> {
    let methods_dir = root.join("wiki").join("methods");
    std::fs::create_dir_all(&methods_dir)?;

    // Primary: AI-summary study_design. When this returns >= 1 row, we use it
    // exclusively (richer + more accurate than the term fallback).
    let mut methods = fetch_methods_from_summaries(conn)?;
    // Fallback: biblio_terms (abstracts-only on-ramp).
    if methods.is_empty() {
        methods = fetch_methods_from_terms(conn, limit)?;
    }

    // Cap at `limit` so the methods layer stays curated + high-signal (mirrors
    // the concept-hub cap rationale).
    methods.truncate(limit);

    let mut written = 0;
    for method in methods {
        if method.article_ids.is_empty() {
            continue;
        }
        let path = methods_dir.join(format!("{}.md", method.slug));
        // Respect reviewed pages (user has edited them).
        if let Ok((existing_fm, _)) = frontmatter::read_file(&path) {
            if existing_fm.get("status") == Some("reviewed") {
                continue;
            }
        }
        let (fm, body) = render_method_hub(&method);
        frontmatter::write_file(&path, &fm, &body)?;
        written += 1;
    }
    Ok(written)
}
