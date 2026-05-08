use bango_lib::dedup::engine::{self, DedupArticle};
use bango_lib::ris::parser::parse_ris;
use bango_lib::ris::validator::validate_all;
use std::fs;
use std::path::PathBuf;

fn asset_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets");
    path.push(name);
    path
}

#[test]
fn test_dedup_no_false_positives_on_real_data() {
    // Sugar.ris and Blue.ris contain distinct articles.
    let content1 = fs::read_to_string(asset_path("Sugar.ris")).expect("fixture not found");
    let content2 = fs::read_to_string(asset_path("Blue.ris")).expect("fixture not found");

    let parsed1 = parse_ris(&content1).expect("Parse failed");
    let parsed2 = parse_ris(&content2).expect("Parse failed");

    let (valid1, _) = validate_all(&parsed1.records);
    let (valid2, _) = validate_all(&parsed2.records);

    let articles: Vec<DedupArticle> = valid1
        .iter()
        .chain(valid2.iter())
        .map(|r| DedupArticle {
            id: uuid::Uuid::new_v4().to_string(),
            title: r.title.clone().unwrap_or_default(),
            authors: r.authors.clone(),
            publication_year: r.publication_year,
            doi: r.doi.clone(),
            import_source: None,
        })
        .collect();

    // Verify we have articles to test
    assert!(articles.len() >= 16);
    let result = engine::run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 0, "Should not find exact duplicates in real data");
    assert_eq!(result.fuzzy_matches.len(), 0, "Should not find fuzzy matches in real data");
}

#[test]
fn test_dedup_detects_doi_duplicate_from_real_data() {
    let content = fs::read_to_string(asset_path("Blue.ris")).expect("fixture not found");
    let parsed = parse_ris(&content).expect("Parse failed");
    let (valid, _) = validate_all(&parsed.records);

    let original = &valid[0];
    let mut articles = vec![DedupArticle {
        id: "a1".to_string(),
        title: original.title.clone().unwrap_or_default(),
        authors: original.authors.clone(),
        publication_year: original.publication_year,
        doi: original.doi.clone(),
        import_source: None,
    }];

    // Add a duplicate with same DOI but different title
    articles.push(DedupArticle {
        id: "a2".to_string(),
        title: "Completely Different Title".to_string(),
        authors: vec!["Other Author".to_string()],
        publication_year: Some(2020),
        doi: original.doi.clone(),
        import_source: None,
    });

    let result = engine::run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 1, "Should detect DOI duplicate");
}
