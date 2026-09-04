/// Placeholder DOIs that should be treated as missing (commonly emitted by
/// Citation Chaser and other tools).
const DOI_PLACEHOLDERS: &[&str] = &["NA", "N/A", "NULL", "NONE", "-"];

/// URL/scheme prefixes commonly prepended to DOIs by exporters.
const DOI_PREFIXES: &[&str] =
    &["https://doi.org/", "http://doi.org/", "https://dx.doi.org/", "http://dx.doi.org/", "doi:"];

/// Strip `prefix` from `s` ASCII case-insensitively. Prefixes are pure ASCII,
/// so a match implies a char boundary at `prefix.len()`; the boundary guard
/// keeps the slice panic-free for non-ASCII leading bytes.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let n = prefix.len();
    (s.len() >= n && s.is_char_boundary(n) && s[..n].eq_ignore_ascii_case(prefix)).then(|| &s[n..])
}

/// Canonical DOI form: trim, strip one leading `doi.org`/`dx.doi.org`/`doi:`
/// prefix (ASCII case-insensitive), trim again, filter placeholders, and
/// ASCII-lowercase. ASCII-lowercase mirrors SQLite `LOWER()`, keeping this
/// helper and the v009 healing SQL byte-equivalent.
/// Returns `None` for empty, whitespace, or placeholder values.
/// Single source of truth for DOI identity across all import channels.
#[must_use]
pub fn normalize_doi(doi: Option<&str>) -> Option<String> {
    let raw = doi?.trim();
    let stripped = DOI_PREFIXES.iter().find_map(|p| strip_prefix_ci(raw, p)).unwrap_or(raw).trim();
    if stripped.is_empty() {
        return None;
    }
    // Check against known placeholders (case-insensitive)
    for placeholder in DOI_PLACEHOLDERS {
        if stripped.eq_ignore_ascii_case(placeholder) {
            return None;
        }
    }
    Some(stripped.to_ascii_lowercase())
}
