//! Shared slug utilities for the wiki ingest pipeline.
//!
//! `author_slug` and `concept_slug` previously each inlined a near-identical
//! dash-squeezing loop (collapse consecutive separators, trim leading/trailing
//! dashes). Both now delegate to the pure `squeeze_slug` helper here so the
//! squeezing logic lives in exactly one place.

/// Squeeze a raw string into a kebab-case slug: lowercase ASCII alphanumerics
/// are preserved, every other char becomes a single `-`, consecutive dashes
/// collapse, and leading/trailing dashes trim to empty.
///
/// Returns the cleaned middle (no prefix). Callers add their own prefix
/// (`author-`, etc.) and handle the empty case.
#[must_use]
pub fn squeeze_slug(raw: &str) -> String {
    let lowercased: String = raw
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    // Collapse consecutive dashes (e.g. "O'Brien, K." -> "o-brien--k" -> "o-brien-k").
    let mut prev_dash = false;
    let mut squeezed = String::with_capacity(lowercased.len());
    for c in lowercased.chars() {
        if c == '-' {
            if !prev_dash {
                squeezed.push('-');
            }
            prev_dash = true;
        } else {
            squeezed.push(c);
            prev_dash = false;
        }
    }
    squeezed.trim_matches('-').to_string()
}

/// Derive a deterministic, kebab-case slug for an author from their normalized
/// name. Prefixed with `author-` to avoid collisions with concept pages
/// (e.g. a researcher named "Author" vs a concept page about authors).
#[must_use]
pub fn author_slug(normalized_name: &str) -> String {
    let squeezed = squeeze_slug(normalized_name);
    if squeezed.is_empty() {
        "author-unnamed".to_string()
    } else {
        format!("author-{squeezed}")
    }
}

/// Derive a deterministic, kebab-case slug for a concept from its term text.
/// No prefix - concept slugs are bare kebab-case.
#[must_use]
pub fn concept_slug(term: &str) -> String {
    let squeezed = squeeze_slug(term);
    if squeezed.is_empty() {
        "concept-unnamed".to_string()
    } else {
        squeezed
    }
}

/// Sanitize a slug for use as a filename. Unlike `author_slug` / `concept_slug`,
/// this preserves the original casing and only replaces special characters
/// (used when writing parsed LLM pages to disk).
#[must_use]
pub fn sanitize_slug(slug: &str) -> String {
    let cleaned: String =
        slug.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' }).collect();
    cleaned.trim_matches('-').to_string()
}
