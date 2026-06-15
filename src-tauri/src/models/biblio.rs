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

// ── Author Productivity models ─────────────────────────────────

/// Author ranking row — one per normalized author.
/// Extends BiblioAuthor with derived metrics for the productivity view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorRank {
    pub id: String,
    pub display_name: String,
    pub normalized_name: String,
    pub article_count: i32,
    pub first_author_count: i32,
    pub last_author_count: i32,
    pub solo_paper_count: i32,
    pub total_citations: i64,
    pub estimated_h_index: i32,
    pub i10_index: i32,
    pub g_index: i32,
    pub avg_citations_per_paper: Option<f64>,
    pub avg_year: Option<f64>,
    pub years_active: Option<i32>,
    pub productivity_rate: Option<f64>,
    /// Papers published in the last 5 years.
    pub recent_paper_count: i32,
    /// Primary institution (most recent by publication_year), if linked.
    pub primary_institution: Option<String>,
}

/// Full author profile for the detail panel (lazy-loaded per click).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorDetail {
    pub rank: AuthorRank,
    pub pubs_by_year: Vec<YearCount>,
    pub institutions: Vec<BiblioInstitution>,
    pub top_collaborators: Vec<AuthorCollaborator>,
    pub recent_papers: Vec<AuthorPaper>,
}

/// A collaborator of the selected author, with co-authorship strength.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorCollaborator {
    pub collaborator_id: String,
    pub collaborator_name: String,
    /// Number of shared papers (full counting).
    pub shared_papers: i32,
}

/// A recent paper by the selected author (for the detail panel list).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorPaper {
    pub article_id: String,
    pub title: String,
    pub publication_year: Option<i32>,
    pub journal: Option<String>,
    pub num_cited: Option<i64>,
    pub author_order: i32,
    pub doi: Option<String>,
}

/// Aggregate KPI stats for the productivity view header strip.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorProductivityKpis {
    pub total_authors: i32,
    /// Sum of article_count across all authors (may exceed included articles — multi-author).
    pub total_papers: i64,
    pub avg_h_index: Option<f64>,
    pub max_h_index: i32,
    pub avg_citations: Option<f64>,
    /// Distinct co-author pairs (edges in the co-authorship network).
    pub total_collaborations: i64,
    /// (min_year, max_year) span of included articles.
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
}
