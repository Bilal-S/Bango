//! Pure Bango article -> Zotero item JSON mapping (export). Reverse of the
//! import table; every helper is `#[must_use]` and unit-tested on every
//! platform. `user_notes`, labels, and Bango-internal fields are never
//! exported.

use std::collections::HashSet;

use crate::models::article::Article;
use crate::ris::doi::normalize_doi;

/// Reverse RIS TY -> itemType table; unknown or `None` -> `journalArticle`.
#[must_use]
pub fn map_ris_type_to_item_type(ris_type: Option<&str>) -> &'static str {
    match ris_type.map(str::to_ascii_uppercase).as_deref() {
        Some("CONF") => "conferencePaper",
        Some("BOOK") => "book",
        Some("CHAP") => "bookSection",
        Some("THES") => "thesis",
        Some("RPRT") => "report",
        Some("GEN") => "document",
        Some("ENCYC") => "encyclopediaArticle",
        Some("DICT") => "dictionaryEntry",
        Some("NEWS") => "newspaperArticle",
        Some("MGZN") => "magazineArticle",
        _ => "journalArticle",
    }
}

/// Split a "Lastname, Firstname" author string into Zotero creator fields.
/// Single-token names become `{name}`; malformed (empty) entries are dropped.
#[must_use]
pub fn map_creators_for_export(authors: &[String]) -> Vec<serde_json::Value> {
    let mut creators = Vec::new();
    for author in authors {
        let trimmed = author.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((last, first)) = trimmed.split_once(',') {
            let last = last.trim();
            let first = first.trim();
            if last.is_empty() && first.is_empty() {
                continue;
            }
            creators.push(serde_json::json!({
                "creatorType": "author",
                "firstName": first,
                "lastName": last,
            }));
        } else {
            creators.push(serde_json::json!({
                "creatorType": "author",
                "name": trimmed,
            }));
        }
    }
    creators
}

/// Join start/end pages as Zotero `pages` ("1-10").
#[must_use]
pub fn join_pages(start: Option<&str>, end: Option<&str>) -> Option<String> {
    match (start.filter(|s| !s.is_empty()), end.filter(|s| !s.is_empty())) {
        (Some(s), Some(e)) => Some(format!("{s}-{e}")),
        (Some(s), None) => Some(s.to_string()),
        (None, Some(e)) => Some(e.to_string()),
        (None, None) => None,
    }
}

/// Merge article tags + keywords into Zotero `tags` entries,
/// case-insensitively deduped, order-preserving.
#[must_use]
pub fn merge_tags(tags: &[String], keywords: &[String]) -> Vec<serde_json::Value> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for tag in tags.iter().chain(keywords.iter()) {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_lowercase()) {
            out.push(serde_json::json!({ "tag": trimmed }));
        }
    }
    out
}

/// Build the Zotero item JSON for one article. Journal-specific fields
/// (`publicationTitle`, `ISSN`) are emitted for `journalArticle` only;
/// `volume`/`issue` for journal + conference types. Every other type gets the
/// common subset.
#[must_use]
pub fn build_item_json(article: &Article, collection_key: &str) -> serde_json::Value {
    let item_type = map_ris_type_to_item_type(article.reference_type.as_deref());
    let mut data = serde_json::Map::new();
    data.insert("itemType".into(), serde_json::json!(item_type));
    data.insert("title".into(), serde_json::json!(article.title));
    if !article.abstract_text.trim().is_empty() {
        data.insert("abstractNote".into(), serde_json::json!(article.abstract_text));
    }
    data.insert(
        "creators".into(),
        serde_json::Value::Array(map_creators_for_export(&article.authors)),
    );
    // date: raw string when present, else the year.
    let date = article
        .date
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(str::to_string)
        .or_else(|| article.publication_year.map(|y| y.to_string()));
    if let Some(date) = date {
        data.insert("date".into(), serde_json::json!(date));
    }
    // DOI in canonical form; Zotero matches imports case-insensitively anyway.
    if let Some(doi) = normalize_doi(article.doi.as_deref()) {
        data.insert("DOI".into(), serde_json::json!(doi));
    }
    if item_type == "journalArticle" {
        if let Some(journal) = article.journal.as_deref().filter(|j| !j.is_empty()) {
            data.insert("publicationTitle".into(), serde_json::json!(journal));
        }
        if let Some(issn) = article.issn.as_deref().filter(|i| !i.is_empty()) {
            data.insert("ISSN".into(), serde_json::json!(issn));
        }
    }
    if item_type == "journalArticle" || item_type == "conferencePaper" {
        if let Some(volume) = article.volume.as_deref().filter(|v| !v.is_empty()) {
            data.insert("volume".into(), serde_json::json!(volume));
        }
        if let Some(issue) = article.issue.as_deref().filter(|i| !i.is_empty()) {
            data.insert("issue".into(), serde_json::json!(issue));
        }
    }
    if let Some(pages) = join_pages(article.start_page.as_deref(), article.end_page.as_deref()) {
        data.insert("pages".into(), serde_json::json!(pages));
    }
    if let Some(url) = article.url.as_deref().filter(|u| !u.is_empty()) {
        data.insert("url".into(), serde_json::json!(url));
    }
    if let Some(language) = article.language.as_deref().filter(|l| !l.is_empty()) {
        data.insert("language".into(), serde_json::json!(language));
    }
    if let Some(publisher) = article.publisher.as_deref().filter(|p| !p.is_empty()) {
        data.insert("publisher".into(), serde_json::json!(publisher));
    }
    if let Some(place) = article.publisher_city.as_deref().filter(|p| !p.is_empty()) {
        data.insert("place".into(), serde_json::json!(place));
    }
    // notes -> extra (plain text). user_notes and labels never export.
    if let Some(notes) = article.notes.as_deref().filter(|n| !n.trim().is_empty()) {
        data.insert("extra".into(), serde_json::json!(notes));
    }
    data.insert(
        "tags".into(),
        serde_json::Value::Array(merge_tags(&article.tags, &article.keywords)),
    );
    data.insert("collections".into(), serde_json::json!([collection_key]));
    serde_json::Value::Object(data)
}

/// How an article compares against the target Zotero collection by canonical
/// DOI. Placeholder DOIs normalize to `None` -> `NoDoi` (skipped + counted,
/// never matched).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportArticleClass {
    Missing,
    AlreadyPresent,
    NoDoi,
}

/// Classify scoped articles against the collection's canonical DOI set.
#[must_use]
pub fn classify_export_articles<'a>(
    articles: &'a [Article],
    collection_dois: &HashSet<String>,
) -> Vec<(&'a Article, ExportArticleClass)> {
    articles
        .iter()
        .map(|article| {
            let class = match normalize_doi(article.doi.as_deref()) {
                Some(doi) if collection_dois.contains(&doi) => ExportArticleClass::AlreadyPresent,
                Some(_) => ExportArticleClass::Missing,
                None => ExportArticleClass::NoDoi,
            };
            (article, class)
        })
        .collect()
}
