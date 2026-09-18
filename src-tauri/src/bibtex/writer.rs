//! BibTeX export writer: maps the shared export projection (`RisExportArticle`)
//! to `.bib` entries. Pure (`#[must_use]`); mirrors the import mappings in
//! `converter.rs` (`bibtex_to_ris_record`) in reverse.

use std::collections::HashMap;

use crate::export::ris_writer::RisExportArticle;
use crate::ris::doi::normalize_doi;

/// Reverse of `converter::map_entry_type`: RIS-style reference type to BibTeX
/// entry type. Unknown or missing types default to `article` (mirroring the
/// `TY  - JOUR` default on the RIS side).
#[must_use]
pub fn map_reference_type(reference_type: Option<&str>) -> String {
    match reference_type.map(str::to_ascii_lowercase).as_deref() {
        None | Some("jour") | Some("article") => "article".to_string(),
        Some("conference") => "inproceedings".to_string(),
        Some(other) => other.to_string(),
    }
}

/// Escape BibTeX metacharacters in field values: braces are escaped; percent
/// and backslash are left as-is per BibTeX convention.
#[must_use]
pub fn escape_field(value: &str) -> String {
    value.replace('{', "\\{").replace('}', "\\}")
}

fn fold_char(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

/// Fold common Latin-1 accented letters to their ASCII base letter so
/// citation keys stay portable across BibTeX toolchains.
#[must_use]
pub fn fold_ascii(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

/// Lowercase, fold to ASCII, and keep alphanumerics only (key fragments).
fn sanitize_key_fragment(text: &str) -> String {
    fold_ascii(&text.to_lowercase()).chars().filter(char::is_ascii_alphanumeric).collect()
}

/// Citation key: `{surname}{year}{title-word}` (e.g. `smith2023sugar`).
/// Authorless entries use `anon`, yearless entries `nd`; the surname fallback
/// guarantees a non-empty key.
#[must_use]
pub fn make_citation_key(article: &RisExportArticle) -> String {
    citation_key_from_parts(&article.authors, article.publication_year, &article.title)
}

/// Citation key from raw parts: `{surname}{year}{title-word}`. Shared by the
/// BibTeX writer and the Obsidian vault export's article page slugs so both
/// surfaces use one key convention. Same `anon`/`nd` fallbacks as
/// `make_citation_key`.
#[must_use]
pub fn citation_key_from_parts(authors: &[String], year: Option<i32>, title: &str) -> String {
    let surname = sanitize_key_fragment(&first_author_surname(authors));
    let surname = if surname.is_empty() { "anon".to_string() } else { surname };
    let year = year.map_or_else(|| "nd".to_string(), |y| y.to_string());
    let title_word = first_significant_title_word(title);
    format!("{surname}{year}{title_word}")
}

/// Assign unique keys across one file: the second occurrence of a key appends
/// `b`, the third `c`, and so on (BibTeX letter-suffix convention, capped at
/// `z` - beyond 26 collisions per key the suffix stays `z`). `pub` so the
/// Obsidian vault export can dedupe its article slugs with the same rule.
pub fn dedupe_keys(keys: &[String]) -> Vec<String> {
    let mut counts: HashMap<&str, u32> = HashMap::new();
    keys.iter()
        .map(|key| {
            let n = counts.entry(key.as_str()).or_insert(0);
            *n += 1;
            if *n == 1 {
                (*key).to_string()
            } else {
                let offset = u8::try_from(*n - 1).unwrap_or(25).min(25);
                format!("{key}{}", char::from(b'a' + offset))
            }
        })
        .collect()
}

/// One `  name = {value}` line; `None` for absent/blank values.
fn field_line(name: &str, value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| format!("  {name} = {{{}}}", escape_field(trimmed)))
}

/// `start--end` when both exist, the lone start page verbatim (elocation ids
/// like `e12345`), `None` when there is no page data.
#[must_use]
pub fn format_pages(start: Option<&str>, end: Option<&str>) -> Option<String> {
    let start = start.map(str::trim).filter(|s| !s.is_empty());
    let end = end.map(str::trim).filter(|s| !s.is_empty());
    match (start, end) {
        (Some(s), Some(e)) => Some(format!("{s}--{e}")),
        (Some(s), None) => Some(s.to_string()),
        (None, _) => None,
    }
}

/// Join trimmed non-empty parts (skips blank keyword/tag entries).
fn join_non_empty(parts: &[String], sep: &str) -> String {
    parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(sep)
}

/// Keyword list with full RIS `KW` parity: plain keywords first, then
/// `Bango:`-prefixed tags and labels (same prefixed spelling as `article_to_ris`).
fn bibtex_keywords(article: &RisExportArticle) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.extend(article.keywords.iter().cloned());
    parts.extend(article.tags.iter().map(|t| format!("Bango:{t}")));
    parts.extend(article.labels.iter().map(|l| format!("Bango:{l}")));
    join_non_empty(&parts, ", ")
}

/// `bango-criteria` mirror of the RIS `C8` payload. Emitted raw (not
/// brace-escaped): the JSON braces are machine-generated and balanced, so
/// they are valid BibTeX and round-trip through the parser; the blanket
/// brace-escape rule stays for arbitrary user text. `None` when no criteria
/// matched (mirroring the RIS writer's conditional C8).
fn bibtex_criteria_line(article: &RisExportArticle) -> Option<String> {
    if article.matched_inclusion_criteria.is_empty()
        && article.matched_exclusion_criteria.is_empty()
    {
        return None;
    }
    let inc = serde_json::to_string(&article.matched_inclusion_criteria).unwrap_or_default();
    let exc = serde_json::to_string(&article.matched_exclusion_criteria).unwrap_or_default();
    let payload = format!("{{\"inc\":{inc},\"exc\":{exc}}}");
    Some(format!("  bango-criteria = {{{payload}}}"))
}

/// Render one export article as a BibTeX entry (two-space indent, braced
/// values, trailing newline) with full RIS field parity: every field the RIS
/// writer emits is attempted here (abstract, language, issn, tags/labels as
/// prefixed keywords, AI reasoning in `comment`, user notes in `note`, the
/// C8 criteria payload as `bango-criteria`). Optional fields are omitted when
/// absent; imported notes land in `annote` because the standard styles
/// ignore it.
#[must_use]
pub fn article_to_bibtex(article: &RisExportArticle, key: &str) -> String {
    let entry_type = map_reference_type(article.reference_type.as_deref());
    let mut lines = vec![format!("@{entry_type}{{{key},")];

    lines.extend(field_line("author", &join_non_empty(&article.authors, " and ")));
    lines.extend(field_line("title", &article.title));
    lines.extend(field_line("abstract", &article.abstract_text));
    lines.extend(field_line("journal", article.journal.as_deref().unwrap_or("")));
    if let Some(year) = article.publication_year {
        lines.extend(field_line("year", &year.to_string()));
    }
    lines.extend(field_line("volume", article.volume.as_deref().unwrap_or("")));
    lines.extend(field_line("number", article.issue.as_deref().unwrap_or("")));
    lines.extend(
        format_pages(article.start_page.as_deref(), article.end_page.as_deref())
            .and_then(|p| field_line("pages", &p)),
    );
    lines.extend(normalize_doi(article.doi.as_deref()).and_then(|d| field_line("doi", &d)));
    lines.extend(field_line("keywords", &bibtex_keywords(article)));
    lines.extend(field_line("annote", article.notes.as_deref().unwrap_or("")));
    lines.extend(field_line("comment", article.ai_reasoning.as_deref().unwrap_or("")));
    lines.extend(field_line("note", article.user_notes.as_deref().unwrap_or("")));
    lines.extend(field_line("url", article.url.as_deref().unwrap_or("")));
    lines.extend(field_line("language", article.language.as_deref().unwrap_or("")));
    lines.extend(field_line("publisher", article.publisher.as_deref().unwrap_or("")));
    lines.extend(field_line("issn", article.issn.as_deref().unwrap_or("")));
    lines.extend(bibtex_criteria_line(article));

    lines.push("}".to_string());
    lines.join("\n") + "\n"
}

/// Render all export articles as a `.bib` file body (UTF-8, one entry per
/// article, citation keys deduplicated with letter suffixes).
#[must_use]
pub fn articles_to_bibtex(articles: &[RisExportArticle]) -> String {
    let keys: Vec<String> = articles.iter().map(make_citation_key).collect();
    let keys = dedupe_keys(&keys);
    articles.iter().zip(keys).map(|(a, k)| article_to_bibtex(a, &k)).collect()
}

/// Family-name heuristic for citation keys: text before the first comma
/// ("Smith, John A."), otherwise the last whitespace token ("John A. Smith").
/// Empty when no author string has content.
#[must_use]
pub fn first_author_surname(authors: &[String]) -> String {
    let Some(first) = authors.iter().map(String::as_str).map(str::trim).find(|a| !a.is_empty())
    else {
        return String::new();
    };
    if let Some((before, _)) = first.split_once(',') {
        let surname = before.trim();
        if !surname.is_empty() {
            return surname.to_string();
        }
    }
    first.split_whitespace().next_back().map(str::to_string).unwrap_or_default()
}

/// First title word worth keying on: skips pure-symbol tokens and leading
/// articles (a/an/the).
fn first_significant_title_word(title: &str) -> String {
    title
        .split_whitespace()
        .map(sanitize_key_fragment)
        .find(|w| !w.is_empty() && !matches!(w.as_str(), "a" | "an" | "the"))
        .unwrap_or_default()
}
