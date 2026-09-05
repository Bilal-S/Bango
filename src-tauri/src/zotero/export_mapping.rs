//! Pure Bango article -> Zotero item JSON mapping (export). Reverse of the
//! import table; every helper is `#[must_use]` and unit-tested on every
//! platform. `labels` and Bango-internal fields are never exported; user
//! notes become Zotero child-note items (see `build_note_item_json`).

use std::collections::HashSet;

use crate::models::article::Article;
use crate::ris::doi::normalize_doi;

/// A partially-known calendar date parsed from a bibliographic date string.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartialDate {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

/// Month name -> number (full names and 3-letter abbreviations,
/// case-insensitive, optional trailing period). `None` for non-month words.
fn month_from_name(token: &str) -> Option<u32> {
    let name = token.trim().trim_end_matches('.').to_ascii_lowercase();
    match name.as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

/// Tolerantly parse the date shapes that actually occur in RIS `DA` fields
/// and Zotero `data.date` strings: ISO (`2016-09-26`, `2025-11`, `2025`),
/// `Mon DD` (`NOV 25`), month-only (`APR`), month ranges (`JUL-AUG` -> the
/// first month), `MM/YYYY` (`02/2017`), `YYYY/MM/DD`, and `Mon YYYY`
/// (`April 1957`). Alphabetic and numeric runs are tokens; everything else
/// is a separator. Unknown tokens are ignored. A 4-digit number in
/// 1000..=3000 is a year; a month name wins over ambiguous 1-12 numbers
/// (a second such number then reads as the day, e.g. `5/6` -> May 6).
#[must_use]
pub fn parse_partial_date(raw: &str) -> PartialDate {
    let mut partial = PartialDate::default();
    let mut ambiguous: Vec<u32> = Vec::new();
    for token in raw.split(|c: char| !c.is_ascii_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            let Ok(value) = token.parse::<u32>() else {
                continue;
            };
            if token.len() == 4 && (1000..=3000).contains(&value) {
                if partial.year.is_none() {
                    partial.year = Some(value as i32);
                }
            } else if (1..=12).contains(&value) {
                ambiguous.push(value);
            } else if (13..=31).contains(&value) && partial.day.is_none() {
                partial.day = Some(value);
            }
        } else if partial.month.is_none() {
            partial.month = month_from_name(token);
        }
    }
    if partial.month.is_none() {
        partial.month = ambiguous.first().copied();
        if partial.month.is_some() && partial.day.is_none() {
            partial.day = ambiguous.get(1).copied();
        }
    }
    partial
}

/// Build the Zotero `date` value: the most specific ISO form Zotero parses
/// exactly (`YYYY-MM-DD`, `YYYY-MM`, `YYYY`). Month/day come from the raw
/// `date` string (tolerantly parsed); the year prefers the authoritative
/// `publication_year` (RIS `PY`) and falls back to a year parsed from the
/// string. Raw strings must never be sent as-is: Zotero re-parses them, so
/// `"NOV 25"` displays as "Nov 25" with no year (ISO partial dates survive).
#[must_use]
pub fn build_export_date(date: Option<&str>, publication_year: Option<i32>) -> Option<String> {
    let partial = date
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map_or_else(PartialDate::default, parse_partial_date);
    let year = publication_year.filter(|y| *y > 0).or(partial.year)?;
    let Some(month) = partial.month else {
        return Some(format!("{year:04}"));
    };
    partial
        .day
        .map_or_else(
            || format!("{year:04}-{month:02}"),
            |day| format!("{year:04}-{month:02}-{day:02}"),
        )
        .into()
}

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

/// One block of the merged Bango user-notes format: a title line, a `---`
/// separator line, then the body text. The Zotero import writes this format
/// (one block per Zotero child note); the export splits it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteBlock {
    pub title: String,
    pub body: String,
}

/// Split merged Bango user notes back into note blocks. Paragraphs (blank-line
/// separated) whose second line is `---` round-trip as their own blocks; when
/// no paragraph matches the separator format (free-form notes typed in Bango),
/// the whole text becomes a single block whose title is the first line.
/// Non-matching paragraphs among matching ones (user edits) each become one
/// block. Empty/whitespace-only input yields no blocks.
#[must_use]
pub fn split_note_blocks(text: &str) -> Vec<NoteBlock> {
    let normalized = text.replace("\r\n", "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut paragraphs: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in trimmed.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
    let matches = |lines: &[&str]| lines.len() >= 2 && lines[1].trim() == "---";
    if !paragraphs.iter().any(|p| matches(p)) {
        // Free-form text: one block, the first line acts as the title.
        let mut lines = trimmed.lines();
        let title = lines.next().unwrap_or_default().trim().to_string();
        let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
        return vec![NoteBlock { title, body }];
    }
    paragraphs
        .into_iter()
        .map(|lines| {
            if matches(&lines) {
                NoteBlock {
                    title: lines[0].trim().to_string(),
                    body: lines[2..].join("\n").trim().to_string(),
                }
            } else {
                NoteBlock {
                    title: lines[0].trim().to_string(),
                    body: lines[1..].join("\n").trim().to_string(),
                }
            }
        })
        .collect()
}

/// Escape the five HTML-significant characters.
#[must_use]
pub fn escape_note_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Build one Zotero child-note item JSON for a note block. The Zotero `note`
/// field is HTML, so the plain-text title + body lines are escaped and joined
/// with `<br/>` (the first line is the note's display title in Zotero).
#[must_use]
pub fn build_note_item_json(parent_key: &str, block: &NoteBlock) -> serde_json::Value {
    let mut lines: Vec<String> = vec![escape_note_html(&block.title)];
    if !block.body.is_empty() {
        lines.extend(block.body.lines().map(escape_note_html));
    }
    serde_json::json!({
        "itemType": "note",
        "parentItem": parent_key,
        "note": lines.join("<br/>"),
        "tags": [],
    })
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
    // date: the most specific ISO form Zotero parses exactly (year-month-day
    // / year-month / year). Raw RIS strings like "NOV 25" must never be sent
    // as-is - Zotero re-parses them into a month/day with no year.
    if let Some(date) = build_export_date(article.date.as_deref(), article.publication_year) {
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

/// Attachment display title / upload filename convention: the last name of
/// the first author, a dash, then the article title capped at 30 chars cut
/// at a word boundary, plus the extension (e.g.
/// "Jones - The awakening of sund.pdf"). A single-token (institutional)
/// first author is used verbatim; no author drops the prefix; an empty
/// title falls back to "Untitled". A leading dot on the extension is
/// normalized away.
#[must_use]
pub fn build_attachment_title(authors: &[String], title: &str, ext: &str) -> String {
    let title_part = {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            "Untitled".to_string()
        } else {
            truncate_prose_at_word_boundary(trimmed, 30)
        }
    };
    let ext = ext.trim_start_matches('.');
    match first_author_lastname(authors) {
        Some(last) => format!("{last} - {title_part}.{ext}"),
        None => format!("{title_part}.{ext}"),
    }
}

/// Last name of the first "Lastname, Firstname" author; a single-token
/// (institutional) name is used verbatim; blank entries are skipped.
fn first_author_lastname(authors: &[String]) -> Option<String> {
    let first = authors.iter().find(|a| !a.trim().is_empty())?;
    let first = first.trim();
    Some(
        match first.split_once(',') {
            Some((last, _)) => last.trim(),
            None => first,
        }
        .to_string(),
    )
}

/// Cut prose at the last space within `max_chars` (never mid-word); a single
/// word longer than the limit hard-cuts. Char-based, not byte-based.
fn truncate_prose_at_word_boundary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    match truncated.rfind(' ') {
        // Callers trim, so a space at index 0 cannot happen; guard anyway.
        Some(idx) if idx > 0 => truncated[..idx].to_string(),
        _ => truncated,
    }
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
