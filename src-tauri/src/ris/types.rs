use std::collections::HashMap;

/// Parsed RIS record before conversion to `NewArticle`.
/// All fields are optional at parse time; validation is separate.
#[derive(Debug, Clone, Default)]
pub struct RisRecord {
    pub reference_type: Option<String>,
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
    /// Electronic ISSN (RIS: EI tag, BibTeX: EISSN field)
    pub eissn: Option<String>,
    pub date: Option<String>,
    pub author_address: Option<String>,
    /// Affiliation extracted from import (e.g., first part of AD in RIS, or institution/org in BibTeX)
    pub affiliation: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    /// Total times cited, extracted from N1 field
    pub num_cited: Option<i32>,
    /// Number of cited references, extracted from N1 field
    pub num_references: Option<i32>,
    /// All unrecognized RIS tags preserved as key-value pairs.
    pub extras: HashMap<String, Vec<String>>,
}

/// Result of parsing a complete RIS file.
#[derive(Debug)]
pub struct RisParseResult {
    pub records: Vec<RisRecord>,
    pub errors: Vec<RisParseError>,
}

/// A single parse error for a record in the RIS file.
#[derive(Debug)]
pub struct RisParseError {
    /// 1-based index of the record in the file.
    pub record_index: usize,
    pub message: String,
}
