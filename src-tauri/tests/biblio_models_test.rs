//! Integration tests for `models::biblio` serialization and Display impls.
//!
//! Extracted from inline `#[cfg(test)] mod tests` in
//! `src/models/biblio.rs` to keep the source file compact.

use bango_lib::models::biblio::{
    BiblioAuthor, BiblioKpis, BiblioNetworkEdge, BiblioNetworkNode, BiblioTerm, JournalYearData,
    NetworkType, TermSource, TermType, YearCount,
};

// ── TermSource ─────────────────────────────────────────────

#[test]
fn term_source_display_metadata() {
    assert_eq!(TermSource::Metadata.to_string(), "metadata");
}

#[test]
fn term_source_display_ai_extracted() {
    assert_eq!(TermSource::AiExtracted.to_string(), "ai_extracted");
}

#[test]
fn term_source_display_user_added() {
    assert_eq!(TermSource::UserAdded.to_string(), "user_added");
}

#[test]
fn term_source_serde_roundtrip() {
    let source = TermSource::AiExtracted;
    let json = serde_json::to_string(&source).unwrap();
    assert_eq!(json, "\"ai_extracted\"");
    let back: TermSource = serde_json::from_str(&json).unwrap();
    assert_eq!(back, TermSource::AiExtracted);
}

#[test]
fn term_source_equality() {
    assert_eq!(TermSource::Metadata, TermSource::Metadata);
    assert_ne!(TermSource::Metadata, TermSource::AiExtracted);
    assert_ne!(TermSource::AiExtracted, TermSource::UserAdded);
}

// ── TermType ───────────────────────────────────────────────

#[test]
fn term_type_display() {
    assert_eq!(TermType::Keyword.to_string(), "keyword");
    assert_eq!(TermType::NounPhrase.to_string(), "noun_phrase");
}

// ── NetworkType ────────────────────────────────────────────

#[test]
fn network_type_display() {
    assert_eq!(NetworkType::CoAuthorship.to_string(), "co_authorship");
    assert_eq!(NetworkType::CoOccurrence.to_string(), "co_occurrence");
    assert_eq!(NetworkType::Citation.to_string(), "citation");
    assert_eq!(NetworkType::BiblioCoupling.to_string(), "biblio_coupling");
    assert_eq!(NetworkType::CoCitation.to_string(), "co_citation");
}

#[test]
fn network_type_serde_roundtrip() {
    let nt = NetworkType::CoAuthorship;
    let json = serde_json::to_string(&nt).unwrap();
    assert_eq!(json, "\"co_authorship\"");
    let back: NetworkType = serde_json::from_str(&json).unwrap();
    assert_eq!(back, NetworkType::CoAuthorship);
}

// ── BiblioNetworkNode ──────────────────────────────────────

#[test]
fn network_node_serialization() {
    let node = BiblioNetworkNode {
        id: "node-1".into(),
        network_id: "net-1".into(),
        entity_id: "author-1".into(),
        label: "Smith J".into(),
        weight: 5.0,
        cluster: Some(0),
        x: Some(1.23),
        y: Some(4.56),
    };
    let json = serde_json::to_string(&node).unwrap();
    assert!(json.contains("\"entity_id\":\"author-1\""));
    assert!(json.contains("\"cluster\":0"));
}

// ── BiblioNetworkEdge ──────────────────────────────────────

#[test]
fn network_edge_serialization() {
    let edge = BiblioNetworkEdge {
        id: "edge-1".into(),
        network_id: "net-1".into(),
        source_id: "author-1".into(),
        target_id: "author-2".into(),
        weight: 3.0,
    };
    let json = serde_json::to_string(&edge).unwrap();
    assert!(json.contains("\"source_id\":\"author-1\""));
    assert!(json.contains("\"target_id\":\"author-2\""));
    assert!(json.contains("\"weight\":3.0"));
}

// ── BiblioAuthor ───────────────────────────────────────────

#[test]
fn biblio_author_camel_case_serialization() {
    let author = BiblioAuthor {
        id: "a-1".into(),
        normalized_name: "smith j".into(),
        display_name: "Smith J".into(),
        first_author_count: 2,
        article_count: 5,
        total_citations: 42,
        avg_year: Some(2021.5),
        estimated_h_index: Some(3),
        created_at: "2024-01-01".into(),
    };
    let json = serde_json::to_string(&author).unwrap();
    assert!(json.contains("\"normalizedName\""));
    assert!(json.contains("\"displayName\""));
    assert!(json.contains("\"totalCitations\""));
    assert!(json.contains("\"avgYear\""));
    assert!(json.contains("\"estimatedHIndex\""));
}

// ── BiblioTerm ─────────────────────────────────────────────

#[test]
fn biblio_term_with_source() {
    let term = BiblioTerm {
        id: "t-1".into(),
        normalized_term: "machine learning".into(),
        raw_term: "Machine Learning".into(),
        term_type: TermType::Keyword,
        source: TermSource::Metadata,
        article_count: 10,
        created_at: "2024-01-01".into(),
    };
    let json = serde_json::to_string(&term).unwrap();
    assert!(json.contains("\"source\":\"metadata\""));
    assert!(json.contains("\"term_type\":\"keyword\""));
}

#[test]
fn biblio_term_ai_source() {
    let term = BiblioTerm {
        id: "t-2".into(),
        normalized_term: "neural networks".into(),
        raw_term: "Neural Networks".into(),
        term_type: TermType::NounPhrase,
        source: TermSource::AiExtracted,
        article_count: 3,
        created_at: "2024-01-01".into(),
    };
    let json = serde_json::to_string(&term).unwrap();
    assert!(json.contains("\"source\":\"ai_extracted\""));
}

// ── BiblioKpis ─────────────────────────────────────────────

#[test]
fn biblio_kpis_serialization() {
    let kpis = BiblioKpis {
        included_count: 100,
        total_citations: 500,
        unique_authors: 42,
        year_from: Some(2010),
        year_to: Some(2024),
        pubs_per_year: Some(7.14),
        pubs_by_year: vec![YearCount { year: 2020, count: 10 }],
        avg_growth_rate: Some(5.5),
        refs_by_year: vec![],
        citations_by_year: vec![],
        journal_distribution: vec![],
    };
    let json = serde_json::to_string(&kpis).unwrap();
    assert!(json.contains("\"includedCount\":100"));
    assert!(json.contains("\"uniqueAuthors\":42"));
    assert!(json.contains("\"avgGrowthRate\":5.5"));
    assert!(json.contains("\"journalDistribution\":[]"));
}

// ── JournalYearData ────────────────────────────────────────

#[test]
fn journal_year_data_camel_case_serialization() {
    let jyd = JournalYearData {
        journal: "Nature".into(),
        year: 2024,
        count: 5,
        journal_index_id: Some("j-1".into()),
    };
    let json = serde_json::to_string(&jyd).unwrap();
    assert!(json.contains("\"journalIndexId\":\"j-1\""));
    assert!(json.contains("\"journal\":\"Nature\""));
    let back: JournalYearData = serde_json::from_str(&json).unwrap();
    assert_eq!(back, jyd);
}

#[test]
fn journal_year_data_null_index_id_serializes_as_null() {
    let jyd = JournalYearData {
        journal: "RAW TITLE".into(),
        year: 2020,
        count: 1,
        journal_index_id: None,
    };
    let json = serde_json::to_string(&jyd).unwrap();
    assert!(json.contains("\"journalIndexId\":null"));
}

// ── YearCount ──────────────────────────────────────────────

#[test]
fn year_count_equality() {
    let a = YearCount { year: 2024, count: 5 };
    let b = YearCount { year: 2024, count: 5 };
    let c = YearCount { year: 2023, count: 5 };
    assert_eq!(a, b);
    assert_ne!(a, c);
}
