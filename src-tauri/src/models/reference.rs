use serde::{Deserialize, Serialize};

/// The type of reference record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceType {
    /// Another article that cites the parent article
    Citation,
    /// A work cited by the parent article
    Reference,
}

impl Default for ReferenceType {
    fn default() -> Self {
        ReferenceType::Citation
    }
}

impl ReferenceType {
    #[must_use]
    pub fn as_int(&self) -> i32 {
        match self {
            Self::Citation => 0,
            Self::Reference => 1,
        }
    }

    #[must_use]
    pub fn from_int(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Citation),
            1 => Some(Self::Reference),
            _ => None,
        }
    }
}

/// Match status for a reference record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchStatus {
    Unmatched,
    Matched,
    NotInLibrary,
}

impl Default for MatchStatus {
    fn default() -> Self {
        MatchStatus::Unmatched
    }
}

impl MatchStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unmatched => "unmatched",
            Self::Matched => "matched",
            Self::NotInLibrary => "not_in_library",
        }
    }

    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "unmatched" => Some(Self::Unmatched),
            "matched" => Some(Self::Matched),
            "not_in_library" => Some(Self::NotInLibrary),
            _ => None,
        }
    }
}

/// A reference/citation detail record linked to a parent article.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub id: String,
    pub reference_type: ReferenceType,
    pub parent_id: String,
    pub match_status: MatchStatus,

    // Metadata (all optional — reference exports often have partial data)
    pub title: Option<String>,
    pub abstract_text: Option<String>,
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
    pub reference_type_field: Option<String>,
    pub date: Option<String>,
    pub author_address: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    pub user_notes: Option<String>,
    pub ris_extras: Option<serde_json::Value>,

    // Citation counts
    pub num_cited: Option<i32>,
    pub num_references: Option<i32>,

    // Full-text tracking
    pub has_full_text: bool,
    pub full_text_file_name: Option<String>,

    // Import tracking
    pub import_source: Option<String>,
    pub imported_at: String,
}

/// A new reference record to be inserted.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewReference {
    pub reference_type: ReferenceType,
    pub parent_id: String,
    pub match_status: MatchStatus,

    pub title: Option<String>,
    pub abstract_text: Option<String>,
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
    pub reference_type_field: Option<String>,
    pub date: Option<String>,
    pub author_address: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    pub ris_extras: Option<serde_json::Value>,

    pub num_cited: Option<i32>,
    pub num_references: Option<i32>,
    pub has_full_text: bool,
    pub full_text_file_name: Option<String>,
    pub import_source: Option<String>,
}