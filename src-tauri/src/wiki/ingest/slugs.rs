//! Shared slug utilities. Single `squeeze_slug` helper used by `author_slug` and `concept_slug`
//! (both previously inlined near-identical dash-squeezing loops).

/// Squeeze raw text to kebab-case: lowercase alphanumerics preserved, every other char → `-`,
/// consecutive dashes collapsed, leading/trailing dashes trimmed. Returns clean middle (no prefix).
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

/// Derive kebab-case slug for an author from normalized name. Prefixed `author-` to avoid
/// collisions with concept pages.
#[must_use]
pub fn author_slug(normalized_name: &str) -> String {
    let squeezed = squeeze_slug(normalized_name);
    if squeezed.is_empty() {
        "author-unnamed".to_string()
    } else {
        format!("author-{squeezed}")
    }
}

/// Derive kebab-case slug for a concept from term text. No prefix - bare kebab-case.
#[must_use]
pub fn concept_slug(term: &str) -> String {
    let squeezed = squeeze_slug(term);
    if squeezed.is_empty() {
        "concept-unnamed".to_string()
    } else {
        squeezed
    }
}

/// Sanitize a slug for use as a filename. Preserves original casing, replaces special chars with `-`.
#[must_use]
pub fn sanitize_slug(slug: &str) -> String {
    let cleaned: String =
        slug.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' }).collect();
    cleaned.trim_matches('-').to_string()
}
