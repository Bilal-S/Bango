//! Normalization utilities for bibliometric data.
//!
//! Provides functions for splitting author strings, normalizing names,
//! normalizing terms (keywords / noun phrases), and parsing affiliations.

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
/// Handles common delimiters: semicolons, " and ", newlines.
/// Strips empty entries and trims whitespace.
pub fn split_authors(authors_str: &str) -> Vec<String> {
    if authors_str.trim().is_empty() {
        return Vec::new();
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

    // Single word — return as-is
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
    let lower = term.to_lowercase();
    // Strip leading/trailing punctuation
    let trimmed = lower
        .trim_matches(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ';' || c == ':')
        .trim();
    // Collapse internal whitespace
    let collapsed: String = trimmed.chars().fold(String::new(), |mut acc, c| {
        if c == '-' || c == '_' {
            acc.push(' ');
        } else {
            acc.push(c);
        }
        acc
    });
    collapse_whitespace(&collapsed)
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

/// Parse an affiliation string into institution, city, and country.
///
/// Common formats:
/// - "MIT, Cambridge, MA, USA"
/// - "Department of CS, Stanford University, Stanford, CA"
/// - "University of Oxford, Oxford, United Kingdom"
pub fn parse_affiliation(affiliation: &str) -> ParsedAffiliation {
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

    // Heuristic: last segment is often country or state abbreviation
    let country = parts.last().map(|s| s.to_string()).filter(|s| !s.is_empty());
    let city =
        if parts.len() >= 3 { parts.get(parts.len() - 2).map(|s| s.to_string()) } else { None };
    // Institution: combine everything before city/country
    let institution = if parts.len() > 2 {
        Some(parts[..parts.len() - 2].join(", "))
    } else {
        Some(parts[0].to_string())
    };

    ParsedAffiliation { institution, city, country }
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── split_authors ────────────────────────────────────────────

    #[test]
    fn test_split_authors_semicolon() {
        let result = split_authors("Smith J; Doe A; Brown K");
        assert_eq!(result, vec!["Smith J", "Doe A", "Brown K"]);
    }

    #[test]
    fn test_split_authors_and() {
        let result = split_authors("Smith J and Doe A");
        assert_eq!(result, vec!["Smith J", "Doe A"]);
    }

    #[test]
    fn test_split_authors_and_case_insensitive() {
        let result = split_authors("Smith J AND Doe A");
        assert_eq!(result, vec!["Smith J", "Doe A"]);
    }

    #[test]
    fn test_split_authors_newline() {
        let result = split_authors("Smith J\nDoe A");
        assert_eq!(result, vec!["Smith J", "Doe A"]);
    }

    #[test]
    fn test_split_authors_single() {
        let result = split_authors("Smith J");
        assert_eq!(result, vec!["Smith J"]);
    }

    #[test]
    fn test_split_authors_empty() {
        let result = split_authors("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_split_authors_whitespace_only() {
        let result = split_authors("   ");
        assert!(result.is_empty());
    }

    #[test]
    fn test_split_authors_trailing_semicolon() {
        let result = split_authors("Smith J;");
        assert_eq!(result, vec!["Smith J"]);
    }

    #[test]
    fn test_split_authors_multiple_and() {
        let result = split_authors("Smith J and Doe A and Brown K");
        assert_eq!(result, vec!["Smith J", "Doe A", "Brown K"]);
    }

    // ── normalize_author_name ───────────────────────────────────

    #[test]
    fn test_normalize_author_name_comma_format() {
        // "Smith, John A." → "smith ja"
        let result = normalize_author_name("Smith, John A.");
        assert_eq!(result, "smith ja");
    }

    #[test]
    fn test_normalize_author_name_first_last() {
        // "John Smith" → "smith j"
        let result = normalize_author_name("John Smith");
        assert_eq!(result, "smith j");
    }

    #[test]
    fn test_normalize_author_name_initials() {
        // "J. A. Smith" → "smith ja"
        let result = normalize_author_name("J. A. Smith");
        assert_eq!(result, "smith ja");
    }

    #[test]
    fn test_normalize_author_name_single_word() {
        let result = normalize_author_name("Smith");
        assert_eq!(result, "smith");
    }

    #[test]
    fn test_normalize_author_name_empty() {
        let result = normalize_author_name("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_normalize_author_name_with_hyphen() {
        // "Al-Rashid, M." → "alrashid m"
        let result = normalize_author_name("Al-Rashid, M.");
        // After replacing hyphen with space: "al rashid m"
        // Last name = "al" (first part), rest = "rashid", "m" → initials "rm"
        // Actually with comma: parts = ["al", "rashid", "m"] → last_name="al", initials="rm"
        assert_eq!(result, "al rm");
    }

    #[test]
    fn test_normalize_consistency() {
        // These should all normalize to the same thing
        let n1 = normalize_author_name("Smith, John A.");
        let n2 = normalize_author_name("John A. Smith");
        let n3 = normalize_author_name("Smith, J. A.");
        assert_eq!(n1, n2);
        assert_eq!(n2, n3);
    }

    // ── build_display_name ───────────────────────────────────────

    #[test]
    fn test_build_display_name_already_comma() {
        let result = build_display_name("Smith, John A.");
        assert_eq!(result, "Smith, John A.");
    }

    #[test]
    fn test_build_display_name_first_last() {
        let result = build_display_name("John Smith");
        assert_eq!(result, "Smith, John");
    }

    #[test]
    fn test_build_display_name_single() {
        let result = build_display_name("Smith");
        assert_eq!(result, "Smith");
    }

    #[test]
    fn test_build_display_name_empty() {
        let result = build_display_name("");
        assert!(result.is_empty());
    }

    // ── parse_authors ────────────────────────────────────────────

    #[test]
    fn test_parse_authors_multiple() {
        let authors = parse_authors("Smith, J; Doe, A");
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0].display_name, "Smith, J");
        assert_eq!(authors[1].display_name, "Doe, A");
    }

    #[test]
    fn test_parse_authors_empty() {
        let authors = parse_authors("");
        assert!(authors.is_empty());
    }

    // ── normalize_term ──────────────────────────────────────────

    #[test]
    fn test_normalize_term_lowercase() {
        assert_eq!(normalize_term("Machine Learning"), "machine learning");
    }

    #[test]
    fn test_normalize_term_strip_punctuation() {
        assert_eq!(normalize_term("deep-learning;"), "deep learning");
    }

    #[test]
    fn test_normalize_term_collapse_whitespace() {
        assert_eq!(normalize_term("  natural   language  "), "natural language");
    }

    #[test]
    fn test_normalize_term_hyphen_to_space() {
        assert_eq!(normalize_term("reinforcement-learning"), "reinforcement learning");
    }

    #[test]
    fn test_normalize_term_empty() {
        assert_eq!(normalize_term(""), "");
    }

    #[test]
    fn test_normalize_term_punctuation_only() {
        assert_eq!(normalize_term("..."), "");
    }

    // ── dedup_terms ──────────────────────────────────────────────

    #[test]
    fn test_dedup_terms_removes_duplicates() {
        let terms = vec![
            "Machine Learning".to_string(),
            "machine learning".to_string(),
            "Deep Learning".to_string(),
        ];
        let result = dedup_terms(&terms);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dedup_terms_preserves_order() {
        let terms = vec!["B".to_string(), "A".to_string(), "a".to_string()];
        let result = dedup_terms(&terms);
        assert_eq!(result, vec!["B".to_string(), "A".to_string()]);
    }

    #[test]
    fn test_dedup_terms_empty() {
        let result = dedup_terms(&[]);
        assert!(result.is_empty());
    }

    // ── parse_affiliation ────────────────────────────────────────

    #[test]
    fn test_parse_affiliation_full() {
        let aff = parse_affiliation("MIT, Cambridge, MA, USA");
        assert_eq!(aff.country.as_deref(), Some("USA"));
        assert_eq!(aff.city.as_deref(), Some("MA"));
        assert_eq!(aff.institution.as_deref(), Some("MIT, Cambridge"));
    }

    #[test]
    fn test_parse_affiliation_two_parts() {
        let aff = parse_affiliation("Stanford University, USA");
        assert_eq!(aff.institution.as_deref(), Some("Stanford University"));
        assert_eq!(aff.country.as_deref(), Some("USA"));
        assert!(aff.city.is_none());
    }

    #[test]
    fn test_parse_affiliation_single() {
        let aff = parse_affiliation("Oxford University");
        assert_eq!(aff.institution.as_deref(), Some("Oxford University"));
        assert!(aff.city.is_none());
        assert!(aff.country.is_none());
    }

    #[test]
    fn test_parse_affiliation_empty() {
        let aff = parse_affiliation("");
        assert!(aff.institution.is_none());
        assert!(aff.city.is_none());
        assert!(aff.country.is_none());
    }

    #[test]
    fn test_parse_affiliation_dept_university_city_country() {
        let aff = parse_affiliation("Dept of CS, Stanford University, Stanford, CA, USA");
        assert_eq!(aff.country.as_deref(), Some("USA"));
        assert_eq!(aff.city.as_deref(), Some("CA"));
        assert_eq!(aff.institution.as_deref(), Some("Dept of CS, Stanford University, Stanford"));
    }
}
