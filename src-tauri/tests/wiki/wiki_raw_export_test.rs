//! Tests for `wiki::raw_export` (raw-source preparation + user-file extractors).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` in
//! `src/wiki/raw_export.rs` to keep the source file compact.

use bango_lib::models::article::Article;
use bango_lib::wiki::frontmatter;
use bango_lib::wiki::raw_export::{
    add_user_file, article_content, csv_to_markdown_table, extract_user_file, process_user_files,
    slugify, strip_html, strip_rtf, RawSourceKind,
};

use tempfile::TempDir;

// ---- article_content ----

#[test]
fn article_content_ignores_full_text_prefers_blob() {
    let mut a = sample_article();
    a.full_text = Some("full body".to_string());
    a.full_text_ai_summary = Some("{\"summary_150_250_words\":\"ai\"}".to_string());
    let (content, kind) = article_content(&a);
    assert_eq!(kind, "ai_summary");
    assert_eq!(content, "## Summary\n\nai");
    assert!(!content.contains("full body"), "full text leaked: {content}");
}

#[test]
fn article_content_falls_back_to_ai_summary() {
    let mut a = sample_article();
    a.full_text = None;
    a.full_text_ai_summary = Some("{\"summary_150_250_words\":\"ai digest\"}".to_string());
    let (content, kind) = article_content(&a);
    assert_eq!(content, "## Summary\n\nai digest");
    assert_eq!(kind, "ai_summary");
}

#[test]
fn article_content_falls_back_to_raw_ai_summary_when_not_json() {
    let mut a = sample_article();
    a.full_text = None;
    a.full_text_ai_summary = Some("plain summary".to_string());
    let (content, kind) = article_content(&a);
    assert_eq!(content, "plain summary");
    assert_eq!(kind, "ai_summary");
}

#[test]
fn article_content_falls_back_to_abstract() {
    let mut a = sample_article();
    a.full_text = None;
    a.full_text_ai_summary = None;
    let (content, kind) = article_content(&a);
    assert_eq!(content, "the abstract");
    assert_eq!(kind, "abstract");
}

#[test]
fn article_content_ignores_empty_full_text() {
    let mut a = sample_article();
    a.full_text = Some("   ".to_string()); // whitespace-only
    let (content, kind) = article_content(&a);
    assert_eq!(kind, "abstract");
    assert_eq!(content, "the abstract");
}

// ---- RawSourceKind classification ----

#[test]
fn classifies_known_extensions() {
    assert_eq!(RawSourceKind::from_extension("pdf"), RawSourceKind::UserPdf);
    assert_eq!(RawSourceKind::from_extension("PDF"), RawSourceKind::UserPdf);
    assert_eq!(RawSourceKind::from_extension("txt"), RawSourceKind::UserText);
    assert_eq!(RawSourceKind::from_extension("html"), RawSourceKind::UserHtml);
    assert_eq!(RawSourceKind::from_extension("rtf"), RawSourceKind::UserRtf);
    assert_eq!(RawSourceKind::from_extension("csv"), RawSourceKind::UserCsv);
    assert_eq!(RawSourceKind::from_extension("md"), RawSourceKind::UserMarkdown);
    assert_eq!(RawSourceKind::from_extension("json"), RawSourceKind::UserData);
    assert_eq!(RawSourceKind::from_extension("py"), RawSourceKind::UserCode);
    assert_eq!(RawSourceKind::from_extension("docx"), RawSourceKind::Unsupported);
}

#[test]
fn extract_user_file_txt() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("notes.txt");
    std::fs::write(&path, "hello world").unwrap();
    let (content, kind) = extract_user_file(&path).unwrap();
    assert_eq!(kind, RawSourceKind::UserText);
    assert!(content.contains("hello world"));
}

#[test]
fn extract_user_file_md_passthrough() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("note.md");
    std::fs::write(&path, "# Title\ntext").unwrap();
    let (content, kind) = extract_user_file(&path).unwrap();
    assert_eq!(kind, RawSourceKind::UserMarkdown);
    assert!(content.contains("# Title"));
}

#[test]
fn extract_user_file_code_is_fenced() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("script.py");
    std::fs::write(&path, "print('hi')").unwrap();
    let (content, kind) = extract_user_file(&path).unwrap();
    assert_eq!(kind, RawSourceKind::UserCode);
    assert!(content.contains("```py"));
    assert!(content.contains("print('hi')"));
}

#[test]
fn extract_user_file_json_is_fenced() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("data.json");
    std::fs::write(&path, "{\"a\":1}").unwrap();
    let (content, kind) = extract_user_file(&path).unwrap();
    assert_eq!(kind, RawSourceKind::UserData);
    assert!(content.contains("```json"));
}

#[test]
fn extract_user_file_unsupported_errors() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("file.docx");
    std::fs::write(&path, b"PK\x03\x04").unwrap();
    let result = extract_user_file(&path);
    assert!(result.is_err());
}

// ---- strip_html / strip_rtf ----

#[test]
fn strip_html_removes_tags_and_decodes_entities() {
    let html = "<h1>Title</h1><p>Hello &amp; goodbye</p><p>Second line</p>";
    let text = strip_html(html).unwrap();
    assert!(!text.contains('<'));
    assert!(text.contains("Title"));
    assert!(text.contains("Hello & goodbye"));
    assert!(text.contains("Second line"));
}

#[test]
fn strip_rtf_removes_control_words() {
    let rtf = "{\\rtf1\\b hello\\par world}";
    let text = strip_rtf(rtf).unwrap();
    assert!(text.contains("hello"));
    assert!(!text.contains("\\b"));
    assert!(!text.contains("{\\rtf1"));
}

#[test]
fn csv_to_markdown_table_renders_header_and_rows() {
    let csv = "name,value\nfoo,1\nbar,2";
    let md = csv_to_markdown_table(csv);
    assert!(md.contains("| name | value |"));
    assert!(md.contains("| --- | --- |"));
    assert!(md.contains("| foo | 1 |"));
    assert!(md.contains("| bar | 2 |"));
}

// ---- idempotency ----

#[test]
fn process_user_files_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("raw")).unwrap();
    std::fs::write(root.join("raw/notes.txt"), "hello").unwrap();

    let r1 = process_user_files(root).unwrap();
    assert_eq!(r1.user_files_written, 1);
    assert_eq!(r1.user_files_skipped, 0);

    // second run: unchanged -> skipped
    let r2 = process_user_files(root).unwrap();
    assert_eq!(r2.user_files_written, 0);
    assert_eq!(r2.user_files_skipped, 1);

    // companion exists and has correct frontmatter
    let companion = root.join("raw/user-notes.md");
    assert!(companion.exists());
    let (fm, body) = frontmatter::read_file(&companion).unwrap();
    assert_eq!(fm.get("source_file"), Some("notes.txt"));
    assert_eq!(fm.get("source_kind"), Some("user_text"));
    assert!(body.contains("hello"));
}

#[test]
fn process_user_files_re_extracts_when_source_changes() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("raw")).unwrap();
    std::fs::write(root.join("raw/notes.txt"), "v1").unwrap();
    process_user_files(root).unwrap();

    // change the source content
    std::fs::write(root.join("raw/notes.txt"), "v2 with more words").unwrap();
    let r = process_user_files(root).unwrap();
    assert_eq!(r.user_files_written, 1);
    assert_eq!(r.user_files_skipped, 0);

    let (_, body) = frontmatter::read_file(&root.join("raw/user-notes.md")).unwrap();
    assert!(body.contains("v2 with more words"));
}

#[test]
fn process_user_files_reports_unsupported() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("raw")).unwrap();
    std::fs::write(root.join("raw/thing.docx"), "PK").unwrap();
    let r = process_user_files(root).unwrap();
    assert_eq!(r.user_files_written, 0);
    assert_eq!(r.user_files_unsupported.len(), 1);
}

#[test]
fn add_user_file_copies_and_extracts() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("raw")).unwrap();
    let src = tmp.path().join("external.txt");
    std::fs::write(&src, "external content").unwrap();

    let companion = add_user_file(root, &src).unwrap();
    assert!(companion.exists());
    // original is copied into raw/
    assert!(root.join("raw/external.txt").exists());
    let (fm, body) = frontmatter::read_file(&companion).unwrap();
    assert_eq!(fm.get("source_file"), Some("external.txt"));
    assert!(body.contains("external content"));
}

// ---- slugify ----

#[test]
fn slugify_handles_spaces_and_punctuation() {
    assert_eq!(slugify("My Report!"), "my-report");
    assert_eq!(slugify("foo___bar"), "foo-bar");
    assert_eq!(slugify("UPPER"), "upper");
}

// ---- AI-summary blob rendering (wikifix-final Change 1) ----

#[test]
fn render_summary_blob_renders_all_fields() {
    let blob = r#"{"summary_150_250_words":"digest words","key_insights":["insight one"],"keywords":["kw-a"],"field":"Public Health","subfield":"Nutrition","section_summaries":[{"section":"Methods","summary":"We ran a trial.","key_points":["randomised"],"study_design":"RCT","sample_size":"120","effect_size":"d=0.4","confidence_interval":"0.1 to 0.7"}]}"#;
    let mut a = sample_article();
    a.full_text_ai_summary = Some(blob.to_string());
    let (content, kind) = article_content(&a);
    assert_eq!(kind, "ai_summary");
    assert!(content.contains("## Summary\n\ndigest words"), "{content}");
    assert!(content.contains("- insight one"), "{content}");
    assert!(content.contains("## Keywords\n\nkw-a"), "{content}");
    assert!(content.contains("## Field\n\nPublic Health > Nutrition"), "{content}");
    assert!(content.contains("### Methods"), "{content}");
    assert!(content.contains("We ran a trial."), "{content}");
    assert!(content.contains("- randomised"), "{content}");
    assert!(content.contains("- Study design: RCT"), "{content}");
    assert!(content.contains("- Sample size: 120"), "{content}");
    assert!(content.contains("- Effect size: d=0.4"), "{content}");
    assert!(content.contains("- 95% CI: 0.1 to 0.7"), "{content}");
}

#[test]
fn article_content_empty_json_blob_falls_to_abstract() {
    let mut a = sample_article();
    a.full_text_ai_summary = Some("{}".to_string());
    let (content, kind) = article_content(&a);
    assert_eq!(kind, "abstract");
    assert_eq!(content, "the abstract");
}

// ---- helpers ----

fn sample_article() -> Article {
    Article {
        id: "art-1".to_string(),
        sequence_id: 1,
        status: bango_lib::models::article::ArticleStatus::Included,
        screening_error: false,
        title: "Sample".to_string(),
        abstract_text: "the abstract".to_string(),
        authors: vec!["Doe, J".to_string()],
        publication_year: Some(2024),
        doi: Some("10.1/x".to_string()),
        journal: Some("Nature".to_string()),
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: vec!["tag1".to_string()],
        url: None,
        language: None,
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn: None,
        eissn: None,
        journal_index_id: None,
        reference_type: None,
        date: None,
        author_address: None,
        affiliation: None,
        accession_number: None,
        custom_field3: None,
        journal_abbreviation: None,
        journal_iso_abbreviation: None,
        notes: None,
        web_of_science_db: None,
        user_notes: None,
        ris_extras: None,
        duplicate_of: None,
        ai_decision: None,
        ai_reasoning: None,
        ai_confidence: None,
        matched_inclusion_criteria: Vec::new(),
        matched_exclusion_criteria: Vec::new(),
        tags: Vec::new(),
        labels: Vec::new(),
        manual_override: false,
        import_source: None,
        imported_at: "2024-01-01T00:00:00Z".to_string(),
        changed_at: "2024-01-01T00:00:00Z".to_string(),
        screened_at: None,
        data_length: None,
        token_estimate: None,
        actual_tokens: None,
        full_text: None,
        full_text_ai_summary: None,
        num_cited: None,
        num_references: None,
        has_citation_details: false,
        has_reference_details: false,
        has_full_text: false,
        full_text_file_name: None,
        has_figures_or_tables: false,
        is_translated: false,
        translation_status: "none".to_string(),
        translation_error: None,
        translated_at: None,
    }
}
