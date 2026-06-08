use serde::{Deserialize, Serialize};

/// The type of reference record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceType {
    /// Another article that cites the parent article
    #[default]
    Citation,
    /// A work cited by the parent article
    Reference,
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

/// Match status for a reference paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MatchStatus {
    #[default]
    Unmatched,
    Matched,
    NotInLibrary,
    Imported,
}

impl MatchStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unmatched => "unmatched",
            Self::Matched => "matched",
            Self::NotInLibrary => "not_in_library",
            Self::Imported => "imported",
        }
    }

    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "unmatched" => Some(Self::Unmatched),
            "matched" => Some(Self::Matched),
            "not_in_library" => Some(Self::NotInLibrary),
            "imported" => Some(Self::Imported),
            _ => None,
        }
    }
}

/// A deduplicated reference paper stored in `reference_papers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencePaper {
    pub id: String,
    pub title: String,
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
    pub reference_type: Option<String>,
    pub date: Option<String>,
    pub notes: Option<String>,
    pub ris_extras: Option<serde_json::Value>,
    pub match_status: MatchStatus,
    pub matched_article_id: Option<String>,
    pub citation_count: i32,
    pub reference_count: i32,
    pub import_source: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A new reference paper to be inserted.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewReferencePaper {
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
    pub reference_type: Option<String>,
    pub date: Option<String>,
    pub notes: Option<String>,
    pub ris_extras: Option<serde_json::Value>,
    pub match_status: Option<MatchStatus>,
    pub matched_article_id: Option<String>,
    pub import_source: Option<String>,
}

/// A link between an article and a reference paper (junction row).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleReferenceLink {
    pub id: String,
    pub parent_article_id: String,
    pub reference_paper_id: String,
    pub reference_type: ReferenceType,
    pub created_at: String,
}

/// A reference paper with its link context (for querying by article).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleReference {
    /// The junction link
    pub link_id: String,
    pub parent_article_id: String,
    pub reference_type: ReferenceType,
    pub link_created_at: String,
    /// The reference paper details
    pub paper: ReferencePaper,
}
