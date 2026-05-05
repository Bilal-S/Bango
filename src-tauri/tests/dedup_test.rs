use bango_lib::dedup::engine::{run_dedup, DedupArticle};
use bango_lib::dedup::similarity::{levenshtein_similarity, normalize_title, short_title_guard};

fn make_article(
    id: &str,
    title: &str,
    authors: &[&str],
    year: Option<i32>,
    doi: Option<&str>,
) -> DedupArticle {
    DedupArticle {
        id: id.to_string(),
        title: title.to_string(),
        authors: authors.iter().map(|a| a.to_string()).collect(),
        publication_year: year,
        doi: doi.map(|d| d.to_string()),
    }
}

#[test]
fn test_doi_exact_match() {
    let articles = vec![
        make_article("1", "Title A", &["Author A"], Some(2023), Some("10.1234/test")),
        make_article("2", "Title B", &["Author B"], Some(2023), Some("10.1234/test")),
    ];
    let result = run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 1);
    assert!(result.fuzzy_matches.is_empty());
}

#[test]
fn test_title_year_exact_match() {
    let articles = vec![
        make_article("1", "Machine Learning for Systematic Reviews", &["Smith"], Some(2023), None),
        make_article("2", "Machine Learning for Systematic Reviews", &["Smith"], Some(2023), None),
    ];
    let result = run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 1);
}

#[test]
fn test_title_year_fuzzy_match() {
    let articles = vec![
        make_article(
            "1",
            "Deep learning approaches for cancer detection",
            &["Smith"],
            Some(2023),
            None,
        ),
        make_article(
            "2",
            "Deep learning approach for cancer detections",
            &["Jones"],
            Some(2023),
            None,
        ),
    ];
    let result = run_dedup(&articles);
    assert_eq!(result.fuzzy_matches.len(), 1);
}

#[test]
fn test_no_match_different_years_and_authors() {
    let articles = vec![
        make_article("1", "Machine learning for systematic reviews", &["Smith"], Some(2020), None),
        make_article("2", "Machine learning for systematic reviews", &["Jones"], Some(2023), None),
    ];
    let result = run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 0);
    assert_eq!(result.fuzzy_matches.len(), 0);
}

#[test]
fn test_no_match_short_titles() {
    let articles = vec![
        make_article("1", "Short", &["Smith"], Some(2023), None),
        make_article("2", "Short", &["Smith"], Some(2023), None),
    ];
    let result = run_dedup(&articles);
    // Short titles skip title-based matching
    assert_eq!(result.exact_duplicates.len(), 0);
    assert_eq!(result.fuzzy_matches.len(), 0);
}

#[test]
fn test_null_year_skips_strategies_2_and_3() {
    let articles = vec![
        make_article(
            "1",
            "Very similar title about machine learning applications",
            &["Smith"],
            None,
            None,
        ),
        make_article(
            "2",
            "Very similar title about machine learning application",
            &["Smith"],
            None,
            None,
        ),
    ];
    let result = run_dedup(&articles);
    // Without year, strategies 2 & 3 are skipped. Strategy 4 (author+title) should catch this.
    assert_eq!(result.exact_duplicates.len() + result.fuzzy_matches.len(), 1);
}

#[test]
fn test_first_author_last_name_match() {
    let articles = vec![
        make_article(
            "1",
            "Neural network approaches to text classification",
            &["Smith, John"],
            Some(2023),
            None,
        ),
        make_article(
            "2",
            "Neural network approaches to text classifications",
            &["Smith, Jane"],
            Some(2023),
            None,
        ),
    ];
    let result = run_dedup(&articles);
    // Author matches, title similarity >= 80%
    assert_eq!(result.exact_duplicates.len() + result.fuzzy_matches.len(), 1);
}

#[test]
fn test_first_match_wins_no_double_matching() {
    let articles = vec![
        make_article("1", "Test Article Title One", &["Smith"], Some(2023), Some("10.1234/same")),
        make_article("2", "Test Article Title One", &["Smith"], Some(2023), Some("10.1234/same")),
    ];
    let result = run_dedup(&articles);
    // Should only match once despite matching multiple strategies
    assert_eq!(result.exact_duplicates.len(), 1);
}

#[test]
fn test_normalize_title_strips_punctuation() {
    assert_eq!(normalize_title("Hello, World! (2023)"), "hello world 2023");
}

#[test]
fn test_normalize_title_collapses_whitespace() {
    assert_eq!(normalize_title("  Hello   World  "), "hello world");
}

#[test]
fn test_normalize_title_strips_all_punctuation() {
    assert_eq!(normalize_title("A Study of ML.;:!?'''-()[]{}"), "a study of ml");
}

#[test]
fn test_levenshtein_identical() {
    assert!((levenshtein_similarity("hello world", "hello world") - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_levenshtein_completely_different() {
    let sim = levenshtein_similarity("abc", "xyz");
    assert!(sim < 0.2, "Expected low similarity, got {}", sim);
}

#[test]
fn test_levenshtein_near_match() {
    let sim = levenshtein_similarity(
        "machine learning approaches to systematic review",
        "machine learning approach to systematic reviews",
    );
    assert!(sim > 0.9, "Expected high similarity, got {}", sim);
}

#[test]
fn test_levenshtein_moderate_match() {
    let sim = levenshtein_similarity(
        "deep learning for cancer detection",
        "deep learning for tumor detection",
    );
    assert!(sim > 0.7 && sim < 0.95, "Expected moderate similarity, got {}", sim);
}

#[test]
fn test_short_title_guard_short() {
    assert!(short_title_guard("ab")); // 2 chars, should be guarded
}

#[test]
fn test_short_title_guard_long_enough() {
    assert!(!short_title_guard("this is a longer title")); // 23 chars, OK
}

#[test]
fn test_short_title_guard_boundary() {
    assert!(short_title_guard("123456789")); // 9 chars, still short
    assert!(!short_title_guard("1234567890")); // 10 chars, OK
}
