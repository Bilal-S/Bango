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
/// `Table` / `Figure` variants were added in Tier 2 Phase 1 (caption + table
/// detection). T1.1 only classified heading-derived sections; T2 adds these
/// structural-element kinds so chunking and the FTS index treat tables/figures
/// uniformly.
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
    /// A GFM table block detected by `detect_markdown_tables` (T2.2).
    /// Atomic in chunking: emitted as one chunk regardless of size.
    Table,
    /// A figure/table caption block detected by `extract_captions` (T2.1).
    /// Atomic in chunking: emitted as one chunk regardless of size.
    Figure,
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
            SectionKind::Table => "Table",
            SectionKind::Figure => "Figure",
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
pub(crate) fn compile_static_regex(pattern: &str) -> Regex {
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

// ─── Tier 2 Phase 1: structural-element extraction ──────────────────────────
//
// These pure functions extract figures/tables WITHOUT perturbing the proven
// `classify_sections` (T1.1) chunking pipeline. `extract_sections_with_tables`
// composes them together.

/// The kind of caption detected by `extract_captions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionKind {
    Figure,
    Table,
}

impl CaptionKind {
    /// Stable display label for the variant.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            CaptionKind::Figure => "Figure",
            CaptionKind::Table => "Table",
        }
    }
}

/// A figure/table caption detected by `extract_captions` (T2.1).
///
/// Captions wrap across 3-5 lines in real PDF-extracted text. The `caption`
/// field holds the merged multi-line body; `following_sentence` is best-effort
/// context (often noise in flattened multi-column PDFs).
#[derive(Debug, Clone)]
pub struct Caption {
    pub kind: CaptionKind,
    /// The caption number as a string: "1", "2a", "10".
    pub number: String,
    /// The merged caption body (first line + all continuation lines).
    pub caption: String,
    /// Best-effort: the sentence immediately following the caption start line.
    /// Often noise in multi-column PDFs; the LLM prompt treats it as optional.
    pub following_sentence: Option<String>,
}

/// Tolerance (in characters) for column-separator position overlap when
/// detecting whitespace-aligned tables.
pub const COLUMN_ALIGN_TOLERANCE: usize = 2;

/// Minimum consecutive aligned lines to form a whitespace-detected table.
pub const MIN_TABLE_LINES: usize = 2;

/// Detects caption start lines: "Figure 1.", "Fig. 2a:", "Table 3", "Tab. 4."
/// Case-insensitive, allows optional period/colon after the number.
static CAPTION_START_RE: Lazy<Regex> = Lazy::new(|| {
    compile_static_regex(r"(?i)^\s*(figure|fig\.?|table|tab\.?)\s+(\d+[a-z]?)[:.]?\s*(.*)$")
});

/// Extract figure/table captions from flat text, merging multi-line caption
/// bodies. Pure: no I/O.
///
/// Algorithm: walk line-by-line. When a line matches `CAPTION_START_RE`, collect
/// it as the first caption line, then greedily consume subsequent non-empty
/// lines until one of: a blank line, another caption start line, a
/// markdown/numbered/keyword heading. The first line after the caption block
/// (if non-empty, non-heading, non-caption) is captured as `following_sentence`.
#[must_use]
pub fn extract_captions(text: &str) -> Vec<Caption> {
    let lines: Vec<&str> = text.lines().collect();
    let mut captions = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        if let Some(caption) = parse_caption_at(&lines, &mut i) {
            captions.push(caption);
        } else {
            i += 1;
        }
    }
    captions
}

/// Parse a caption starting at `*idx` and advance `*idx` past it. Returns
/// `None` if `lines[*idx]` is not a caption start line.
fn parse_caption_at(lines: &[&str], idx: &mut usize) -> Option<Caption> {
    let line = lines.get(*idx)?;
    let caps = CAPTION_START_RE.captures(line)?;

    // Safe: capture groups 1, 2, 3 exist because the regex matched.
    let keyword = caps.get(1).map(|m| m.as_str().to_lowercase())?;
    let number = caps.get(2).map(|m| m.as_str().to_string())?;
    let first_body = caps.get(3).map(|m| m.as_str().trim().to_string()).unwrap_or_default();

    let kind = if keyword.starts_with("fig") { CaptionKind::Figure } else { CaptionKind::Table };

    // Greedily consume continuation lines until: blank line, new caption, heading.
    let mut caption_parts: Vec<String> = Vec::new();
    if !first_body.is_empty() {
        caption_parts.push(first_body);
    }
    *idx += 1;
    while *idx < lines.len() {
        let next = lines[*idx];
        if next.trim().is_empty() || CAPTION_START_RE.is_match(next) || is_heading_line(next) {
            break;
        }
        caption_parts.push(next.trim().to_string());
        *idx += 1;
    }

    // Best-effort following sentence: the next non-empty line after the caption
    // block, if it is not itself a new caption or heading.
    let mut following_sentence = None;
    while *idx < lines.len() {
        let next = lines[*idx];
        if next.trim().is_empty() {
            *idx += 1;
            continue;
        }
        if CAPTION_START_RE.is_match(next) || is_heading_line(next) {
            break;
        }
        following_sentence = Some(next.trim().to_string());
        // Advance past the following sentence so the outer loop doesn't re-scan it.
        *idx += 1;
        break;
    }

    let caption = caption_parts.join(" ");
    Some(Caption { kind, number, caption, following_sentence })
}

/// Detect consecutive lines forming a table and return the text with tables
/// replaced by `<!-- TABLE:N -->` placeholders plus the extracted table
/// sections. Pure: no I/O.
///
/// A table block is `MIN_TABLE_LINES`+ consecutive non-empty lines where either:
/// (a) one line uses `|` as a separator (strong signal; a single pipe line is
/// sufficient on its own), OR (b) each line has 2+ column-separator runs (2+
/// spaces/tabs) and the separator positions overlap within
/// `COLUMN_ALIGN_TOLERANCE` characters across the consecutive lines.
#[must_use]
pub fn detect_markdown_tables(text: &str) -> (String, Vec<Section>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut out_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut table_sections = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        if let Some((block_len, true)) = scan_table_block(&lines, i) {
            let block: Vec<&str> = lines[i..i + block_len].to_vec();
            let gfm = to_gfm_table(&block);
            let placeholder_idx = table_sections.len() + 1;
            out_lines.push(format!("<!-- TABLE:{placeholder_idx} -->"));
            let word_count = gfm.split_whitespace().count();
            table_sections.push(Section {
                kind: SectionKind::Table,
                heading: Some(format!("Table {placeholder_idx}")),
                body: gfm,
                word_count,
            });
            i += block_len;
        } else {
            out_lines.push(lines[i].to_string());
            i += 1;
        }
    }

    (out_lines.join("\n"), table_sections)
}

/// Scan forward from `start` and return `(block_length, is_table)` if the lines
/// form a table. Returns `None` if the line at `start` is not part of a table.
fn scan_table_block(lines: &[&str], start: usize) -> Option<(usize, bool)> {
    let first = lines.get(start)?;
    if first.trim().is_empty() {
        return None;
    }

    // Pipe-strong-signal path: consume consecutive non-empty lines that contain `|`.
    if first.contains('|') {
        let mut end = start + 1;
        while end < lines.len() {
            let line = lines[end];
            if line.trim().is_empty() || !line.contains('|') {
                break;
            }
            end += 1;
        }
        // A single pipe line is sufficient (strong signal).
        return Some((end - start, true));
    }

    // Whitespace-alignment path: need MIN_TABLE_LINES consecutive aligned lines.
    let mut block_end = start;
    let mut separator_positions_list: Vec<Vec<usize>> = Vec::new();
    while block_end < lines.len() {
        let line = lines[block_end];
        if line.trim().is_empty() {
            break;
        }
        let positions = whitespace_separator_positions(line);
        if positions.len() < 2 {
            break;
        }
        separator_positions_list.push(positions);
        block_end += 1;
    }

    if separator_positions_list.len() < MIN_TABLE_LINES {
        return None;
    }

    // Check that separator positions overlap within tolerance across all lines.
    if positions_aligned(&separator_positions_list) {
        Some((block_end - start, true))
    } else {
        None
    }
}

/// Return the byte offsets of runs of 2+ whitespace characters (the column
/// separators) in a line.
fn whitespace_separator_positions(line: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].1 == ' ' || chars[i].1 == '\t' {
            let start = chars[i].0;
            let mut run = 1;
            while i + run < chars.len() && (chars[i + run].1 == ' ' || chars[i + run].1 == '\t') {
                run += 1;
            }
            if run >= 2 {
                positions.push(start);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    positions
}

/// `true` if every line's separator positions overlap every other line's within
/// `COLUMN_ALIGN_TOLERANCE` characters.
fn positions_aligned(positions_list: &[Vec<usize>]) -> bool {
    if positions_list.len() < 2 {
        return false;
    }
    for a in 0..positions_list.len() {
        for b in (a + 1)..positions_list.len() {
            if !sets_overlap_within_tolerance(&positions_list[a], &positions_list[b]) {
                return false;
            }
        }
    }
    true
}

/// `true` if for each position in `a` there is a position in `b` within
/// `COLUMN_ALIGN_TOLERANCE` characters (and vice versa).
fn sets_overlap_within_tolerance(a: &[usize], b: &[usize]) -> bool {
    let close = |x: usize, set: &[usize]| {
        set.iter().any(|y| ((x as isize) - (*y as isize)).unsigned_abs() <= COLUMN_ALIGN_TOLERANCE)
    };
    a.iter().all(|x| close(*x, b)) && b.iter().all(|y| close(*y, a))
}

/// Render a raw table block (pipe-delimited or whitespace-aligned) as a GFM
/// Markdown table. Assumes the block has already been validated as a table.
fn to_gfm_table(block: &[&str]) -> String {
    // If the block is pipe-delimited, normalize it directly.
    if block.iter().any(|l| l.contains('|')) {
        let rows: Vec<Vec<String>> = block
            .iter()
            .map(|line| {
                line.trim().trim_matches('|').split('|').map(|c| c.trim().to_string()).collect()
            })
            .collect();
        return render_gfm_rows(&rows);
    }

    // Whitespace-aligned: split each line on 2+ space runs.
    let rows: Vec<Vec<String>> = block
        .iter()
        .map(|line| {
            split_on_whitespace_columns(line).into_iter().map(|s| s.trim().to_string()).collect()
        })
        .collect();
    render_gfm_rows(&rows)
}

/// Split a line into cells on runs of 2+ whitespace characters.
fn split_on_whitespace_columns(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == ' ' || ch == '\t' {
            // Consume the whole whitespace run.
            let mut run = 1;
            while let Some(&next) = chars.peek() {
                if next == ' ' || next == '\t' {
                    chars.next();
                    run += 1;
                } else {
                    break;
                }
            }
            if run >= 2 {
                if !current.trim().is_empty() {
                    cells.push(std::mem::take(&mut current).trim().to_string());
                }
            } else {
                current.push(' ');
            }
        } else {
            current.push(ch);
        }
    }
    if !current.trim().is_empty() {
        cells.push(current.trim().to_string());
    }
    cells
}

/// Render rows as a GFM table. First row is the header; second row is the
/// delimiter (`---`); remaining rows are the body.
fn render_gfm_rows(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    // Determine the max column count to pad rows.
    let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let padded: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut p = r.clone();
            while p.len() < max_cols {
                p.push(String::new());
            }
            p
        })
        .collect();

    let header = &padded[0];
    let mut out = String::new();
    out.push_str("| ");
    out.push_str(&header.join(" | "));
    out.push_str(" |");
    out.push('\n');
    out.push_str("| ");
    out.push_str(&std::iter::repeat_n("---", max_cols).collect::<Vec<_>>().join(" | "));
    out.push_str(" |");
    out.push('\n');
    for row in &padded[1..] {
        out.push_str("| ");
        out.push_str(&row.join(" | "));
        out.push_str(" |");
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// Compose table detection + section classification. Keeps `classify_sections`
/// (T1.1) untouched so the proven chunking pipeline is not perturbed.
///
/// Tables are appended as `SectionKind::Table` sections after the heading-derived
/// sections (tables are high-value regardless of their original position).
#[must_use]
pub fn extract_sections_with_tables(text: &str) -> Vec<Section> {
    let (text_without_tables, table_sections) = detect_markdown_tables(text);
    let mut sections = classify_sections(&text_without_tables);
    sections.extend(table_sections);
    sections
}
