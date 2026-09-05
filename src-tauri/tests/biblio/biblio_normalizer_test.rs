//! Integration tests for `biblio::normalizer`.
//!
//! Extracted from inline `#[cfg(test)] mod tests` in
//! `src/biblio/normalizer.rs` to keep the source file compact.

use bango_lib::biblio::affiliation_extractor::AffiliationExtractor;
use bango_lib::biblio::normalizer::{
    build_display_name, dedup_terms, normalize_author_name, normalize_term, parse_affiliation,
    parse_affiliation_with_extractor, parse_authors, sanitize_raw_term, split_authors,
    split_keywords,
};

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
    let result = normalize_author_name("Smith, John A.");
    assert_eq!(result, "smith ja");
}

#[test]
fn test_normalize_author_name_first_last() {
    let result = normalize_author_name("John Smith");
    assert_eq!(result, "smith j");
}

#[test]
fn test_normalize_author_name_initials() {
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
    let result = normalize_author_name("Al-Rashid, M.");
    assert_eq!(result, "al rm");
}

#[test]
fn test_normalize_consistency() {
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
    assert_eq!(normalize_term("Machine Learning"), "machin learn");
}

#[test]
fn test_normalize_term_strip_punctuation() {
    assert_eq!(normalize_term("deep-learning;"), "deep learn");
}

#[test]
fn test_normalize_term_collapse_whitespace() {
    assert_eq!(normalize_term("  natural   language  "), "natur languag");
}

#[test]
fn test_normalize_term_hyphen_to_space() {
    assert_eq!(normalize_term("reinforcement-learning"), "reinforc learn");
}

#[test]
fn test_normalize_term_empty() {
    assert_eq!(normalize_term(""), "");
}

#[test]
fn test_normalize_term_punctuation_only() {
    assert_eq!(normalize_term("..."), "");
}

#[test]
fn test_normalize_term_empty_array() {
    assert_eq!(normalize_term("[]"), "");
    assert_eq!(normalize_term("[\"\"]"), "");
    assert_eq!(normalize_term("[   ]"), "");
    assert_eq!(normalize_term("['']"), "");
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
    assert_eq!(aff.city.as_deref(), Some("Cambridge"));
    assert_eq!(aff.institution.as_deref(), Some("MIT"));
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
    assert_eq!(aff.city.as_deref(), Some("Stanford"));
    assert_eq!(aff.institution.as_deref(), Some("Stanford University"));
}

#[test]
fn test_parse_affiliation_state_only() {
    let aff = parse_affiliation("MIT, Cambridge, MA");
    assert_eq!(aff.country.as_deref(), Some("USA"));
    assert_eq!(aff.city.as_deref(), Some("Cambridge"));
    assert_eq!(aff.institution.as_deref(), Some("MIT"));
}

#[test]
fn test_parse_affiliation_uk_normalization() {
    let aff = parse_affiliation("University of Oxford, Oxford, UK");
    assert_eq!(aff.country.as_deref(), Some("United Kingdom"));
    assert_eq!(aff.city.as_deref(), Some("Oxford"));
    assert_eq!(aff.institution.as_deref(), Some("University of Oxford"));
}

// ── JSON author parsing ─────────────────────────────────────

#[test]
fn test_split_authors_json_string_array() {
    let result = split_authors(r#"["Smith, J", "Doe, A", "Brown, K"]"#);
    assert_eq!(result, vec!["Smith, J", "Doe, A", "Brown, K"]);
}

#[test]
fn test_split_authors_json_single_element() {
    let result = split_authors(r#"["Smith, J"]"#);
    assert_eq!(result, vec!["Smith, J"]);
}

#[test]
fn test_split_authors_json_empty_array() {
    let result = split_authors("[]");
    assert!(result.is_empty());
}

#[test]
fn test_split_authors_json_with_objects_name_field() {
    let result = split_authors(r#"[{"name":"Smith, J"}, {"name":"Doe, A"}]"#);
    assert_eq!(result, vec!["Smith, J", "Doe, A"]);
}

#[test]
fn test_split_authors_json_with_objects_family_given() {
    let result =
        split_authors(r#"[{"family":"Smith","given":"John"}, {"family":"Doe","given":"Alice"}]"#);
    assert_eq!(result, vec!["Smith, John", "Doe, Alice"]);
}

#[test]
fn test_split_authors_json_family_only() {
    let result = split_authors(r#"[{"family":"Smith"}, {"family":"Doe"}]"#);
    assert_eq!(result, vec!["Smith", "Doe"]);
}

#[test]
fn test_split_authors_json_mixed_types() {
    let result =
        split_authors(r#"["Smith, J", {"name":"Doe, A"}, {"family":"Brown","given":"K"}]"#);
    assert_eq!(result, vec!["Smith, J", "Doe, A", "Brown, K"]);
}

#[test]
fn test_split_authors_json_with_empty_strings() {
    let result = split_authors(r#"["Smith, J", "", "Doe, A"]"#);
    assert_eq!(result, vec!["Smith, J", "Doe, A"]);
}

#[test]
fn test_parse_authors_json_round_trip() {
    let authors = parse_authors(r#"["Smith, J", "Doe, A"]"#);
    assert_eq!(authors.len(), 2);
    assert_eq!(authors[0].display_name, "Smith, J");
    assert_eq!(authors[0].normalized_name, "smith j");
    assert_eq!(authors[1].display_name, "Doe, A");
    assert_eq!(authors[1].normalized_name, "doe a");
}

#[test]
fn test_affiliation_extractor_safe_init() {
    let ext = AffiliationExtractor::new();
    assert!(ext.is_ok());
}

#[test]
fn test_affiliation_extractor_multilingual() {
    let ext = AffiliationExtractor::new().unwrap();
    // English
    assert_eq!(
        ext.extract("Dept of Computer Science, Stanford Univ"),
        Some("Stanford University".to_string())
    );
    // French
    assert_eq!(
        ext.extract("Département de Physique, Université de Paris"),
        Some("Université de Paris".to_string())
    );
    // Spanish (no translation of proper nouns)
    assert_eq!(
        ext.extract("Facultad de Ciencias, Universidad de Buenos Aires"),
        Some("Universidad de Buenos Aires".to_string())
    );
    // German
    assert_eq!(
        ext.extract("Institut für Informatik, Universität Heidelberg"),
        Some("Universität Heidelberg".to_string())
    );
    // Korean (non-spaced substring match)
    assert_eq!(ext.extract("컴퓨터공학과, 서울대학교"), Some("서울대학교".to_string()));
}

#[test]
fn test_parse_affiliation_with_scoring() {
    let ext = AffiliationExtractor::new().unwrap();
    let aff = parse_affiliation_with_extractor(
        "Center for Brain Research, Harvard University, Boston, MA, USA",
        Some(&ext),
    );
    assert_eq!(aff.country.as_deref(), Some("USA"));
    assert_eq!(aff.city.as_deref(), Some("Boston"));
    assert_eq!(aff.institution.as_deref(), Some("Harvard University"));
}

// ── split_keywords (JSON-aware) ──────────────────────────────

#[test]
fn test_split_keywords_json_array() {
    let result = split_keywords(r#"["Allura Red", "tartrazine"]"#);
    assert_eq!(result, vec!["Allura Red", "tartrazine"]);
}

#[test]
fn test_split_keywords_json_single() {
    let result = split_keywords(r#"["unicorns"]"#);
    assert_eq!(result, vec!["unicorns"]);
}

#[test]
fn test_split_keywords_json_empty_array() {
    let result = split_keywords("[]");
    assert!(result.is_empty());
}

#[test]
fn test_split_keywords_json_with_empty_strings() {
    let result = split_keywords(r#"["", "real", ""]"#);
    assert_eq!(result, vec!["real"]);
}

#[test]
fn test_split_keywords_does_not_produce_broken_fragments() {
    let result = split_keywords(r#"["Allura Red", "tartrazine"]"#);
    assert!(!result.iter().any(|k| k.contains('[') || k.contains(']')));
    assert!(!result.iter().any(|k| k.contains('"')));
}

#[test]
fn test_split_keywords_semicolon_delimited() {
    let result = split_keywords("Allura Red; tartrazine; erythrosine");
    assert_eq!(result, vec!["Allura Red", "tartrazine", "erythrosine"]);
}

#[test]
fn test_split_keywords_comma_delimited() {
    let result = split_keywords("Allura Red, tartrazine, erythrosine");
    assert_eq!(result, vec!["Allura Red", "tartrazine", "erythrosine"]);
}

#[test]
fn test_split_keywords_mixed_delimiters() {
    let result = split_keywords("Allura Red; tartrazine, erythrosine");
    assert_eq!(result, vec!["Allura Red", "tartrazine", "erythrosine"]);
}

#[test]
fn test_split_keywords_empty() {
    assert!(split_keywords("").is_empty());
    assert!(split_keywords("   ").is_empty());
}

#[test]
fn test_split_keywords_trailing_semicolon() {
    let result = split_keywords("Allura Red;");
    assert_eq!(result, vec!["Allura Red"]);
}

#[test]
fn test_split_keywords_single_keyword() {
    let result = split_keywords("machine learning");
    assert_eq!(result, vec!["machine learning"]);
}

// ── sanitize_raw_term ────────────────────────────────────────

#[test]
fn test_sanitize_raw_term_clean() {
    assert_eq!(sanitize_raw_term("Allura Red"), "Allura Red");
    assert_eq!(sanitize_raw_term("machine learning"), "machine learning");
}

#[test]
fn test_sanitize_raw_term_strips_brackets_and_quotes() {
    assert_eq!(sanitize_raw_term(r#"["Allura Red""#), "Allura Red");
    assert_eq!(sanitize_raw_term(r#""tartrazine"]"#), "tartrazine");
    assert_eq!(sanitize_raw_term(r#""quoted""#), "quoted");
    assert_eq!(sanitize_raw_term(r#"'single'"#), "single");
}

#[test]
fn test_sanitize_raw_term_strips_trailing_comma() {
    assert_eq!(sanitize_raw_term(r#"Allura Red,"#), "Allura Red");
}

#[test]
fn test_sanitize_raw_term_collapses_whitespace() {
    assert_eq!(sanitize_raw_term("  Allura   Red  "), "Allura Red");
}

#[test]
fn test_sanitize_raw_term_empty() {
    assert_eq!(sanitize_raw_term(""), "");
    assert_eq!(sanitize_raw_term("   "), "");
    assert_eq!(sanitize_raw_term("\"\""), "");
}

#[test]
fn test_sanitize_raw_term_preserves_internal_punctuation() {
    assert_eq!(sanitize_raw_term("co-variate"), "co-variate");
    assert_eq!(sanitize_raw_term("Dr. Strange"), "Dr. Strange");
}
