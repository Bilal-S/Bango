//! OpenAlex Work -> Bango `NewArticle` mapping. All pure (`#[must_use]`).

use std::collections::HashMap;

use crate::models::article::NewArticle;

use super::OpenAlexWork;

/// Reconstruct abstract from OpenAlex's inverted index. Returns `""` if empty/null.
#[must_use]
pub fn reconstruct_abstract(inverted_index: &Option<HashMap<String, Vec<i32>>>) -> String {
    let Some(index) = inverted_index else {
        return String::new();
    };
    if index.is_empty() {
        return String::new();
    }
    let mut words: Vec<(&String, i32)> = Vec::new();
    for (word, positions) in index {
        for &pos in positions {
            words.push((word, pos));
        }
    }
    words.sort_by_key(|(_, pos)| *pos);
    words.iter().map(|(word, _)| word.as_str()).collect::<Vec<_>>().join(" ")
}

const SNIPPET_MAX_CHARS: usize = 200;

/// Truncate to 200 chars at last word boundary. Append `"..."` if truncated.
#[must_use]
pub fn truncate_snippet(abstract_text: &str) -> String {
    if abstract_text.chars().count() <= SNIPPET_MAX_CHARS {
        return abstract_text.to_string();
    }
    let truncated: String = abstract_text.chars().take(SNIPPET_MAX_CHARS).collect();
    if let Some(last_space) = truncated.rfind(' ') {
        let mut snippet = truncated[..last_space].to_string();
        snippet.push_str("...");
        snippet
    } else {
        // Single long word - hard-truncate at 200 + ellipsis
        let mut snippet = truncated;
        snippet.push_str("...");
        snippet
    }
}

/// Strip `https://doi.org/` prefix, lowercase. Returns `None` if empty/absent.
#[must_use]
pub fn normalize_doi(doi: &Option<String>) -> Option<String> {
    doi.as_ref().filter(|s| !s.is_empty()).map(|s| {
        s.strip_prefix("https://doi.org/")
            .or_else(|| s.strip_prefix("http://doi.org/"))
            .unwrap_or(s)
            .to_lowercase()
    })
}

/// Map OpenAlex work type to RIS-equivalent `reference_type`.
#[must_use]
pub fn map_work_type_to_reference_type(work_type: &Option<String>) -> Option<String> {
    work_type.as_ref().map(|t| {
        match t.as_str() {
            "article" => "JOUR",
            "book" => "BOOK",
            "book-chapter" => "CHAP",
            "preprint" => "GEN",
            "dissertation" => "THES",
            "dataset" => "GEN",
            "review" => "JOUR",
            _ => "GEN",
        }
        .to_string()
    })
}

/// Extract ISSN-L from source, normalized via `normalize_issn`. Returns `None` if absent/invalid.
#[must_use]
pub fn extract_issn_l(source: &super::OpenAlexSource) -> Option<String> {
    source.issn_l.as_deref().map(crate::db::journal_repo::normalize_issn).filter(|s| !s.is_empty())
}

/// Extract the eISSN: first ISSN in the array differing from `issn_l`, normalized.
#[must_use]
pub fn extract_eissn(source: &super::OpenAlexSource) -> Option<String> {
    let issn_l = source.issn_l.as_deref();
    source
        .issn
        .as_ref()
        .and_then(|issns| issns.iter().find(|issn| Some(issn.as_str()) != issn_l).cloned())
        .map(|s| crate::db::journal_repo::normalize_issn(&s))
        .filter(|s| !s.is_empty())
}

/// Extract author display names from the authorships array.
#[must_use]
pub fn extract_authors(work: &OpenAlexWork) -> Vec<String> {
    work.authorships.iter().filter_map(|a| a.author.display_name.clone()).collect()
}

/// Extract keyword display names from the keywords array.
#[must_use]
pub fn extract_keywords(work: &OpenAlexWork) -> Vec<String> {
    work.keywords.iter().map(|k| k.display_name.clone()).collect()
}

/// Map a single OpenAlex Work to a Bango `NewArticle`. Pure, no I/O.
#[must_use]
pub fn map_work_to_new_article(work: &OpenAlexWork) -> NewArticle {
    let title = work.title.clone().unwrap_or_default();
    let abstract_text = reconstruct_abstract(&work.abstract_inverted_index);
    let authors = extract_authors(work);
    let keywords = extract_keywords(work);

    let (journal, url, issn, eissn) = match &work.primary_location {
        Some(loc) => {
            let journal = loc.source.as_ref().and_then(|s| s.display_name.clone());
            let url = loc.landing_page_url.clone();
            let (issn, eissn) = match &loc.source {
                Some(source) => {
                    let issn = extract_issn_l(source);
                    let eissn = extract_eissn(source);
                    (issn, eissn)
                }
                None => (None, None),
            };
            (journal, url, issn, eissn)
        }
        None => (None, None, None, None),
    };

    let (volume, issue, start_page, end_page) = work
        .biblio
        .as_ref()
        .map(|b| (b.volume.clone(), b.issue.clone(), b.first_page.clone(), b.last_page.clone()))
        .unwrap_or((None, None, None, None));

    let doi = normalize_doi(&work.doi);

    let data_length = title.chars().count() + abstract_text.chars().count();
    let token_estimate = data_length / 4;

    NewArticle {
        title,
        abstract_text,
        authors,
        publication_year: work.publication_year,
        doi,
        journal,
        volume,
        issue,
        start_page,
        end_page,
        keywords,
        url,
        language: work.language.clone(),
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn,
        eissn,
        journal_index_id: None,
        reference_type: map_work_type_to_reference_type(&work.work_type),
        date: work.publication_date.clone(),
        author_address: None,
        affiliation: None,
        accession_number: None,
        custom_field3: None,
        journal_abbreviation: None,
        journal_iso_abbreviation: None,
        notes: None,
        web_of_science_db: None,
        ris_extras: None,
        import_source: Some("openalex".to_string()),
        data_length: Some(data_length),
        token_estimate: Some(token_estimate),
        num_cited: Some(work.cited_by_count),
        num_references: None,
        has_full_text: false,
        full_text_file_name: None,
    }
}

/// Map a slice of OpenAlex Works to Bango `NewArticle` values.
#[must_use]
pub fn map_works_to_new_articles(works: &[OpenAlexWork]) -> Vec<NewArticle> {
    works.iter().map(map_work_to_new_article).collect()
}

/// Extract DOI from a work as normalized string (lowercase, no prefix).
#[must_use]
pub fn work_doi_normalized(work: &OpenAlexWork) -> Option<String> {
    normalize_doi(&work.doi)
}

/// Map an OpenAlex Work to a `NewReferencePaper` for the reference harvest.
#[must_use]
pub fn map_work_to_reference_paper(
    work: &OpenAlexWork,
) -> crate::models::reference::NewReferencePaper {
    let title = work.title.clone().unwrap_or_default();
    let authors = extract_authors(work);
    let keywords = extract_keywords(work);

    let (journal, url, issn, eissn) = match &work.primary_location {
        Some(loc) => {
            let journal = loc.source.as_ref().and_then(|s| s.display_name.clone());
            let url = loc.landing_page_url.clone();
            let (issn, eissn) = match &loc.source {
                Some(source) => {
                    let issn = extract_issn_l(source);
                    let eissn = extract_eissn(source);
                    (issn, eissn)
                }
                None => (None, None),
            };
            (journal, url, issn, eissn)
        }
        None => (None, None, None, None),
    };

    let (volume, issue, start_page, end_page) = work
        .biblio
        .as_ref()
        .map(|b| (b.volume.clone(), b.issue.clone(), b.first_page.clone(), b.last_page.clone()))
        .unwrap_or((None, None, None, None));

    let doi = normalize_doi(&work.doi);

    crate::models::reference::NewReferencePaper {
        title: if title.is_empty() { None } else { Some(title) },
        abstract_text: Some(reconstruct_abstract(&work.abstract_inverted_index)),
        authors,
        publication_year: work.publication_year,
        doi,
        journal,
        volume,
        issue,
        start_page,
        end_page,
        keywords,
        url,
        language: work.language.clone(),
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn,
        eissn,
        journal_index_id: None,
        reference_type: map_work_type_to_reference_type(&work.work_type),
        date: work.publication_date.clone(),
        notes: None,
        ris_extras: None,
        match_status: None,
        matched_article_id: None,
        import_source: Some("openalex_harvest".to_string()),
    }
}
