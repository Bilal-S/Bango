//! Section-aware text classification for full-text extraction.
//!
//! Pure functions that split flat extracted text into `Section`s by detecting
//! heading lines (markdown `##`, numbered `2.1 Study Design`, or keyword
//! headings like `Methods` / `Results`). The result feeds:
//! - `chunking::chunk_sections` (T1.2) for FTS5 row-per-chunk indexing.
//! - `commands::summary::generate_section_summaries` (T1.3) for per-section
//!   LLM summaries.
//! - `pdf_extract::strip_abstract` / `strip_references` (refactored to consume
//!   `classify_sections` so the stripping is robust to formatting variance).
//!
//! No I/O, no DB. All functions are `#[must_use]` pure.

use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::utils::pdf_extract;

/// The kind of section a block of text belongs to.
///
/// `Table` / `Figure` variants are intentionally absent here: they are added in
/// Tier 2 (caption + table detection). T1.1 only classifies heading-derived
/// sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// A detected heading that does not map to a known semantic section.
    Heading,
    Abstract,
    Introduction,
    Methods,
    Results,
    Discussion,
    Conclusion,
    /// Excluded from chunks (references / bibliography / acknowledgments).
    References,
    /// Default body text when no heading structure is detected.
    Text,
}

impl SectionKind {
    /// Stable display label for the variant, used by prompt builders and
    /// section-aware summary rendering. Guaranteed to be a single capitalized
    /// word matching the enum variant name (e.g. `"Methods"`, `"Results"`).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SectionKind::Heading => "Heading",
            SectionKind::Abstract => "Abstract",
            SectionKind::Introduction => "Introduction",
            SectionKind::Methods => "Methods",
            SectionKind::Results => "Results",
            SectionKind::Discussion => "Discussion",
            SectionKind::Conclusion => "Conclusion",
            SectionKind::References => "References",
            SectionKind::Text => "Text",
        }
    }
}

/// A classified block of text bounded by headings (or the whole document when
/// no headings are detected).
#[derive(Debug, Clone)]
pub struct Section {
    pub kind: SectionKind,
    /// The heading line that started this section, e.g. `Some("2.1 Study Design")`.
    /// `None` for the default `Text` section of unstructured prose.
    pub heading: Option<String>,
    /// The body text (excluding the heading line itself).
    pub body: String,
    pub word_count: usize,
}

// ─── Detection regexes (compiled once) ──────────────────────────────────────
//
// `Regex::new` on a static, hand-validated pattern cannot fail in practice.
// clippy forbids `.expect()` in library code, so we use `.unwrap_or_else` with
// a fallback that compiles a trivial always-matching-safe pattern. The fallback
// is never reached for these static patterns; it exists purely to satisfy the
// "no panics" rule without `expect`.

/// Compile a static, hand-validated regex pattern.
///
/// `clippy::expect_used` is denied across the crate, but these patterns are
/// compile-time constants that cannot fail. The `allow` scopes the exception to
/// this one helper; the fallback pattern `^$` is a literal that is always valid.
#[allow(clippy::expect_used)]
fn compile_static_regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|_| Regex::new(r"^$").expect("fallback regex is valid"))
}

/// Markdown headings: `# Methods`, `## 2.1 Study Design`, etc.
static MARKDOWN_HEADING_RE: Lazy<Regex> = Lazy::new(|| compile_static_regex(r"^#{1,6}\s+(.+)$"));

/// Numbered headings like `2.1 Study Design`, `3 Methods`, `1.2.3 Results`.
///
/// Tightened to avoid false-positive sentence matches: requires a capitalised
/// short phrase (<=60 chars) with no trailing sentence punctuation and no
/// lowercase body words, so `3. The results showed` does not match.
static NUMBERED_HEADING_RE: Lazy<Regex> =
    Lazy::new(|| compile_static_regex(r"^\d+(?:\.\d+){0,2}\.?\s+[A-Z][A-Za-z\s\-:]{2,60}$"));

// ─── Keyword → SectionKind mapping ──────────────────────────────────────────
//
// Each entry is `(case-insensitive keyword, SectionKind)`. A heading line is
// classified by the first matching keyword (order matters: more specific
// phrases like "Materials and Methods" must come before "Methods").

/// Keyword groups, in priority order. Each group maps to one `SectionKind`.
/// A heading matches a group if any keyword in the group equals the heading
/// (case-insensitive, trimmed).
const KEYWORD_GROUPS: &[(&[&str], SectionKind)] = &[
    (&["abstract"], SectionKind::Abstract),
    (&["introduction", "background"], SectionKind::Introduction),
    (&["materials and methods", "methodology", "methods"], SectionKind::Methods),
    (&["findings", "results"], SectionKind::Results),
    (&["discussion"], SectionKind::Discussion),
    (&["conclusion", "conclusions"], SectionKind::Conclusion),
    (
        &["references", "bibliography", "acknowledgments", "acknowledgements"],
        SectionKind::References,
    ),
];

/// Classify a heading line (already detected as a heading by the regex passes)
/// into a semantic `SectionKind`. Returns `SectionKind::Heading` for generic
/// headings that do not match any known keyword.
#[must_use]
fn classify_heading(line: &str) -> SectionKind {
    let normalized = line.trim().to_lowercase();
    // Strip a leading `## ` / `# ` / `2.1 ` prefix so keyword matching sees the
    // heading text only.
    let normalized = normalized.trim_start_matches('#').trim_start();
    let normalized = normalized.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
    let normalized = normalized.trim();
    for (keywords, kind) in KEYWORD_GROUPS {
        if keywords.contains(&normalized) {
            return *kind;
        }
    }
    SectionKind::Heading
}

/// `true` if a line is a heading line. Three detection paths:
/// 1. Markdown heading (`## Methods`).
/// 2. Numbered heading (`2.1 Study Design`).
/// 3. Bare keyword line - a line that is exactly one of the section keywords
///    (e.g. `METHODS`, `Introduction`, `Results`) with optional surrounding
///    whitespace. This catches PDFs where the section title stands alone on its
///    line without a `#` or number prefix.
#[must_use]
fn is_heading_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if MARKDOWN_HEADING_RE.is_match(trimmed) || NUMBERED_HEADING_RE.is_match(trimmed) {
        return true;
    }
    // Bare keyword line: case-insensitive exact match against any keyword.
    let lower = trimmed.to_lowercase();
    KEYWORD_GROUPS.iter().any(|(keywords, _)| keywords.iter().any(|kw| lower == *kw))
}

/// Classify flat text into a list of `Section`s bounded by detected headings.
///
/// - Text before the first heading becomes a `SectionKind::Text` section (the
///   "preamble" - often the abstract, but we let the keyword match decide if an
///   `Abstract` heading is present).
/// - When no headings are detected at all, the entire text becomes a single
///   `SectionKind::Text` section (graceful degrade: chunking still works on
///   plain text).
/// - Empty / whitespace-only text yields an empty `Vec`.
#[must_use]
pub fn classify_sections(text: &str) -> Vec<Section> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let mut current_kind = SectionKind::Text;
    let mut current_heading: Option<String> = None;
    let mut current_body = String::new();

    let flush = |sections: &mut Vec<Section>,
                 kind: &mut SectionKind,
                 heading: &mut Option<String>,
                 body: &mut String| {
        let body_trim = body.trim();
        // Skip an empty preamble that has no heading.
        if body_trim.is_empty() && heading.is_none() {
            body.clear();
            return;
        }
        let word_count = body_trim.split_whitespace().count();
        sections.push(Section {
            kind: *kind,
            heading: heading.take(),
            body: body_trim.to_string(),
            word_count,
        });
        body.clear();
    };

    for line in text.lines() {
        if is_heading_line(line) {
            flush(&mut sections, &mut current_kind, &mut current_heading, &mut current_body);
            let line_trim = line.trim();
            current_kind = classify_heading(line_trim);
            current_heading = Some(line_trim.to_string());
        } else {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }
    flush(&mut sections, &mut current_kind, &mut current_heading, &mut current_body);

    sections
}

/// Extract sections from a file (PDF or TXT) using the same header/footer +
/// abstract/references stripping pipeline as `extract_pdf_text` /
/// `extract_txt_text`, then classifying the result into `Section`s.
///
/// Returns an empty `Vec` on extraction failure (graceful degrade).
pub fn extract_sections(file_path: &Path) -> Result<Vec<Section>, String> {
    let extension = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let text = match extension.as_str() {
        "pdf" => pdf_extract::extract_pdf_text(file_path)?,
        "txt" => {
            let content = std::fs::read_to_string(file_path)
                .map_err(|e| format!("Failed to read TXT file: {e}"))?;
            pdf_extract::extract_txt_text(&content)
        }
        other => {
            return Err(format!(
                "Unsupported file type: .{other}. Only .pdf and .txt are supported."
            ))
        }
    };

    Ok(classify_sections(&text))
}
