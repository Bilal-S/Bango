//! Normalization utilities for bibliometric data.
//!
//! Provides functions for splitting author strings, normalizing names,
//! normalizing terms (keywords / noun phrases), and parsing affiliations.

use crate::biblio::affiliation_extractor::AffiliationExtractor;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};

static STEMMER: Lazy<Stemmer> = Lazy::new(|| Stemmer::create(Algorithm::English));

/// Snowball-stem a phrase. Each whitespace-delimited token is stemmed
/// independently and the results joined. Keeps multi-word phrases atomic
/// as a node while still normalizing inflections ("networks" → "network").
pub fn stem_phrase(text: &str) -> String {
    text.split_whitespace().map(|w| STEMMER.stem(w).to_string()).collect::<Vec<_>>().join(" ")
}

/// A parsed author with raw and normalized forms.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedAuthor {
    pub raw: String,
    pub display_name: String,
    pub normalized_name: String,
}

/// A parsed affiliation with optional institution, city, and country.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedAffiliation {
    pub institution: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
}

/// Split an author string into individual authors.
///
/// Handles common formats:
/// - JSON array: `["Smith, J", "Doe, A"]`
/// - JSON objects: `[{"name":"Smith, J"},{"name":"Doe, A"}]`
/// - Semicolon-delimited: `Smith, J; Doe, A`
/// - "and"-delimited: `Smith, J and Doe, A`
/// - Newline-delimited: `Smith, J\nDoe, A`
///
/// Strips empty entries and trims whitespace.
pub fn split_authors(authors_str: &str) -> Vec<String> {
    if authors_str.trim().is_empty() {
        return Vec::new();
    }

    // ── JSON array detection ──────────────────────────────────────
    let trimmed = authors_str.trim();
    if trimmed.starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(items) = arr.as_array() {
                let names: Vec<String> = items
                    .iter()
                    .filter_map(|item| {
                        match item {
                            // String element: "Smith, J"
                            serde_json::Value::String(s) => {
                                let t = s.trim().to_string();
                                if t.is_empty() {
                                    None
                                } else {
                                    Some(t)
                                }
                            }
                            // Object element: {"name":"Smith, J"} or {"family":"Smith","given":"John"}
                            serde_json::Value::Object(map) => {
                                // Try "name" field first
                                if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                                    let t = name.trim().to_string();
                                    if !t.is_empty() {
                                        return Some(t);
                                    }
                                }
                                // Try "family" + "given" fields (BibTeX JSON format)
                                let family =
                                    map.get("family").and_then(|v| v.as_str()).unwrap_or("");
                                let given = map.get("given").and_then(|v| v.as_str()).unwrap_or("");
                                if !family.is_empty() {
                                    if given.is_empty() {
                                        Some(family.to_string())
                                    } else {
                                        Some(format!("{}, {}", family, given))
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    })
                    .collect();
                // Return parsed names (may be empty for "[]")
                return names;
            }
        }
        // JSON parse failed - fall through to delimiter-based splitting
    }

    // Try semicolon first (most common in RIS/BibTeX)
    let authors = if authors_str.contains(';') {
        authors_str.split(';').map(|s| s.trim().to_string()).collect()
    } else if authors_str.to_lowercase().contains(" and ") {
        // Handle " and " delimiter (common in BibTeX)
        authors_str
            .split_regex_inclusive(" and ")
            .into_iter()
            .map(|s| {
                s.trim()
                    .trim_end_matches(" and ")
                    .trim_end_matches(" And ")
                    .trim_end_matches(" AND ")
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        // Try newline delimiter
        if authors_str.contains('\n') {
            authors_str
                .split('\n')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            // Single author
            vec![authors_str.trim().to_string()]
        }
    };

    authors.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Normalize an author name for deduplication.
///
/// Strategy:
/// - Lowercase
/// - Remove punctuation (commas, periods, hyphens → spaces)
/// - Collapse whitespace
/// - Take first letter of each word (to handle "Smith John" vs "Smith J" vs "Smith, J.")
/// - Actually: keep last name + first initial for best matching
pub fn normalize_author_name(raw: &str) -> String {
    let cleaned: String = raw
        .to_lowercase()
        .chars()
        .map(|c| if c == ',' || c == '.' || c == '-' || c == '_' { ' ' } else { c })
        .collect();
    let parts: Vec<&str> = cleaned.split_whitespace().collect();

    if parts.is_empty() {
        return String::new();
    }

    // If "lastname, firstname" format (comma-separated), last name is first
    if raw.contains(',') {
        // Already comma-separated: "Smith, John A" → parts = ["smith", "john", "a"]
        // Keep full last name + first initial of remaining parts
        if parts.len() >= 2 {
            let last_name = parts[0];
            let initials: String =
                parts[1..].iter().map(|p| p.chars().next().unwrap_or(' ')).collect();
            return format!("{} {}", last_name, initials.replace(' ', ""));
        }
        return parts.join(" ");
    }

    // "John Smith" or "J Smith" format
    if parts.len() >= 2 {
        let last_name = parts[parts.len() - 1];
        let initials: String =
            parts[..parts.len() - 1].iter().map(|p| p.chars().next().unwrap_or(' ')).collect();
        return format!("{} {}", last_name, initials.replace(' ', ""));
    }

    // Single word - return as-is
    parts.join(" ")
}

/// Build a display name from a raw author string.
///
/// Attempts to produce "Last, First" format.
pub fn build_display_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Already in "Last, First" format
    if trimmed.contains(',') {
        return trimmed.to_string();
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() >= 2 {
        // Assume last word is surname
        let last = parts[parts.len() - 1];
        let firsts = parts[..parts.len() - 1].join(" ");
        return format!("{}, {}", last, firsts);
    }

    trimmed.to_string()
}

/// Parse an author string into structured authors.
pub fn parse_authors(authors_str: &str) -> Vec<ParsedAuthor> {
    split_authors(authors_str)
        .into_iter()
        .map(|raw| {
            let display_name = build_display_name(&raw);
            let normalized_name = normalize_author_name(&raw);
            ParsedAuthor { raw, display_name, normalized_name }
        })
        .collect()
}

/// Normalize a term (keyword or noun phrase) for deduplication.
///
/// - Lowercase
/// - Strip trailing punctuation
/// - Collapse whitespace
pub fn normalize_term(term: &str) -> String {
    let t_trim = term.trim();
    if t_trim.is_empty() || t_trim == "[]" || t_trim == "[\"\"]" {
        return String::new();
    }
    let lower = term.to_lowercase();
    // Strip leading/trailing punctuation and brackets
    let trimmed = lower
        .trim_matches(|c: char| {
            c.is_whitespace()
                || c == ','
                || c == '.'
                || c == ';'
                || c == ':'
                || c == '['
                || c == ']'
        })
        .trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Collapse internal whitespace and filter out brackets/quotes
    let collapsed: String = trimmed.chars().fold(String::new(), |mut acc, c| {
        if c == '-' || c == '_' {
            acc.push(' ');
        } else if c != '[' && c != ']' && c != '"' && c != '\'' {
            acc.push(c);
        }
        acc
    });
    let normalized = collapse_whitespace(&collapsed);
    if normalized.is_empty() {
        return String::new();
    }
    stem_phrase(&normalized)
}

/// Split a keywords string into individual keywords.
///
/// Handles common storage formats:
/// - JSON array (the canonical storage form in the `articles.keywords` column):
///   `["Allura Red", "tartrazine"]`
/// - Semicolon-delimited: `Allura Red; tartrazine`
/// - Comma-delimited: `Allura Red, tartrazine`
///
/// Strips empty entries and trims whitespace. Falls back to delimiter-based
/// splitting when the input is not a valid JSON array.
pub fn split_keywords(keywords_str: &str) -> Vec<String> {
    let trimmed = keywords_str.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // ── JSON array detection ──────────────────────────────────────
    // The `articles.keywords` column is a JSON array of strings (written via
    // `serde_json::to_string`). Splitting it on `,` would produce broken
    // fragments like `["Allura Red"` - so parse JSON first.
    if trimmed.starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(items) = arr.as_array() {
                let kws: Vec<String> = items
                    .iter()
                    .filter_map(|item| {
                        let s = item.as_str()?;
                        let t = s.trim().to_string();
                        if t.is_empty() {
                            None
                        } else {
                            Some(t)
                        }
                    })
                    .collect();
                return kws;
            }
        }
        // JSON parse failed - fall through to delimiter-based splitting
    }

    // ── Delimiter-based fallback (RIS/plain-text) ─────────────────
    keywords_str
        .split(';')
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Sanitize a raw term for storage in `biblio_terms.raw_term`.
///
/// Strips brackets, quotes, and stray JSON artifacts so the stored display
/// value contains only the human-readable word(s). Defense-in-depth against
/// malformed input - the canonical cleaning happens in `split_keywords`.
pub fn sanitize_raw_term(term: &str) -> String {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Remove brackets, double/single quotes anywhere in the string.
    let cleaned: String =
        trimmed.chars().filter(|&c| c != '[' && c != ']' && c != '"' && c != '\'').collect();
    // Strip leading/trailing punctuation that may remain after quote removal
    // (e.g., a leading `"` consumed by filter leaves a trailing `,`).
    let stripped = cleaned
        .trim_matches(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';' || c == ':')
        .to_string();
    // Collapse internal whitespace
    collapse_whitespace(&stripped)
}

/// Deduplicate a list of terms by their normalized form.
/// Returns unique terms preserving first-encountered order.
pub fn dedup_terms(terms: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for term in terms {
        let norm = normalize_term(term);
        if !norm.is_empty() && seen.insert(norm) {
            result.push(term.clone());
        }
    }
    result
}

#[must_use]
fn is_us_state(part: &str) -> bool {
    let trimmed = part.to_lowercase().trim().to_string();
    if trimmed.len() == 2 {
        let states = [
            "al", "ak", "az", "ar", "ca", "co", "ct", "de", "fl", "ga", "hi", "id", "il", "in",
            "ia", "ks", "ky", "la", "me", "md", "ma", "mi", "mn", "ms", "mo", "mt", "ne", "nv",
            "nh", "nj", "nm", "ny", "nc", "nd", "oh", "ok", "or", "pa", "ri", "sc", "sd", "tn",
            "tx", "ut", "vt", "va", "wa", "wv", "wi", "wy", "dc",
        ];
        return states.contains(&trimmed.as_str());
    }
    if trimmed.len() >= 5 {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() == 2 && parts[0].len() == 2 {
            let states = [
                "al", "ak", "az", "ar", "ca", "co", "ct", "de", "fl", "ga", "hi", "id", "il", "in",
                "ia", "ks", "ky", "la", "me", "md", "ma", "mi", "mn", "ms", "mo", "mt", "ne", "nv",
                "nh", "nj", "nm", "ny", "nc", "nd", "oh", "ok", "or", "pa", "ri", "sc", "sd", "tn",
                "tx", "ut", "vt", "va", "wa", "wv", "wi", "wy", "dc",
            ];
            if states.contains(&parts[0]) && parts[1].chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    let full_states = [
        "alabama",
        "alaska",
        "arizona",
        "arkansas",
        "california",
        "colorado",
        "connecticut",
        "delaware",
        "florida",
        "georgia",
        "hawaii",
        "idaho",
        "illinois",
        "indiana",
        "iowa",
        "kansas",
        "kentucky",
        "louisiana",
        "maine",
        "maryland",
        "massachusetts",
        "michigan",
        "minnesota",
        "mississippi",
        "missouri",
        "montana",
        "nebraska",
        "nevada",
        "new hampshire",
        "new jersey",
        "new mexico",
        "new york",
        "north carolina",
        "north dakota",
        "ohio",
        "oklahoma",
        "oregon",
        "pennsylvania",
        "rhode island",
        "south carolina",
        "south dakota",
        "tennessee",
        "texas",
        "utah",
        "vermont",
        "virginia",
        "washington",
        "west virginia",
        "wisconsin",
        "wyoming",
        "district of columbia",
    ];
    full_states.contains(&trimmed.as_str())
}

#[must_use]
fn is_department_part(part: &str) -> bool {
    let p = part.to_lowercase();
    p.starts_with("dept ")
        || p.starts_with("dept. ")
        || p.starts_with("dept of ")
        || p.starts_with("dept. of ")
        || p.starts_with("department ")
        || p.starts_with("department of ")
        || p.starts_with("division of ")
        || p.starts_with("faculty of ")
        || p.starts_with("lab of ")
        || p.starts_with("laboratory of ")
        || p.starts_with("school of ")
            && (p.contains("science")
                || p.contains("engineering")
                || p.contains("medicine")
                || p.contains("art")
                || p.contains("business")
                || p.contains("law"))
}

/// Parse an affiliation string into institution, city, and country.
///
/// Common formats:
/// - "MIT, Cambridge, MA, USA"
/// - "Department of CS, Stanford University, Stanford, CA"
/// - "University of Oxford, Oxford, United Kingdom"
#[must_use]
pub fn parse_affiliation(affiliation: &str) -> ParsedAffiliation {
    parse_affiliation_with_extractor(affiliation, None)
}

#[must_use]
pub fn parse_affiliation_with_extractor(
    affiliation: &str,
    extractor: Option<&AffiliationExtractor>,
) -> ParsedAffiliation {
    if affiliation.trim().is_empty() {
        return ParsedAffiliation::default();
    }

    let parts: Vec<&str> =
        affiliation.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

    if parts.is_empty() {
        return ParsedAffiliation::default();
    }

    // Single part: treat as institution only
    if parts.len() == 1 {
        return ParsedAffiliation {
            institution: Some(parts[0].to_string()),
            city: None,
            country: None,
        };
    }

    let mut country_str: Option<String> = None;
    let mut city_str: Option<String> = None;
    let mut is_us = false;
    let mut used_parts = 0;

    // 1. Country Check
    if let Some(&last_part) = parts.last() {
        let last_lower = last_part.to_lowercase();
        if last_lower == "usa"
            || last_lower == "united states"
            || last_lower == "united states of america"
            || last_lower == "u.s.a."
            || last_lower == "u.s."
            || last_lower == "us"
        {
            country_str = Some("USA".to_string());
            is_us = true;
            used_parts += 1;
        } else if last_lower == "uk"
            || last_lower == "united kingdom"
            || last_lower == "u.k."
            || last_lower == "england"
            || last_lower == "scotland"
            || last_lower == "wales"
            || last_lower == "great britain"
            || last_lower == "gb"
        {
            country_str = Some("United Kingdom".to_string());
            used_parts += 1;
        } else {
            let common_countries = [
                "germany",
                "france",
                "china",
                "japan",
                "australia",
                "canada",
                "italy",
                "spain",
                "netherlands",
                "switzerland",
                "sweden",
                "norway",
                "finland",
                "denmark",
                "singapore",
                "south korea",
                "india",
                "brazil",
                "south africa",
                "belgium",
                "austria",
                "new zealand",
            ];
            for &c in &common_countries {
                if last_lower == c {
                    country_str = Some(c[..1].to_uppercase() + &c[1..]);
                    used_parts += 1;
                    break;
                }
            }
        }
    }

    // 2. City & State extraction based on parts length and country
    if country_str.is_some() {
        if parts.len() >= 3 {
            if is_us {
                let state_candidate = parts[parts.len() - 2];
                if is_us_state(state_candidate) {
                    used_parts += 1;
                    if parts.len() >= 4 {
                        city_str = Some(parts[parts.len() - 3].to_string());
                        used_parts += 1;
                    }
                } else {
                    city_str = Some(state_candidate.to_string());
                    used_parts += 1;
                }
            } else {
                // Non-US country, assume second-to-last is city
                city_str = Some(parts[parts.len() - 2].to_string());
                used_parts += 1;
            }
        }
    } else {
        // No recognized country yet. Check if last part is a US state.
        if parts.len() >= 2 {
            let last_part = parts[parts.len() - 1];
            if is_us_state(last_part) {
                country_str = Some("USA".to_string());
                used_parts += 1;
                if parts.len() >= 3 {
                    city_str = Some(parts[parts.len() - 2].to_string());
                    used_parts += 1;
                }
            } else if parts.len() >= 3 {
                // Naive fallback for unrecognized country
                city_str = Some(parts[parts.len() - 2].to_string());
                country_str = Some(last_part.to_string());
                used_parts += 2;
            } else {
                // parts.len() == 2, unrecognized country. Treat last as country, first as institution.
                country_str = Some(last_part.to_string());
                used_parts += 1;
            }
        }
    }

    // 3. Institution Extraction & Department Filtering
    let remaining_len = parts.len().saturating_sub(used_parts);
    let institution = if remaining_len > 0 {
        let joined_remaining = parts[..remaining_len].join(", ");
        let mut extracted = None;
        if let Some(ext) = extractor {
            extracted = ext.extract(&joined_remaining);
        }
        extracted.or_else(|| {
            let inst_parts: Vec<&str> =
                parts[..remaining_len].iter().copied().filter(|p| !is_department_part(p)).collect();
            if inst_parts.is_empty() {
                // Fallback if filtering left nothing
                Some(joined_remaining.clone())
            } else {
                Some(inst_parts.join(", "))
            }
        })
    } else {
        Some(parts[0].to_string())
    };

    ParsedAffiliation { institution, city: city_str, country: country_str }
}

/// Collapse multiple whitespace chars into a single space.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Trait helper for split with regex-free " and " handling
// ---------------------------------------------------------------------------

trait SplitInclusive {
    fn split_regex_inclusive<'a>(&'a self, delimiter: &'a str) -> Vec<&'a str>;
}

impl SplitInclusive for str {
    fn split_regex_inclusive<'a>(&'a self, delimiter: &'a str) -> Vec<&'a str> {
        let mut result = Vec::new();
        let mut start = 0;
        let lower = self.to_lowercase();
        let delim_lower = delimiter.to_lowercase();
        while let Some(pos) = lower[start..].find(&delim_lower) {
            let abs_pos = start + pos;
            result.push(&self[start..abs_pos]);
            start = abs_pos + delimiter.len();
        }
        if start < self.len() {
            result.push(&self[start..]);
        }
        result
    }
}
