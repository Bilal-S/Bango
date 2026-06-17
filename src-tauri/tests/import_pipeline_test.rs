//! Coverage for ris::import_pipeline (read_content, parse_and_validate, preview, filter).
use bango_lib::ris::import_pipeline::{
    build_preview_records, filter_excluded, parse_and_validate, parse_and_validate_from_records,
    read_content, PreviewRecord, ValidationMode,
};
use bango_lib::ris::types::RisRecord;

fn sample_ris() -> &'static str {
    "TY  - JOUR\nTI  - First Paper\nAB  - An abstract\nAU  - Smith, J.\nAU  - Doe, A.\nPY  - 2021\nJO  - Nature\nDO  - 10.1000/first\nER  - \n\n\
     TY  - JOUR\nTI  - Second Paper\nAB  - Another abstract\nAU  - Roe, B.\nPY  - 2022\nER  - \n"
}

#[test]
fn read_content_uses_content_when_provided() {
    let out = read_content(Some("TY  - JOUR".to_string()), None).expect("content");
    assert_eq!(out, "TY  - JOUR");
}

#[test]
fn read_content_errors_when_neither_provided() {
    let err = read_content(None, None).expect_err("should error");
    assert!(err.to_string().contains("No content or file path"));
}

#[test]
fn read_content_reads_from_file_path() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("test.ris");
    std::fs::write(&path, "TY  - JOUR\nER  - \n").expect("write");
    let out = read_content(None, Some(path.to_string_lossy().to_string())).expect("read");
    assert!(out.contains("JOUR"));
}

#[test]
fn read_content_errors_when_file_too_large() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("big.ris");
    // Create a sparse-looking large file by writing metadata-only (use a real large write is wasteful;
    // instead simulate by patching: we just check the path-missing path).
    // Actually test the size guard by creating a file > MAX via truncating.
    let f = std::fs::File::create(&path).expect("create");
    f.set_len(super_size_max_plus_one()).expect("set_len");
    let err = read_content(None, Some(path.to_string_lossy().to_string())).expect_err("too large");
    assert!(err.to_string().contains("File too large"));
}

fn super_size_max_plus_one() -> u64 {
    // MAX_RIS_FILE_SIZE + 1; replicate the constant to avoid exporting it.
    100 * 1024 * 1024 + 1
}

#[test]
fn parse_and_validate_strict_excludes_invalid_records() {
    // A record with no abstract or authors fails strict validation.
    let content = "TY  - JOUR\nTI  - Valid\nAB  - abs\nAU  - A\nER  - \n\n\
                   TY  - JOUR\nTI  - No Abstract No Authors\nER  - \n";
    let out = parse_and_validate(content, ValidationMode::Strict).expect("parse");
    assert_eq!(out.total_records, 2);
    assert_eq!(out.valid_records.len(), 1, "only the valid record passes");
    assert!(!out.errors.is_empty(), "errors reported for invalid record");
    assert!(out.error_groups.iter().any(|g| g.count > 0));
}

#[test]
fn parse_and_validate_none_accepts_all_records() {
    let out = parse_and_validate(sample_ris(), ValidationMode::None).expect("parse");
    assert_eq!(out.total_records, 2);
    assert_eq!(out.valid_records.len(), 2);
    assert!(out.errors.is_empty());
    assert!(out.error_groups.is_empty());
}

#[test]
fn parse_and_validate_from_records_none_passes_through() {
    let recs = vec![RisRecord { title: Some("X".to_string()), ..Default::default() }];
    let out = parse_and_validate_from_records(&recs, ValidationMode::None).expect("parse");
    assert_eq!(out.total_records, 1);
    assert_eq!(out.valid_records.len(), 1);
}

#[test]
fn build_preview_records_limits_and_maps_fields() {
    let out = parse_and_validate(sample_ris(), ValidationMode::None).expect("parse");
    let previews = build_preview_records(&out.valid_records, 10);
    assert_eq!(previews.len(), 2);
    assert_eq!(previews[0].title.as_deref(), Some("First Paper"));
    assert_eq!(previews[0].authors, vec!["Smith, J.", "Doe, A."]);
    assert_eq!(previews[0].publication_year, Some(2021));
    assert_eq!(previews[0].journal.as_deref(), Some("Nature"));
    assert_eq!(previews[0].doi.as_deref(), Some("10.1000/first"));
}

#[test]
fn build_preview_records_respects_max() {
    let out = parse_and_validate(sample_ris(), ValidationMode::None).expect("parse");
    let previews = build_preview_records(&out.valid_records, 1);
    assert_eq!(previews.len(), 1);
}

#[test]
fn preview_record_from_ris_record_maps_all_fields() {
    let rec = RisRecord {
        title: Some("T".to_string()),
        authors: vec!["A".to_string()],
        publication_year: Some(2020),
        journal: Some("J".to_string()),
        doi: Some("D".to_string()),
        ..Default::default()
    };
    let p = PreviewRecord::from_ris_record(&rec);
    assert_eq!(p.title.as_deref(), Some("T"));
    assert_eq!(p.authors, vec!["A"]);
    assert_eq!(p.publication_year, Some(2020));
    assert_eq!(p.journal.as_deref(), Some("J"));
    assert_eq!(p.doi.as_deref(), Some("D"));

    // into_article_preview defaults missing title
    let flat = PreviewRecord { title: None, ..p }.into_article_preview();
    assert_eq!(flat.title, "");
}

#[test]
fn filter_excluded_removes_specified_indices() {
    let recs = parse_and_validate(sample_ris(), ValidationMode::None).expect("parse").valid_records;
    let (kept, skipped) = filter_excluded(&recs, &[0]);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].title.as_deref(), Some("Second Paper"));
    assert_eq!(skipped, 1);
}

#[test]
fn filter_excluded_with_empty_indices_keeps_all() {
    let recs = parse_and_validate(sample_ris(), ValidationMode::None).expect("parse").valid_records;
    let (kept, skipped) = filter_excluded(&recs, &[]);
    assert_eq!(kept.len(), 2);
    assert_eq!(skipped, 0);
}

#[test]
fn filter_excluded_dedupes_duplicate_indices() {
    let recs = parse_and_validate(sample_ris(), ValidationMode::None).expect("parse").valid_records;
    // Duplicate index 0 - skipped count should be 1, not 2
    let (kept, skipped) = filter_excluded(&recs, &[0, 0]);
    assert_eq!(kept.len(), 1);
    assert_eq!(skipped, 1);
}
