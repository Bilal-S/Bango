//! Cluster thematic analysis helpers.
//!
//! Pure resolution + prompt building behind the `biblio_analyze_cluster_themes`
//! command: resolve one Louvain cluster's member entities to `included`
//! articles, cap the corpus with a Top-N representative-article limit, and
//! build a grounded prompt whose markdown references use the stable
//! `author:{biblio_authors.id}` / `article:{articles.id}` link protocols.
//!
//! Node `id` semantics per network (see the plan's Verified contracts):
//! `co_authorship` members are `biblio_authors.id` UUIDs; `co_occurrence`
//! members are `normalize_term(raw_term)` strings produced at fetch time by
//! `get_keyword_network_json` (NOT `biblio_terms.id`).

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::biblio::normalizer::normalize_term;
use crate::error::AppError;
use crate::models::biblio::NetworkType;

/// Maximum articles sent to the LLM (decision D1: Top-N cap instead of a
/// batch/synthesis pipeline). Ranking: `num_cited` DESC (NULLs last), then
/// `publication_year` DESC, then id ASC for determinism.
pub const MAX_ARTICLES_PER_CLUSTER: usize = 40;

/// Maximum cluster members listed in the reference-identifier block.
/// Members are cheap (one line each), but extreme clusters still need a bound.
pub const MAX_MEMBERS_PER_CLUSTER: usize = 100;

/// Per-article abstract truncation limit, applied on a word boundary.
pub const ABSTRACT_MAX_CHARS: usize = 1200;

/// Per-article author-list truncation limit, applied on a word boundary
/// (guards the prompt budget against mega-author papers).
pub const AUTHORS_MAX_CHARS: usize = 300;

/// Per-article keyword-list truncation limit, applied on a word boundary.
pub const KEYWORDS_MAX_CHARS: usize = 200;

/// A cluster member entity sent by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterMember {
    pub id: String,
    pub label: String,
}

/// Resolved article backing the thematic prompt. Distinct from
/// `summary::prompt::ArticleSummary` (decision D9): it carries the stable
/// `id` needed by the link protocols and no `evidence` field.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterArticleSummary {
    pub id: String,
    pub title: String,
    pub authors: String,
    pub year: Option<i32>,
    pub abstract_text: Option<String>,
    pub doi: Option<String>,
    pub keywords: Option<String>,
    pub num_cited: Option<i64>,
}

/// A markdown link protocol the LLM may emit (decision D10: protocols are
/// data, not branches). `example` is the instruction shown in the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkProtocol {
    pub prefix: &'static str,
    pub example: &'static str,
}

const ARTICLE_PROTOCOL: LinkProtocol =
    LinkProtocol { prefix: "article", example: "[Article Title](article:ARTICLE_ID)" };
const AUTHOR_PROTOCOL: LinkProtocol =
    LinkProtocol { prefix: "author", example: "[Author Name](author:AUTHOR_ID)" };
const CO_AUTHORSHIP_PROTOCOLS: &[LinkProtocol] = &[ARTICLE_PROTOCOL, AUTHOR_PROTOCOL];
const CO_OCCURRENCE_PROTOCOLS: &[LinkProtocol] = &[ARTICLE_PROTOCOL];

/// Link protocols each network is taught. Networks without a supported
/// resolver yield an empty slice (the dispatcher rejects them separately).
#[must_use]
pub fn link_protocols_for(network_type: &NetworkType) -> &'static [LinkProtocol] {
    match network_type {
        NetworkType::CoAuthorship => CO_AUTHORSHIP_PROTOCOLS,
        NetworkType::CoOccurrence => CO_OCCURRENCE_PROTOCOLS,
        _ => &[],
    }
}

/// Dispatch member resolution by network type (decision D10). The command
/// calls only this entry point; adding a network later means one resolver
/// plus one match arm plus one protocol entry.
pub fn resolve_members_to_articles(
    conn: &Connection,
    network_type: &NetworkType,
    member_ids: &[String],
) -> Result<Vec<ClusterArticleSummary>, AppError> {
    match network_type {
        NetworkType::CoAuthorship => resolve_authors_to_articles(conn, member_ids),
        NetworkType::CoOccurrence => resolve_terms_to_articles(conn, member_ids),
        other => Err(AppError::Validation(format!(
            "Cluster thematic analysis does not support the {other} network"
        ))),
    }
}

/// Resolve author UUIDs to included articles via `biblio_article_authors`.
pub fn resolve_authors_to_articles(
    conn: &Connection,
    author_ids: &[String],
) -> Result<Vec<ClusterArticleSummary>, AppError> {
    if author_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = placeholders_for(author_ids.len());
    let sql = format!(
        "SELECT DISTINCT a.id FROM articles a \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         WHERE a.status = 'included' AND baa.author_id IN ({placeholders})"
    );
    let ids = query_article_ids(conn, &sql, author_ids)?;
    fetch_articles_by_ids(conn, &ids)
}

/// Resolve normalized-term member ids to included articles.
///
/// MUST mirror `get_keyword_network_json`'s row collection (decision D8): the
/// keyword network merges terms from `biblio_article_terms` (metadata /
/// ai_extracted / user_added), `article_tags` (tags), and `article_labels`
/// (labels), and the view's default sources include tags and labels. A member
/// id matches a collected raw term when `normalize_term(raw_term) == id`; the
/// stored `biblio_terms.normalized_term` column is NOT the join key.
///
/// Resolving against all three sources unconditionally is correct: members
/// come only from the rendered graph, so no filter knobs (`sources`,
/// `minOccurrences`) are needed.
pub fn resolve_terms_to_articles(
    conn: &Connection,
    term_ids: &[String],
) -> Result<Vec<ClusterArticleSummary>, AppError> {
    if term_ids.is_empty() {
        return Ok(Vec::new());
    }
    let wanted: HashSet<&str> = term_ids.iter().map(String::as_str).collect();
    let mut matched: HashSet<String> = HashSet::new();

    // Source 1: biblio_article_terms (metadata / ai_extracted / user_added).
    collect_matching_article_ids(
        conn,
        "SELECT bat.article_id, bt.raw_term \
         FROM biblio_article_terms bat \
         JOIN articles a ON a.id = bat.article_id \
         JOIN biblio_terms bt ON bt.id = bat.term_id \
         WHERE a.status = 'included'",
        &wanted,
        &mut matched,
    )?;
    // Source 2: article_tags.
    collect_matching_article_ids(
        conn,
        "SELECT at.article_id, t.name \
         FROM article_tags at \
         JOIN articles a ON a.id = at.article_id \
         JOIN tags t ON t.id = at.tag_id \
         WHERE a.status = 'included'",
        &wanted,
        &mut matched,
    )?;
    // Source 3: article_labels.
    collect_matching_article_ids(
        conn,
        "SELECT al.article_id, l.name \
         FROM article_labels al \
         JOIN articles a ON a.id = al.article_id \
         JOIN labels l ON l.id = al.label_id \
         WHERE a.status = 'included'",
        &wanted,
        &mut matched,
    )?;

    let ids = matched.into_iter().collect::<Vec<_>>();
    fetch_articles_by_ids(conn, &ids)
}

/// Run a two-column `(article_id, raw_term)` query, normalizing each raw term
/// in Rust and recording the article when it matches a wanted member id.
fn collect_matching_article_ids(
    conn: &Connection,
    sql: &str,
    wanted: &HashSet<&str>,
    matched: &mut HashSet<String>,
) -> Result<(), AppError> {
    let mut stmt = conn.prepare(sql)?;
    let rows =
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    for row in rows {
        let (article_id, raw_term) = row?;
        if wanted.contains(normalize_term(&raw_term).as_str()) {
            matched.insert(article_id);
        }
    }
    Ok(())
}

fn placeholders_for(count: usize) -> String {
    vec!["?"; count].join(", ")
}

fn query_article_ids(
    conn: &Connection,
    sql: &str,
    params: &[String],
) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(sql)?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(ids)
}

/// Fetch included articles by id, mapped to `ClusterArticleSummary`.
fn fetch_articles_by_ids(
    conn: &Connection,
    ids: &[String],
) -> Result<Vec<ClusterArticleSummary>, AppError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = placeholders_for(ids.len());
    let sql = format!(
        "SELECT a.id, a.title, a.authors, a.publication_year, a.abstract_text, \
         a.doi, a.keywords, a.num_cited \
         FROM articles a WHERE a.status = 'included' AND a.id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
            Ok(ClusterArticleSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                authors: row.get(2)?,
                year: row.get(3)?,
                abstract_text: row.get(4)?,
                doi: row.get(5)?,
                keywords: row.get(6)?,
                num_cited: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Rank by `num_cited` DESC (NULLs last), then `publication_year` DESC, then
/// id ASC for determinism, and truncate to `MAX_ARTICLES_PER_CLUSTER`.
/// Returns the capped list plus the pre-cap total so the prompt can disclose
/// the truncation (decision D1).
#[must_use]
pub fn apply_top_n_cap(
    mut articles: Vec<ClusterArticleSummary>,
) -> (Vec<ClusterArticleSummary>, usize) {
    let total = articles.len();
    articles.sort_by(|a, b| {
        b.num_cited
            .unwrap_or(-1)
            .cmp(&a.num_cited.unwrap_or(-1))
            .then_with(|| b.year.unwrap_or(i32::MIN).cmp(&a.year.unwrap_or(i32::MIN)))
            .then_with(|| a.id.cmp(&b.id))
    });
    articles.truncate(MAX_ARTICLES_PER_CLUSTER);
    (articles, total)
}

/// Format an abstract for the prompt: null/empty renders as a placeholder,
/// long abstracts truncate on a word boundary with a trailing ellipsis.
#[must_use]
fn format_abstract(abstract_text: Option<&str>) -> String {
    let Some(text) = abstract_text.map(str::trim) else {
        return "(no abstract available)".to_string();
    };
    if text.is_empty() {
        return "(no abstract available)".to_string();
    }
    truncate_on_word_boundary(text, ABSTRACT_MAX_CHARS)
}

/// Truncate to `max_chars` (measured in chars) on a word boundary with a
/// trailing ellipsis; input at or below the cap passes through unchanged.
#[must_use]
fn truncate_on_word_boundary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    // Walk back from the limit to the last whitespace so the cut never lands
    // mid-word.
    let mut end = max_chars;
    while end > 0 && text.chars().nth(end - 1).is_some_and(|c| !c.is_whitespace()) {
        end -= 1;
    }
    if end == 0 {
        end = max_chars;
    }
    let prefix: String = text.chars().take(end).collect();
    format!("{}...", prefix.trim_end())
}

/// Append a `  {label}: {value}` line for an optional per-article detail.
/// Empty or whitespace-only values omit the line entirely; long values
/// truncate on a word boundary via `truncate_on_word_boundary`.
fn push_detail_line(out: &mut String, label: &str, value: Option<&str>, max_chars: usize) {
    if let Some(text) = value.map(str::trim).filter(|text| !text.is_empty()) {
        out.push_str(&format!("  {label}: {}\n", truncate_on_word_boundary(text, max_chars)));
    }
}

fn year_label(year: Option<i32>) -> String {
    match year {
        Some(year) => year.to_string(),
        None => "no year".to_string(),
    }
}

/// System prompt for cluster thematic analysis. Pure for unit-test isolation;
/// the output-structure contract mirrors the user prompt's required sections.
#[must_use]
pub fn cluster_themes_system_prompt() -> String {
    String::from(
        "You are an expert bibliometric analyst who explains what the members of a research cluster share.\n\
         Ground every claim in the provided article titles and abstracts. Never invent references.\n\
         Respond in Markdown only. Do not use the em dash character; use a plain hyphen.\n\
         Structure the answer with exactly these sections in order:\n\
         # Cluster N - Thematic Analysis\n\
         ## Overview\n\
         ## Main Themes\n\
         ## Representative Articles\n\
         In ## Main Themes, write one bullet per theme and ground each in named authors or article titles using the requested link syntax.\n\
         In ## Representative Articles, list up to 10 articles as a numbered list with the article link, year, and DOI when available.",
    )
}

/// Build the grounded user prompt. `articles` is the post-cap list;
/// `total_article_count` is the pre-cap total from `apply_top_n_cap` and
/// drives the truncation disclosure line (decision D1).
#[must_use]
pub fn build_cluster_themes_prompt(
    network_type: &NetworkType,
    cluster_index: i32,
    members: &[ClusterMember],
    articles: &[ClusterArticleSummary],
    total_article_count: usize,
) -> String {
    let member_noun =
        if matches!(network_type, NetworkType::CoAuthorship) { "authors" } else { "terms" };
    let mut out = String::new();

    out.push_str("Analyze the shared research themes of one cluster.\n\n");

    // Reference-identifier block: members (capped), each with its stable id.
    out.push_str(&format!("## Cluster members ({member_noun})\n"));
    for member in members.iter().take(MAX_MEMBERS_PER_CLUSTER) {
        out.push_str(&format!("- {} [id: {}]\n", member.label, member.id));
    }
    if members.len() > MAX_MEMBERS_PER_CLUSTER {
        out.push_str(&format!(
            "(Member list capped at the first {} of {} members.)\n",
            MAX_MEMBERS_PER_CLUSTER,
            members.len()
        ));
    }
    out.push('\n');

    // Capped articles with stable ids and truncated detail lines.
    out.push_str(&format!(
        "## Included articles ({} of {} identified for this cluster)\n",
        articles.len(),
        total_article_count
    ));
    for article in articles {
        let doi = article.doi.as_deref().map(|doi| format!(" doi:{doi}")).unwrap_or_default();
        out.push_str(&format!(
            "- {} ({}) [id: {}]{}\n",
            article.title,
            year_label(article.year),
            article.id,
            doi
        ));
        push_detail_line(&mut out, "Authors", Some(article.authors.as_str()), AUTHORS_MAX_CHARS);
        push_detail_line(&mut out, "Keywords", article.keywords.as_deref(), KEYWORDS_MAX_CHARS);
        out.push_str(&format!(
            "  Abstract: {}\n",
            format_abstract(article.abstract_text.as_deref())
        ));
    }
    out.push('\n');

    // Link protocols (data-driven, decision D10).
    out.push_str("## Reference rules\n");
    out.push_str("Wrap references in markdown links using ONLY the ids listed above:\n");
    for protocol in link_protocols_for(network_type) {
        out.push_str(&format!("- {}\n", protocol.example));
    }
    if matches!(network_type, NetworkType::CoAuthorship) {
        out.push_str("Reference cluster-member authors with author links only.\n");
    } else {
        out.push_str("Reference cluster-member terms as plain text; never emit an author link.\n");
    }
    out.push_str("Never invent an id, author, or article.\n\n");

    out.push_str("## Required output structure\n");
    out.push_str(&format!("# Cluster {} - Thematic Analysis\n", cluster_index + 1));
    out.push_str("## Overview\n");
    out.push_str("## Main Themes\n");
    out.push_str("## Representative Articles\n");
    out.push_str(
        "Number up to 10 representative articles with their article link, year, and DOI.\n",
    );

    if total_article_count > articles.len() {
        out.push('\n');
        out.push_str(
            "The article list above was capped. Start ## Overview with this exact italic line:\n",
        );
        out.push_str(&format!(
            "*Based on the {} most representative of {} included articles (ranked by citations, then recency).*\n",
            articles.len(),
            total_article_count
        ));
    }
    out
}
