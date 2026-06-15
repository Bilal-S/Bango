use serde::{Deserialize, Serialize};

/// Normalized author entity for bibliometrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiblioAuthor {
    pub id: String,
    pub normalized_name: String,
    pub display_name: String,
    pub first_author_count: i32,
    pub article_count: i32,
    pub total_citations: i32,
    pub avg_year: Option<f64>,
    pub estimated_h_index: Option<i32>,
    pub created_at: String,
}

/// Link between an article and a normalized author.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioArticleAuthor {
    pub id: String,
    pub article_id: String,
    pub author_id: String,
    pub author_order: i32,
    pub raw_name: Option<String>,
    pub raw_affiliation: Option<String>,
}

/// Normalized institution entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioInstitution {
    pub id: String,
    pub normalized_name: String,
    pub country: Option<String>,
    pub city: Option<String>,
    pub created_at: String,
}

/// Link between an author (per-article) and an institution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioAuthorAffiliation {
    pub id: String,
    pub article_id: String,
    pub author_id: String,
    pub institution_id: String,
}

/// Term type: keyword from RIS/metadata or noun-phrase extracted by LLM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TermType {
    Keyword,
    NounPhrase,
}

impl std::fmt::Display for TermType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TermType::Keyword => write!(f, "keyword"),
            TermType::NounPhrase => write!(f, "noun_phrase"),
        }
    }
}

/// Source of a term: metadata extraction, LLM screening, or user-added.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TermSource {
    Metadata,
    AiExtracted,
    UserAdded,
}

impl std::fmt::Display for TermSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TermSource::Metadata => write!(f, "metadata"),
            TermSource::AiExtracted => write!(f, "ai_extracted"),
            TermSource::UserAdded => write!(f, "user_added"),
        }
    }
}

/// Normalized term (keyword or extracted noun phrase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioTerm {
    pub id: String,
    pub normalized_term: String,
    pub raw_term: String,
    pub term_type: TermType,
    pub source: TermSource,
    pub article_count: i32,
    pub created_at: String,
}

/// Link between an article and a term.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioArticleTerm {
    pub id: String,
    pub article_id: String,
    pub term_id: String,
    pub frequency: i32,
}

/// Network type for bibliometric visualizations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkType {
    CoAuthorship,
    CoOccurrence,
    Citation,
    BiblioCoupling,
    CoCitation,
}

impl std::fmt::Display for NetworkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkType::CoAuthorship => write!(f, "co_authorship"),
            NetworkType::CoOccurrence => write!(f, "co_occurrence"),
            NetworkType::Citation => write!(f, "citation"),
            NetworkType::BiblioCoupling => write!(f, "biblio_coupling"),
            NetworkType::CoCitation => write!(f, "co_citation"),
        }
    }
}

/// Saved network metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiblioNetworkMeta {
    pub id: String,
    pub network_type: NetworkType,
    pub label: String,
    pub article_filter: Option<String>,
    pub params_json: Option<String>,
    pub node_count: i32,
    pub edge_count: i32,
    pub created_at: String,
}

/// A node in a saved network graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioNetworkNode {
    pub id: String,
    pub network_id: String,
    pub entity_id: String,
    pub label: String,
    pub weight: f64,
    pub cluster: Option<i32>,
    pub x: Option<f64>,
    pub y: Option<f64>,
}

/// An edge in a saved network graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioNetworkEdge {
    pub id: String,
    pub network_id: String,
    pub source_id: String,
    pub target_id: String,
    pub weight: f64,
}

/// Status summary returned by `biblio_get_status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiblioStatus {
    pub author_count: i32,
    pub institution_count: i32,
    pub term_count: i32,
    pub article_author_links: i32,
    pub article_term_links: i32,
    pub network_count: i32,
}

/// A single year's publication count for the per-year bar chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YearCount {
    pub year: i32,
    pub count: i32,
}

/// One journal's article count for a single publication year.
///
/// `journal` carries the canonical `journal_index.journal_title` when
/// `journal_index_id` is `Some`; otherwise it carries the normalized raw title
/// (`UPPER(TRIM(articles.journal))`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalYearData {
    pub journal: String,
    pub year: i32,
    pub count: i32,
    /// FK → `journal_index.id`. `Some` → canonical match; `None` → raw fallback.
    pub journal_index_id: Option<String>,
}

/// Full metadata + time-series for one journal. Loaded lazily by the timeline
/// info card via `biblio_get_journal_info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalInfo {
    pub id: String,
    pub journal_title: String,
    pub issn: Option<String>,
    pub eissn: Option<String>,
    pub publisher_name: Option<String>,
    pub publisher_address: Option<String>,
    pub languages: Option<String>,
    pub web_of_science_categories: Option<String>,
    /// Number of included articles linked to this journal.
    pub article_count: i32,
    pub first_year: Option<i32>,
    pub last_year: Option<i32>,
    /// This journal's yearly included-article counts (ascending by year).
    pub pubs_by_year: Vec<YearCount>,
    /// SUM(num_cited) across included articles in this journal.
    pub citations_total: i64,
}

/// KPI summary for the bibliometric dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BiblioKpis {
    /// Number of included articles.
    pub included_count: i32,
    /// Total citations across included articles (SUM of num_cited).
    pub total_citations: i64,
    /// Number of unique normalized authors.
    pub unique_authors: i32,
    /// Earliest publication year among included articles.
    pub year_from: Option<i32>,
    /// Latest publication year among included articles.
    pub year_to: Option<i32>,
    /// Average publications per year (total included / number of distinct years).
    pub pubs_per_year: Option<f64>,
    /// Publications per year, ordered by year ASC — powers the mini bar chart.
    pub pubs_by_year: Vec<YearCount>,
    /// Average year-over-year growth rate across all consecutive year pairs (percentage).
    pub avg_growth_rate: Option<f64>,
    /// Reference papers of included articles, grouped by publication_year — powers the References bar chart.
    pub refs_by_year: Vec<YearCount>,
    /// Normalized citations by year — actual detail records where available, decay-distributed otherwise.
    pub citations_by_year: Vec<YearCount>,
    /// Per-journal, per-year counts for the timeline stacked view. Grouped by
    /// canonical `journal_title` when `journal_index_id` is set, else normalized raw title.
    pub journal_distribution: Vec<JournalYearData>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
