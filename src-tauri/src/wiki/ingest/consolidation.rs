//! Deterministic page consolidation (multi-batch only).
//!
//! Merges near-duplicate pages produced by independent parallel ingest batches
//! and rewrites inbound `[[wikilinks]]` to canonical slugs. All logic here is
//! deterministic (no LLM merge calls) - the merge is a lossless append +
//! metadata union.

use std::collections::{HashMap, HashSet};

use crate::wiki::frontmatter::{self, Frontmatter};

use super::ParsedPage;

/// Jaccard similarity threshold (on stemmed slug tokens) above which two
/// concept/method pages are considered near-duplicates and merged.
pub const DEDUP_JACCARD_THRESHOLD: f64 = 0.5;

/// Minimum number of shared `source_articles` for two pages to be considered
/// near-duplicates regardless of slug similarity.
pub const DEDUP_SHARED_SOURCES_MIN: usize = 2;

/// Merge near-duplicate pages in-place. Returns a map of `old_slug -> new_slug`
/// for all pages that were merged into a canonical page (the inbound link
/// rewriter uses this to update `[[wikilinks]]` across the page set).
///
/// Detection (two pages are duplicates when ANY is true):
/// - Exact slug match (case-insensitive).
/// - Stemmed-token Jaccard similarity of slugs >= `DEDUP_JACCARD_THRESHOLD`.
/// - Shared `source_articles` count >= `DEDUP_SHARED_SOURCES_MIN`.
///
/// Merge is lossless: the duplicate's body is appended under a
/// `## Additional perspectives` heading; `source_articles` and `tags` are
/// unioned. The canonical page is the one with the shortest slug (most likely
/// the LLM's "preferred" form) or, on ties, the first encountered.
pub fn consolidate_pages(pages: &mut Vec<ParsedPage>) -> HashMap<String, String> {
    if pages.len() <= 1 {
        return HashMap::new();
    }

    // Build the list of merge targets: for each page, find the canonical page
    // it should merge INTO (if any). We use a simple O(n^2) scan since n is
    // small (dozens to low hundreds of pages).
    let n = pages.len();
    // `canonical[i]` = the index of the page that page `i` should merge into.
    // Initially, each page is its own canonical.
    let mut canonical: Vec<usize> = (0..n).collect();
    let mut slug_map: HashMap<String, String> = HashMap::new();

    for i in 0..n {
        // Skip if page i already merged into something.
        if canonical[i] != i {
            continue;
        }
        // Skip author pages - they are pre-seeded and should never be merged.
        let page_type_i = pages[i].frontmatter.get("type").unwrap_or("concept");
        if page_type_i == "author" {
            continue;
        }
        for j in (i + 1)..n {
            // Skip if page j already merged into something.
            if canonical[j] != j {
                continue;
            }
            let page_type_j = pages[j].frontmatter.get("type").unwrap_or("concept");
            if page_type_j == "author" {
                continue;
            }
            // Only merge pages of the same type (concept + concept, etc.).
            if page_type_i != page_type_j {
                continue;
            }
            if pages_are_duplicates(&pages[i], &pages[j]) {
                canonical[j] = i;
            }
        }
    }

    // Collect the list of merges: (source_idx, canonical_idx). We build the
    // merge data (body + frontmatter to append) from the immutable borrow,
    // then apply the appends + removals in separate passes to satisfy the
    // borrow checker.
    let mut merges: Vec<(usize, usize)> = Vec::new(); // (source_idx, canonical_idx)
    for (i, &canon) in canonical.iter().enumerate().take(n) {
        if canon != i {
            merges.push((i, canon));
        }
    }

    let mut append_data: HashMap<usize, Vec<(String, Frontmatter)>> = HashMap::new();
    // Track which indices to remove (sorted desc so swap_remove ordering is safe).
    let mut to_remove: Vec<usize> = merges.iter().map(|(src, _)| *src).collect();
    to_remove.sort_unstable_by(|a, b| b.cmp(a));

    for &(src_idx, canon_idx) in &merges {
        let src_body = pages[src_idx].body.clone();
        let src_fm = pages[src_idx].frontmatter.clone();
        append_data.entry(canon_idx).or_default().push((src_body, src_fm));
        // Record the slug redirect.
        let old_slug = pages[src_idx].slug.clone();
        let new_slug = pages[canon_idx].slug.clone();
        // Case-insensitive: store the lowercased old slug so the rewriter can
        // match [[Old-Slug]] as well as [[old-slug]].
        slug_map.insert(old_slug.to_lowercase(), new_slug);
    }

    // Apply the appends.
    for (canon_idx, appends) in append_data {
        for (body, fm) in appends {
            // Append body.
            pages[canon_idx].body.push_str("\n\n## Additional perspectives\n\n");
            pages[canon_idx].body.push_str(&body);
            // Union source_articles.
            union_list_field(&mut pages[canon_idx].frontmatter, &fm, "source_articles");
            // Union tags.
            union_list_field(&mut pages[canon_idx].frontmatter, &fm, "tags");
        }
    }

    // Remove merged source pages (descending order keeps earlier indices valid).
    for &idx in &to_remove {
        if idx < pages.len() {
            pages.remove(idx);
        }
    }

    slug_map
}

/// Union a list-valued frontmatter field from `src` into `dest`.
/// Handles the `[a, b]` inline YAML format used by the wiki frontmatter.
fn union_list_field(dest: &mut Frontmatter, src: &Frontmatter, field: &str) {
    let dest_list = frontmatter::parse_list(dest.get(field).unwrap_or(""));
    let src_list = frontmatter::parse_list(src.get(field).unwrap_or(""));
    if dest_list.is_empty() && src_list.is_empty() {
        return;
    }
    let mut seen: HashSet<String> = dest_list.iter().cloned().collect();
    for item in src_list {
        seen.insert(item);
    }
    let mut combined: Vec<String> = seen.into_iter().collect();
    combined.sort();
    let formatted = format!("[{}]", combined.join(", "));
    dest.set(field, &formatted);
}

/// Determine whether two parsed pages are near-duplicates.
pub(super) fn pages_are_duplicates(a: &ParsedPage, b: &ParsedPage) -> bool {
    // Exact slug match (case-insensitive).
    if a.slug.to_lowercase() == b.slug.to_lowercase() {
        return true;
    }
    // Stemmed-token Jaccard similarity on slugs.
    let tokens_a = stemmed_token_set(&a.slug);
    let tokens_b = stemmed_token_set(&b.slug);
    let jaccard = jaccard_similarity(&tokens_a, &tokens_b);
    if jaccard >= DEDUP_JACCARD_THRESHOLD {
        return true;
    }
    // Shared source_articles count.
    let sources_a = frontmatter::parse_list(a.frontmatter.get("source_articles").unwrap_or(""));
    let sources_b = frontmatter::parse_list(b.frontmatter.get("source_articles").unwrap_or(""));
    let set_a: HashSet<&String> = sources_a.iter().collect();
    let shared = sources_b.iter().filter(|s| set_a.contains(s)).count();
    if shared >= DEDUP_SHARED_SOURCES_MIN {
        return true;
    }
    false
}

/// Tokenize a slug into a set of stemmed words (using the project's existing
/// Snowball stemmer). This catches semantic paraphrase with word reordering
/// (`childhood-obesity` vs `obesity-in-children` both stem to {childhood,
/// obes} / {obes, children} - the `in` stopword is filtered).
fn stemmed_token_set(slug: &str) -> HashSet<String> {
    let stopwords: HashSet<&str> =
        ["in", "of", "the", "a", "an", "and", "or", "for", "to", "on"].into_iter().collect();
    slug.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .filter(|s| !stopwords.contains(s.to_lowercase().as_str()))
        .map(|s| crate::biblio::normalizer::stem_phrase(&s.to_lowercase()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Compute the Jaccard similarity between two sets: |A ∩ B| / |A ∪ B|.
#[must_use]
pub fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

/// Rewrite `[[wikilink]]` targets in every page's body to point to the
/// canonical slug. The `slug_map` keys are lowercased old slugs; matching is
/// case-insensitive (per the lint convention where `[[Sugar-Reduction]]`
/// resolves to `sugar-reduction`). Aliases are preserved:
/// `[[old-slug|Alias]]` -> `[[new-slug|Alias]]`.
pub fn rewrite_page_links(pages: &mut [ParsedPage], slug_map: &HashMap<String, String>) {
    if slug_map.is_empty() {
        return;
    }
    // Pre-compile a case-insensitive lookup: lowercase old -> new.
    for page in pages.iter_mut() {
        page.body = rewrite_body_links(&page.body, slug_map);
    }
}

/// Rewrite `[[target]]` and `[[target|alias]]` links in a body string.
pub fn rewrite_body_links(body: &str, slug_map: &HashMap<String, String>) -> String {
    if slug_map.is_empty() {
        return body.to_string();
    }
    let bytes: Vec<char> = body.chars().collect();
    let n = bytes.len();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < n {
        if bytes[i] == '[' && i + 1 < n && bytes[i + 1] == '[' {
            // Found opening [[. Extract the target up to | or ]].
            let start = i + 2;
            let mut j = start;
            let mut target = String::new();
            let mut alias_start: Option<usize> = None;
            let mut closed = false;
            while j < n {
                if bytes[j] == '|' {
                    alias_start = Some(j);
                    break;
                }
                if bytes[j] == ']' && j + 1 < n && bytes[j + 1] == ']' {
                    closed = true;
                    break;
                }
                target.push(bytes[j]);
                j += 1;
            }
            if closed || alias_start.is_some() {
                let trimmed = target.trim();
                if let Some(new_slug) = slug_map.get(&trimmed.to_lowercase()) {
                    // Rewrite this link. Preserve alias if present.
                    out.push_str("[[");
                    out.push_str(new_slug);
                    if let Some(alias_idx) = alias_start {
                        // Copy from the alias separator to the closing ]].
                        out.push('|');
                        let mut k = alias_idx + 1;
                        while k < n && !(bytes[k] == ']' && k + 1 < n && bytes[k + 1] == ']') {
                            out.push(bytes[k]);
                            k += 1;
                        }
                        out.push_str("]]");
                        // Advance past closing ]].
                        i = k + 2;
                    } else {
                        out.push_str("]]");
                        i = j + 2;
                    }
                    continue;
                }
            }
            // Not a match: copy the [[ and continue scanning.
            out.push('[');
            out.push('[');
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}
