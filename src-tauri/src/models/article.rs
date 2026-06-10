use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    pub id: String,
    pub sequence_id: i64,
    pub status: ArticleStatus,
    pub screening_error: bool,
    pub title: String,
    pub abstract_text: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub start_page: Option<String>,
    pub end_page: Option<String>,
    pub keywords: Vec<String>,
    pub url: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub publisher_city: Option<String>,
    pub publisher_address: Option<String>,
    pub issn: Option<String>,
    /// Electronic ISSN (RIS: EI, BibTeX: EISSN)
    pub eissn: Option<String>,
    /// FK to journal_index(id), set during import via ISSN/eISSN/title matching
    pub journal_index_id: Option<String>,
    pub reference_type: Option<String>,
    pub date: Option<String>,
    pub author_address: Option<String>,
    pub affiliation: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    pub user_notes: Option<String>,
    pub ris_extras: Option<serde_json::Value>,
    pub duplicate_of: Option<String>,
    pub ai_decision: Option<AiDecision>,
    pub ai_reasoning: Option<String>,
    pub ai_confidence: Option<f64>,
    pub matched_inclusion_criteria: Vec<String>,
    pub matched_exclusion_criteria: Vec<String>,
    pub tags: Vec<String>,
    pub labels: Vec<String>,
    pub manual_override: bool,
    pub import_source: Option<String>,
    pub imported_at: String,
    pub changed_at: String,
    pub screened_at: Option<String>,
    pub data_length: Option<usize>,
    pub token_estimate: Option<usize>,
    pub actual_tokens: Option<usize>,
    pub full_text: Option<String>,
    pub full_text_ai_summary: Option<String>,

    // --- Reference system fields ---
    /// Total times cited (from N1: `Total Times Cited: NN`)
    pub num_cited: Option<i32>,
    /// Number of references (from N1: `Cited Reference Count: NN`)
    pub num_references: Option<i32>,
    /// True when citation detail records exist in `references` (type=0)
    pub has_citation_details: bool,
    /// True when reference detail records exist in `references` (type=1)
    pub has_reference_details: bool,
    /// True when a full-text file is associated
    pub has_full_text: bool,
    /// Relative path with partial subpath (e.g., `fulltext/smith2023.pdf`)
    pub full_text_file_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArticleStatus {
    Duplicate,
    Working,
    Included,
    Rejected,
}

impl ArticleStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate",
            Self::Working => "working",
            Self::Included => "included",
            Self::Rejected => "rejected",
        }
    }
}

impl std::fmt::Display for ArticleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AiDecision {
    Include,
    Exclude,
}

impl AiDecision {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Include => "include",
            Self::Exclude => "exclude",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewArticle {
    pub title: String,
    pub abstract_text: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub start_page: Option<String>,
    pub end_page: Option<String>,
    pub keywords: Vec<String>,
    pub url: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub publisher_city: Option<String>,
    pub publisher_address: Option<String>,
    pub issn: Option<String>,
    /// Electronic ISSN (RIS: EI, BibTeX: EISSN)
    pub eissn: Option<String>,
    /// FK to journal_index(id), set during import via ISSN/eISSN/title matching
    pub journal_index_id: Option<String>,
    pub reference_type: Option<String>,
    pub date: Option<String>,
    pub author_address: Option<String>,
    pub affiliation: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    pub ris_extras: Option<serde_json::Value>,
    pub import_source: Option<String>,
    pub data_length: Option<usize>,
    pub token_estimate: Option<usize>,

    // --- Reference system fields ---
    /// Total times cited (from N1: `Total Times Cited: NN`)
    pub num_cited: Option<i32>,
    /// Number of references (from N1: `Cited Reference Count: NN`)
    pub num_references: Option<i32>,
    /// True when a full-text file is associated
    pub has_full_text: bool,
    /// Relative path with partial subpath (e.g., `fulltext/smith2023.pdf`)
    pub full_text_file_name: Option<String>,
    // Note: has_citation_details and has_reference_details default to false
    // and are only set when reference records are inserted, not during article import
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleCounts {
    pub all: usize,
    pub duplicate: usize,
    pub working: usize,
    pub included: usize,
    pub rejected: usize,
    #[serde(default)]
    pub error: usize,
    pub references: usize,
}
