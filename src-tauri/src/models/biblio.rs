use serde::{Deserialize, Serialize};

/// Normalized author entity for bibliometrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioAuthor {
    pub id: String,
    pub normalized_name: String,
    pub display_name: String,
    pub first_author_count: i32,
    pub article_count: i32,
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

/// Normalized term (keyword or extracted noun phrase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblioTerm {
    pub id: String,
    pub normalized_term: String,
    pub raw_term: String,
    pub term_type: TermType,
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
}
