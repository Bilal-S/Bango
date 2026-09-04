use super::similarity::{levenshtein_similarity, normalize_title, short_title_guard};
use super::types::{DedupResult, DuplicatePair, MatchStrategy, MatchType};

/// Lightweight article representation for dedup comparison.
#[derive(Debug, Clone)]
pub struct DedupArticle {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub doi: Option<String>,
    pub import_source: Option<String>,
}

/// Runs all dedup strategies against a list of articles.
/// Returns exact duplicates (auto-merge) and fuzzy matches (manual review).
#[must_use]
pub fn run_dedup(articles: &[DedupArticle]) -> DedupResult {
    let mut exact_duplicates = Vec::new();
    let mut fuzzy_matches = Vec::new();
    let mut matched_ids = std::collections::HashSet::new();

    for i in 0..articles.len() {
        if matched_ids.contains(&articles[i].id) {
            continue;
        }

        for j in (i + 1)..articles.len() {
            if matched_ids.contains(&articles[j].id) {
                continue;
            }

            if let Some(pair) = compare_articles(&articles[i], &articles[j]) {
                match pair.match_type {
                    MatchType::ExactDuplicate => {
                        exact_duplicates.push(pair);
                        // Mark the second article as matched (the "duplicate")
                        matched_ids.insert(articles[j].id.clone());
                    }
                    MatchType::FuzzyMatch => {
                        fuzzy_matches.push(pair);
                    }
                }
                break; // First match wins
            }
        }
    }

    let auto_merged_count = exact_duplicates.len();
    let needs_review_count = fuzzy_matches.len();

    DedupResult { exact_duplicates, fuzzy_matches, auto_merged_count, needs_review_count }
}

fn compare_articles(a: &DedupArticle, b: &DedupArticle) -> Option<DuplicatePair> {
    // Strategy 1: DOI exact match
    if let (Some(doi_a), Some(doi_b)) = (&a.doi, &b.doi) {
        let norm_a = crate::ris::doi::normalize_doi(Some(doi_a));
        let norm_b = crate::ris::doi::normalize_doi(Some(doi_b));
        if let (Some(na), Some(nb)) = (norm_a, norm_b) {
            if na == nb {
                return Some(make_pair(
                    a,
                    b,
                    1.0,
                    MatchType::ExactDuplicate,
                    MatchStrategy::DoiExact,
                ));
            }
        }
    }

    let norm_a = normalize_title(&a.title);
    let norm_b = normalize_title(&b.title);

    // Short-title guard for strategies 2-4
    let a_short = short_title_guard(&a.title);
    let b_short = short_title_guard(&b.title);

    // Strategy 2: Title + Year (>= 95% similarity)
    if !a_short && !b_short {
        if let (Some(year_a), Some(year_b)) = (a.publication_year, b.publication_year) {
            if year_a == year_b {
                let sim = levenshtein_similarity(&norm_a, &norm_b);
                if sim >= 0.95 {
                    return Some(make_pair(
                        a,
                        b,
                        sim,
                        MatchType::ExactDuplicate,
                        MatchStrategy::TitleYear,
                    ));
                }

                // Strategy 3: Fuzzy Title + Year (70-94% similarity)
                if sim >= 0.70 {
                    return Some(make_pair(
                        a,
                        b,
                        sim,
                        MatchType::FuzzyMatch,
                        MatchStrategy::FuzzyTitleYear,
                    ));
                }
            }
        }
    }

    // Strategy 4: Author + Title partial
    if !a_short && !b_short {
        if let (Some(first_a), Some(first_b)) = (a.authors.first(), b.authors.first()) {
            let last_a = extract_last_name(first_a);
            let last_b = extract_last_name(first_b);
            if last_a.eq_ignore_ascii_case(&last_b) {
                let sim = levenshtein_similarity(&norm_a, &norm_b);
                if sim >= 0.80 {
                    return Some(make_pair(
                        a,
                        b,
                        sim,
                        MatchType::FuzzyMatch,
                        MatchStrategy::AuthorTitle,
                    ));
                }
            }
        }
    }

    None
}

fn make_pair(
    a: &DedupArticle,
    b: &DedupArticle,
    similarity: f64,
    match_type: MatchType,
    strategy: MatchStrategy,
) -> DuplicatePair {
    DuplicatePair {
        article_a_id: a.id.clone(),
        article_b_id: b.id.clone(),
        article_a_title: a.title.clone(),
        article_b_title: b.title.clone(),
        article_a_authors: a.authors.clone(),
        article_b_authors: b.authors.clone(),
        article_a_year: a.publication_year,
        article_b_year: b.publication_year,
        article_a_source: a.import_source.clone(),
        article_b_source: b.import_source.clone(),
        similarity,
        match_type,
        strategy,
    }
}

/// Extracts the last name from an author string like "Smith, John" or "John Smith".
fn extract_last_name(author: &str) -> String {
    let trimmed = author.trim();
    if let Some(pos) = trimmed.find(',') {
        // "Smith, John" format
        trimmed[..pos].trim().to_lowercase()
    } else if let Some(pos) = trimmed.rfind(' ') {
        // "John Smith" format
        trimmed[pos + 1..].trim().to_lowercase()
    } else {
        trimmed.to_lowercase()
    }
}
