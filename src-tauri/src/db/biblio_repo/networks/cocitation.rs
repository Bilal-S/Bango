use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

use super::labels::format_paper_label;
use crate::error::AppError;

/// Scope for co-citation analysis: which citing articles to include.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CocitationScope {
    /// Only articles with `status = 'included'` (default).
    IncludedArticles,
    /// All non-duplicate articles (working + included + rejected).
    AllArticles,
}

impl CocitationScope {
    /// Returns the SQL WHERE-clause fragment filtering `articles.status`.
    fn status_filter(self) -> &'static str {
        match self {
            Self::IncludedArticles => "a.status = 'included'",
            Self::AllArticles => "a.status IN ('working', 'included', 'rejected')",
        }
    }

    /// Returns the string label for the scope.
    fn label(self) -> &'static str {
        match self {
            Self::IncludedArticles => "included",
            Self::AllArticles => "all",
        }
    }
}

/// Normalization mode for co-citation edge weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CocitationNormalization {
    Raw,
    Cosine,
    Jaccard,
    Pearson,
}

impl CocitationNormalization {
    /// Returns the string label.
    fn label(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Cosine => "cosine",
            Self::Jaccard => "jaccard",
            Self::Pearson => "pearson",
        }
    }
}

/// Raw co-citation pair: two papers and their raw co-citation count.
struct CocitationPair {
    source: String,
    target: String,
    raw_count: i32,
}

/// Computed co-citation data: raw counts and per-paper citation totals.
struct CocitationComputation {
    pairs: Vec<CocitationPair>,
    /// `paper_id → c_i` (how many in-scope articles cite this paper).
    citation_totals: HashMap<String, i32>,
}

/// Build raw co-citation counts from `article_reference_links` (type=1).
///
/// For each in-scope article, all pairs of its reference papers are co-cited.
/// The result is filtered by `min_citation_count` (per-paper) and
/// `min_co_citation` (per-pair).
///
/// This function does NOT persist anything — it returns the computed data
/// for the JSON serializer.
fn compute_cocitation(
    conn: &Connection,
    scope: CocitationScope,
    min_citation_count: i32,
    min_co_citation: i32,
) -> Result<CocitationComputation, AppError> {
    // 1. Fetch (article_id, reference_paper_id) pairs where type = 1 (reference).
    let status_filter = scope.status_filter();
    let sql = format!(
        "SELECT l.parent_article_id, l.reference_paper_id \
         FROM article_reference_links l \
         JOIN articles a ON a.id = l.parent_article_id \
         WHERE l.type = 1 AND {status_filter}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let pairs: Vec<(String, String)> =
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    // 2. Group reference papers by parent article.
    let mut article_refs: HashMap<String, Vec<String>> = HashMap::new();
    for (article_id, paper_id) in pairs {
        article_refs.entry(article_id).or_default().push(paper_id);
    }

    // 3. Compute per-paper citation totals (c_i = unique articles citing paper i).
    let mut citation_totals: HashMap<String, i32> = HashMap::new();
    for refs in article_refs.values() {
        // Dedup: one article may cite the same paper only once (UNIQUE constraint),
        // but defensive dedup in case of data anomalies.
        let unique: HashSet<&String> = refs.iter().collect();
        for paper_id in unique {
            *citation_totals.entry(paper_id.clone()).or_insert(0) += 1;
        }
    }

    // 4. Apply min_citation_count filter → candidate node set.
    let candidates: HashSet<String> = citation_totals
        .iter()
        .filter(|(_, c)| **c >= min_citation_count)
        .map(|(k, _)| k.clone())
        .collect();

    if candidates.is_empty() {
        return Ok(CocitationComputation { pairs: Vec::new(), citation_totals });
    }

    // 5. For each article, generate all co-citation pairs among candidates.
    let mut cocitation_matrix: HashMap<(String, String), i32> = HashMap::new();
    for refs in article_refs.values() {
        // Filter to candidates and dedup within this article.
        let filtered: Vec<&String> = refs.iter().filter(|r| candidates.contains(*r)).collect();
        let unique: HashSet<&String> = filtered.iter().copied().collect();
        let sorted: Vec<&String> = {
            let mut v = unique.into_iter().collect::<Vec<_>>();
            v.sort_unstable();
            v
        };
        for i in 0..sorted.len() {
            for j in (i + 1)..sorted.len() {
                // Canonical ordering: source < target to make undirected pairs unique.
                let (src, tgt) = if sorted[i] < sorted[j] {
                    (sorted[i].clone(), sorted[j].clone())
                } else {
                    (sorted[j].clone(), sorted[i].clone())
                };
                *cocitation_matrix.entry((src, tgt)).or_insert(0) += 1;
            }
        }
    }

    // 6. Filter edges by min_co_citation and build the pairs list.
    let mut pair_list: Vec<CocitationPair> = cocitation_matrix
        .iter()
        .filter(|(_, count)| **count >= min_co_citation)
        .map(|((s, t), c)| CocitationPair { source: s.clone(), target: t.clone(), raw_count: *c })
        .collect();

    // Sort by raw count descending for deterministic output.
    pair_list.sort_unstable_by_key(|p| std::cmp::Reverse(p.raw_count));

    Ok(CocitationComputation { pairs: pair_list, citation_totals })
}

/// Compute the normalized weight for a co-citation pair.
///
/// - **Raw**: `c_ij`
/// - **Cosine**: `c_ij / sqrt(c_i × c_j)`
/// - **Jaccard**: `c_ij / (c_i + c_j − c_ij)`
/// - **Pearson**: correlation coefficient (range [−1, 1]). Requires the full
///   co-citation matrix and the set of all papers. Computed lazily.
fn normalize_weight(mode: CocitationNormalization, raw: f64, c_i: f64, c_j: f64) -> f64 {
    match mode {
        CocitationNormalization::Raw => raw,
        CocitationNormalization::Cosine => {
            let denom = (c_i * c_j).sqrt();
            if denom > 0.0 {
                raw / denom
            } else {
                0.0
            }
        }
        CocitationNormalization::Jaccard => {
            let denom = c_i + c_j - raw;
            if denom > 0.0 {
                raw / denom
            } else {
                0.0
            }
        }
        // Pearson is handled separately in compute_pearson_weights (needs full matrix).
        // This fallback should never be called for Pearson.
        CocitationNormalization::Pearson => raw,
    }
}

/// Compute Pearson correlation co-citation weights for all pairs.
///
/// For each pair (i, j), we treat each in-scope article as a binary vector
/// over all candidate papers. The correlation is computed from the 2×2
/// contingency table:
///
/// ```text
///            j cited   j not cited
/// i cited      a          b
/// i not cited  c          d
/// ```
///
/// where `a = c_ij`, `b = c_i − c_ij`, `c = c_j − c_ij`, and
/// `d = N − a − b − c` (N = total in-scope articles).
///
/// The phi coefficient (= Pearson r for binary variables) is:
/// `(ad − bc) / sqrt((a+b)(c+d)(a+c)(b+d))`
fn compute_pearson_weights(
    pairs: &[CocitationPair],
    citation_totals: &HashMap<String, i32>,
    total_articles: i64,
) -> HashMap<(String, String), f64> {
    let n = total_articles as f64;
    let mut result = HashMap::with_capacity(pairs.len());

    for pair in pairs {
        let a = pair.raw_count as f64;
        let c_i = citation_totals.get(&pair.source).copied().unwrap_or(0) as f64;
        let c_j = citation_totals.get(&pair.target).copied().unwrap_or(0) as f64;
        let b = c_i - a;
        let c = c_j - a;
        let d = n - a - b - c;

        let numer = a * d - b * c;
        let denom_sq = (a + b) * (c + d) * (a + c) * (b + d);
        let weight = if denom_sq > 0.0 { numer / denom_sq.sqrt() } else { 0.0 };

        result.insert((pair.source.clone(), pair.target.clone()), weight);
    }

    result
}

/// Get the co-citation network as JSON for graph rendering.
///
/// Computes co-citation on-demand from `article_reference_links` (type=1).
/// Nodes are `reference_papers` cited by at least `min_citation_count` in-scope
/// articles. Edges connect papers co-cited by at least `min_co_citation` articles.
///
/// All four normalization modes (raw, cosine, jaccard, pearson) are computed
/// and returned; the frontend selects which to visualize.
#[allow(clippy::type_complexity)]
pub fn get_cocitation_network_json(
    conn: &Connection,
    scope: CocitationScope,
    normalization: CocitationNormalization,
    min_citation_count: i32,
    min_co_citation: i32,
) -> Result<serde_json::Value, AppError> {
    let computation = compute_cocitation(conn, scope, min_citation_count, min_co_citation)?;

    // Count total in-scope articles for Pearson and diagnostics.
    let total_articles: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM articles a WHERE {}", scope.status_filter()),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Total reference papers linked to in-scope articles (for diagnostics).
    let total_ref_papers: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT l.reference_paper_id) \
                 FROM article_reference_links l \
                 JOIN articles a ON a.id = l.parent_article_id \
                 WHERE l.type = 1 AND {}",
                scope.status_filter()
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Compute Pearson weights if needed.
    let pearson_weights = if normalization == CocitationNormalization::Pearson {
        compute_pearson_weights(&computation.pairs, &computation.citation_totals, total_articles)
    } else {
        HashMap::new()
    };

    // Build edges JSON.
    let edges: Vec<serde_json::Value> = computation
        .pairs
        .iter()
        .map(|pair| {
            let c_i = computation.citation_totals.get(&pair.source).copied().unwrap_or(0) as f64;
            let c_j = computation.citation_totals.get(&pair.target).copied().unwrap_or(0) as f64;

            let (selected_weight, _raw_weight, cosine_w, jaccard_w, pearson_w) = match normalization
            {
                CocitationNormalization::Raw => {
                    let r = pair.raw_count as f64;
                    (
                        r,
                        r,
                        normalize_weight(CocitationNormalization::Cosine, r, c_i, c_j),
                        normalize_weight(CocitationNormalization::Jaccard, r, c_i, c_j),
                        0.0,
                    )
                }
                CocitationNormalization::Cosine => {
                    let r = pair.raw_count as f64;
                    let cos = normalize_weight(CocitationNormalization::Cosine, r, c_i, c_j);
                    (
                        cos,
                        r,
                        cos,
                        normalize_weight(CocitationNormalization::Jaccard, r, c_i, c_j),
                        0.0,
                    )
                }
                CocitationNormalization::Jaccard => {
                    let r = pair.raw_count as f64;
                    let jac = normalize_weight(CocitationNormalization::Jaccard, r, c_i, c_j);
                    (
                        jac,
                        r,
                        normalize_weight(CocitationNormalization::Cosine, r, c_i, c_j),
                        jac,
                        0.0,
                    )
                }
                CocitationNormalization::Pearson => {
                    let r = pair.raw_count as f64;
                    let p = pearson_weights
                        .get(&(pair.source.clone(), pair.target.clone()))
                        .copied()
                        .unwrap_or(0.0);
                    (
                        p,
                        r,
                        normalize_weight(CocitationNormalization::Cosine, r, c_i, c_j),
                        normalize_weight(CocitationNormalization::Jaccard, r, c_i, c_j),
                        p,
                    )
                }
            };

            serde_json::json!({
                "source": pair.source,
                "target": pair.target,
                "weight": (selected_weight * 1000.0).round() / 1000.0,
                "rawWeight": pair.raw_count,
                "cosineWeight": (cosine_w * 1000.0).round() / 1000.0,
                "jaccardWeight": (jaccard_w * 1000.0).round() / 1000.0,
                "pearsonWeight": (pearson_w * 1000.0).round() / 1000.0,
            })
        })
        .collect();

    // Collect all node IDs from surviving edges.
    let node_ids: HashSet<&str> =
        computation.pairs.iter().flat_map(|p| [p.source.as_str(), p.target.as_str()]).collect();

    // Fetch node metadata from reference_papers.
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    if !node_ids.is_empty() {
        let id_list: Vec<&str> = node_ids.iter().copied().collect();
        let placeholders: String = id_list.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, title, authors, publication_year, journal, doi, \
                    abstract_text, citation_count, match_status, matched_article_id, reference_type \
             FROM reference_papers \
             WHERE id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let params = rusqlite::params_from_iter(id_list.iter());
        let node_rows: Vec<(
            String,
            String,
            String,
            Option<i32>,
            Option<String>,
            Option<String>,
            String,
            i64,
            String,
            Option<String>,
            Option<String>,
        )> = stmt
            .query_map(params, |row| {
                Ok((
                    row.get(0)?,                                 // id
                    row.get(1)?,                                 // title
                    row.get(2)?,                                 // authors
                    row.get(3)?,                                 // publication_year
                    row.get(4)?,                                 // journal
                    row.get(5)?,                                 // doi
                    row.get::<_, String>(6).unwrap_or_default(), // abstract_text
                    row.get(7)?,                                 // citation_count
                    row.get(8)?,                                 // match_status
                    row.get(9)?,                                 // matched_article_id
                    row.get(10)?,                                // reference_type
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        for (
            id,
            title,
            authors,
            year,
            journal,
            doi,
            abstract_text,
            citation_count,
            _match_status,
            matched_article_id,
            reference_type,
        ) in node_rows
        {
            let label = format_paper_label(&authors, year);
            let co_citation_total = computation.citation_totals.get(&id).copied().unwrap_or(0);
            nodes.push(serde_json::json!({
                "id": id,
                "label": label,
                "title": title,
                "authors": authors,
                "year": year,
                "journal": journal,
                "doi": doi,
                "citationCount": citation_count,
                "coCitationCount": co_citation_total,
                "matchedArticleId": matched_article_id,
                "abstract": abstract_text,
                "referenceType": reference_type,
            }));
        }
    }

    let candidate_count = nodes.len();

    Ok(serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "meta": {
            "nodeCount": nodes.len(),
            "edgeCount": edges.len(),
            "inScopeArticleCount": total_articles,
            "referencePaperCount": total_ref_papers,
            "candidatePaperCount": candidate_count,
            "scope": scope.label(),
            "normalization": normalization.label(),
        }
    }))
}
