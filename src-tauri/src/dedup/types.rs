use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchType {
    ExactDuplicate,
    FuzzyMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchStrategy {
    DoiExact,
    TitleYear,
    FuzzyTitleYear,
    AuthorTitle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatePair {
    pub article_a_id: String,
    pub article_b_id: String,
    pub article_a_title: String,
    pub article_b_title: String,
    pub article_a_authors: Vec<String>,
    pub article_b_authors: Vec<String>,
    pub article_a_year: Option<i32>,
    pub article_b_year: Option<i32>,
    pub similarity: f64,
    pub match_type: MatchType,
    pub strategy: MatchStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupResult {
    pub exact_duplicates: Vec<DuplicatePair>,
    pub fuzzy_matches: Vec<DuplicatePair>,
    pub auto_merged_count: usize,
    pub needs_review_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DedupResolution {
    KeepA,
    KeepB,
    KeepBoth,
}
