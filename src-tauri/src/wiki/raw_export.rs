//! Raw source preparation for the wiki.
//!
//! Two on-ramps feed `wiki-root/raw/` with Markdown the LLM can ingest:
//!
//! 1. **Article exports** (`export_included_articles`): query the DB for
//!    `status = 'included'` articles and write one `raw/{article_id}.md` per
//!    article using the content fallback `full_text` -> `full_text_ai_summary`
//!    -> `abstract_text`. Only included articles are ever touched - files in
//!    the `fulltext/` attachment dir for rejected/working articles are ignored.
//!
//! 2. **User-added files** (`process_user_files`): scan `raw/` for non-`.md`
//!    files the user dropped in (or added via `wiki_add_raw_file`) and extract
//!    them to companion `.md` files. Supports PDF/TXT/HTML/RTF/CSV/MD/JSON/XML/
//!    source-code without new dependencies (reuses `pdf_extract`, `regex`,
//!    `sha2`, `std`).
//!
//! Both paths are idempotent: a `source_hash` in the companion frontmatter lets
//! us skip unchanged sources on re-runs.

use std::path::{Path, PathBuf};

use lopdf::{Document as LopdfDocument, Object as LopdfObject};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db::article_repo;
use crate::error::AppError;
use crate::models::article::Article;
use crate::utils::pdf_extract;
use crate::utils::sections::{
    extract_captions, extract_sections_with_tables, CaptionKind, SectionKind,
};
use crate::wiki::frontmatter::{self, Frontmatter};

/// Result of a raw preparation run.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawExportReport {
    /// Included articles written (or skipped as unchanged).
    pub articles_written: usize,
    pub articles_skipped: usize,
    /// User files extracted (or skipped as unchanged).
    pub user_files_written: usize,
    pub user_files_skipped: usize,
    /// Files whose extension has no extractor (reported, not fatal).
    pub user_files_unsupported: Vec<String>,
}

// ---------------------------------------------------------------------------
// Article export (Phase 2a)
// ---------------------------------------------------------------------------

/// Resolve which content to use for an article and a label for `content_source`.
/// Order: full_text -> full_text_ai_summary -> abstract_text.
#[must_use]
pub fn article_content(article: &Article) -> (String, &'static str) {
    if let Some(ref ft) = article.full_text {
        let t = ft.trim();
        if !t.is_empty() {
            return (ft.clone(), "full_text");
        }
    }
    if let Some(ref s) = article.full_text_ai_summary {
        let summary = extract_ai_summary_field(s).unwrap_or_else(|| s.clone());
        if !summary.trim().is_empty() {
            return (summary, "ai_summary");
        }
    }
    (article.abstract_text.clone(), "abstract")
}

/// Pull `summary_150_250_words` out of a stored AI summary JSON blob, if present.
fn extract_ai_summary_field(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value.get("summary_150_250_words")?.as_str().map(str::to_string)
}

/// Build the frontmatter for an article-export raw page.
fn article_frontmatter(article: &Article, content_source: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();
    fm.set("id", &article.id);
    fm.set("title", &article.title);
    fm.set("type", "source");
    fm.set("slug", &article.id);
    fm.set("summary", "");
    fm.set("created", &article.imported_at);
    fm.set("updated", &article.imported_at);
    fm.set("status", "draft");
    fm.set("source_articles", &format!("[\"{}\"]", article.id));
    if !article.authors.is_empty() {
        fm.set("authors", &fmt_list(&article.authors));
    }
    if let Some(y) = article.publication_year {
        fm.set("year", &y.to_string());
    }
    if let Some(ref j) = article.journal {
        fm.set("journal", j);
    }
    if let Some(ref d) = article.doi {
        fm.set("doi", d);
    }
    // Store the abstract in frontmatter so the static-site exporter can render
    // metadata-only article stub pages without a second DB query. The body
    // carries the full content (full_text/ai_summary/abstract fallback); the
    // abstract is kept separately here for the copyright-safe stub.
    if !article.abstract_text.is_empty() {
        fm.set("abstract_text", &article.abstract_text);
    }
    if !article.keywords.is_empty() {
        fm.set("keywords", &fmt_list(&article.keywords));
    }
    if !article.tags.is_empty() {
        fm.set("tags", &fmt_list(&article.tags));
    }
    if !article.labels.is_empty() {
        fm.set("labels", &fmt_list(&article.labels));
    }
    fm.set("content_source", content_source);
    fm
}

/// Format a `Vec<String>` as a YAML inline list `[a, b, c]`.
fn fmt_list(items: &[String]) -> String {
    let inner: Vec<String> = items
        .iter()
        .map(|s| {
            if s.contains(',') || s.contains('"') {
                format!("\"{}\"", s.replace('"', "\\\""))
            } else {
                s.clone()
            }
        })
        .collect();
    format!("[{}]", inner.join(", "))
}

/// Build the Markdown body for an article raw page.
///
/// When `content_source == "full_text"`, the content is re-emitted as structured
/// Markdown (T2.4 Phase 2): `## Methods` / `## Results` headings from
/// `extract_sections_with_tables`, preserved GFM tables, and `**Figure N:**`
/// caption lines from `extract_captions`. Other sources (`abstract`, `ai_summary`)
/// pass through unchanged (no structured re-emit).
fn article_body(article: &Article, content: &str, content_source: &str) -> String {
    let year =
        article.publication_year.map(|y| y.to_string()).unwrap_or_else(|| "Unknown".to_string());
    let authors =
        if article.authors.is_empty() { "Unknown".to_string() } else { article.authors.join("; ") };
    let journal = article.journal.clone().unwrap_or_default();
    let meta_line = if journal.is_empty() {
        format!("Authors: {}  |  Year: {}", authors, year)
    } else {
        format!("Authors: {}  |  Year: {}  |  Journal: {}", authors, year, journal)
    };

    let body_content = if content_source == "full_text" {
        structure_full_text(content)
    } else {
        content.to_string()
    };

    format!("# {}\n\n{}\n\n## Content\n\n{}", article.title, meta_line, body_content)
}

/// Re-emit flat full-text as structured Markdown (T2.4 Phase 2).
///
/// - Runs `extract_sections_with_tables` to detect GFM tables (preserved as
///   `SectionKind::Table`) and split the prose into heading-bounded sections.
/// - Emits `## {SectionLabel}` headings for high-value sections (Methods,
///   Results, Discussion, Conclusion, Introduction, Abstract).
/// - Appends preserved GFM tables under their own `## Table N` heading.
/// - Appends `**Figure N:** caption` lines for detected captions.
///
/// Low-value section kinds (`Heading`, `Text`) are emitted as plain body
/// paragraphs without a synthetic heading (graceful degrade).
#[must_use]
fn structure_full_text(text: &str) -> String {
    let sections = extract_sections_with_tables(text);
    let captions = extract_captions(text);

    let mut out = String::new();

    for s in &sections {
        if s.body.trim().is_empty() {
            continue;
        }
        match s.kind {
            SectionKind::Methods
            | SectionKind::Results
            | SectionKind::Discussion
            | SectionKind::Conclusion
            | SectionKind::Introduction
            | SectionKind::Abstract => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("## {}\n\n{}", s.kind.label(), s.body));
            }
            SectionKind::Table => {
                if !out.is_empty() {
                    out.push('\n');
                }
                let heading = s.heading.as_deref().unwrap_or("Table");
                out.push_str(&format!("## {heading}\n\n{}", s.body));
            }
            SectionKind::Figure => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("## Figure\n\n{}", s.body));
            }
            SectionKind::Heading | SectionKind::Text | SectionKind::References => {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&s.body);
            }
        }
    }

    if !captions.is_empty() {
        out.push_str("\n\n## Captions\n\n");
        for c in &captions {
            let label = match c.kind {
                CaptionKind::Figure => "Figure",
                CaptionKind::Table => "Table",
            };
            out.push_str(&format!("**{} {}:** {}\n", label, c.number, c.caption));
        }
    }

    out
}

/// Hash a string for idempotency checks.
fn hash_str(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex_encode(&hasher.finalize())
}

/// Hash raw bytes for idempotency checks.
fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

/// Hash a file's bytes for idempotency checks.
fn hash_file(path: &Path) -> Result<String, AppError> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex_encode(&hasher.finalize()))
}

/// Lowercase hex encoding (no external dep).
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Export all `status = 'included'` articles to `raw/{article_id}.md`.
/// Idempotent: skips articles whose content hash is unchanged since last export.
pub fn export_included_articles(
    conn: &Connection,
    root: &Path,
) -> Result<RawExportReport, AppError> {
    let raw_dir = root.join("raw");
    std::fs::create_dir_all(&raw_dir)?;

    let articles = article_repo::get_articles_by_status(conn, "included")?;
    let mut report = RawExportReport::default();

    for article in &articles {
        let (content, content_source) = article_content(article);
        let body = article_body(article, &content, content_source);
        let source_hash = hash_str(&body);

        let path = raw_dir.join(format!("{}.md", sanitize_filename(&article.id)));

        // Idempotency: skip if existing companion has the same hash.
        if let Ok((existing_fm, _)) = frontmatter::read_file(&path) {
            if existing_fm.get("source_hash") == Some(source_hash.as_str()) {
                report.articles_skipped += 1;
                continue;
            }
        }

        let mut fm = article_frontmatter(article, content_source);
        fm.set("source_hash", &source_hash);
        frontmatter::write_file(&path, &fm, &body)?;
        report.articles_written += 1;
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// User-added files (Phase 2b)
// ---------------------------------------------------------------------------

/// Classification of a user-added file by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawSourceKind {
    UserPdf,
    UserText,
    UserHtml,
    UserRtf,
    UserCsv,
    UserMarkdown,
    UserCode,
    UserData,
    Unsupported,
}

impl RawSourceKind {
    /// The frontmatter `source_kind` token.
    #[must_use]
    pub fn as_token(&self) -> &'static str {
        match self {
            Self::UserPdf => "user_pdf",
            Self::UserText => "user_text",
            Self::UserHtml => "user_html",
            Self::UserRtf => "user_rtf",
            Self::UserCsv => "user_csv",
            Self::UserMarkdown => "user_markdown",
            Self::UserCode => "user_code",
            Self::UserData => "user_data",
            Self::Unsupported => "unsupported",
        }
    }

    /// Classify a file by extension.
    #[must_use]
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "pdf" => Self::UserPdf,
            "txt" | "text" | "log" => Self::UserText,
            "html" | "htm" => Self::UserHtml,
            "rtf" => Self::UserRtf,
            "csv" => Self::UserCsv,
            "md" | "markdown" => Self::UserMarkdown,
            "json" | "xml" => Self::UserData,
            "rs" | "py" | "js" | "ts" | "java" | "c" | "cpp" | "go" | "rb" | "sh" | "yml"
            | "yaml" | "toml" | "ini" | "cfg" => Self::UserCode,
            _ => Self::Unsupported,
        }
    }

    /// Whether this kind has a real extractor (vs `Unsupported`).
    #[must_use]
    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

/// Extract textual content from a user-added file based on its extension.
pub fn extract_user_file(path: &Path) -> Result<(String, RawSourceKind), AppError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let kind = RawSourceKind::from_extension(ext);
    if !kind.is_supported() {
        return Err(AppError::Import(format!(
            "No extractor for extension '.{}' (file: {})",
            ext,
            path.display()
        )));
    }
    let content = match kind {
        RawSourceKind::UserPdf => pdf_extract::extract_pdf_text(path).map_err(AppError::Import)?,
        RawSourceKind::UserText => {
            let raw = std::fs::read_to_string(path)?;
            pdf_extract::extract_txt_text(&raw)
        }
        RawSourceKind::UserHtml => strip_html(&std::fs::read_to_string(path)?)?,
        RawSourceKind::UserRtf => strip_rtf(&std::fs::read_to_string(path)?)?,
        RawSourceKind::UserCsv => csv_to_markdown_table(&std::fs::read_to_string(path)?),
        RawSourceKind::UserMarkdown => std::fs::read_to_string(path)?,
        RawSourceKind::UserCode | RawSourceKind::UserData => {
            let raw = std::fs::read_to_string(path)?;
            format!("```{ext}\n{raw}\n```")
        }
        RawSourceKind::Unsupported => unreachable!(),
    };
    Ok((content, kind))
}

/// Strip HTML tags and decode common entities. Returns plain text.
fn strip_html(html: &str) -> Result<String, AppError> {
    let block_re = regex::Regex::new(r"(?i)</?(p|div|br|h[1-6]|li|tr|table)[^>]*>")
        .map_err(|e| AppError::Import(format!("regex error: {e}")))?;
    let tag_re =
        regex::Regex::new(r"<[^>]*>").map_err(|e| AppError::Import(format!("regex error: {e}")))?;
    let blockified = block_re.replace_all(html, "\n");
    let no_tags = tag_re.replace_all(&blockified, "");
    let decoded = decode_html_entities(&no_tags);
    Ok(collapse_whitespace(&decoded))
}

/// Decode the handful of HTML entities most likely to appear in research notes.
fn decode_html_entities(s: &str) -> String {
    s.replace("\u{0026}amp;", "\u{0026}")
        .replace("\u{0026}lt;", "<")
        .replace("\u{0026}gt;", ">")
        .replace("\u{0026}quot;", "\"")
        .replace("&#39;", "'")
        .replace("\u{0026}nbsp;", " ")
        .replace("\u{0026}ndash;", "-")
}

/// Collapse runs of whitespace into single spaces; preserve newlines.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_blank_line = false;
    for line in s.lines() {
        let trimmed = trim_inner_spaces(line.trim());
        if trimmed.is_empty() {
            if !prev_blank_line {
                out.push('\n');
                prev_blank_line = true;
            }
            continue;
        }
        prev_blank_line = false;
        out.push_str(&trimmed);
        out.push('\n');
    }
    out.trim().to_string()
}

/// Collapse internal runs of spaces/tabs into a single space.
fn trim_inner_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_run = false;
    for ch in s.chars() {
        if ch == ' ' || ch == '\t' {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            in_run = false;
            out.push(ch);
        }
    }
    out
}

/// Strip RTF control words and braces down to plain text.
fn strip_rtf(rtf: &str) -> Result<String, AppError> {
    let control_re = regex::Regex::new(r"\\[a-zA-Z]+-?\d* ?")
        .map_err(|e| AppError::Import(format!("regex error: {e}")))?;
    let brace_re =
        regex::Regex::new(r"[{}\\]").map_err(|e| AppError::Import(format!("regex error: {e}")))?;
    let no_control = control_re.replace_all(rtf, "");
    let cleaned = brace_re.replace_all(&no_control, "");
    // Recover paragraph/line breaks from leftover \par / \line tokens.
    let with_breaks = cleaned.replace("\\par", "\n").replace("\\line", "\n");
    Ok(with_breaks)
}

/// Render a CSV string as a Markdown table.
fn csv_to_markdown_table(csv: &str) -> String {
    let mut lines = csv.lines();
    let Some(header) = lines.next() else {
        return String::new();
    };
    let header_cells: Vec<&str> = header.split(',').map(|c| c.trim().trim_matches('"')).collect();
    let mut out = String::new();
    out.push_str("| ");
    out.push_str(&header_cells.join(" | "));
    out.push_str(" |\n| ");
    out.push_str(&vec!["---"; header_cells.len()].join(" | "));
    out.push_str(" |\n");
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split(',').map(|c| c.trim().trim_matches('"')).collect();
        out.push_str("| ");
        out.push_str(&cells.join(" | "));
        out.push_str(" |\n");
    }
    out
}

/// Build the companion `.md` body for a user file.
fn user_file_body(title: &str, content: &str, kind: RawSourceKind) -> String {
    let note = if kind == RawSourceKind::UserMarkdown {
        ""
    } else {
        "_Extracted from the attached source file._\n\n"
    };
    format!("# {title}\n\n{note}{content}")
}

/// Build the frontmatter for a user-file companion `.md`.
fn user_file_frontmatter(
    slug: &str,
    title: &str,
    source_file: &str,
    kind: RawSourceKind,
    source_hash: &str,
) -> Frontmatter {
    let mut fm = Frontmatter::default();
    fm.set("id", slug);
    fm.set("title", title);
    fm.set("type", "source");
    fm.set("slug", slug);
    fm.set("summary", "");
    fm.set("status", "draft");
    fm.set("source_file", source_file);
    fm.set("source_kind", kind.as_token());
    fm.set("source_hash", source_hash);
    fm.set("content_source", kind.as_token());
    fm.set("links", "[]");
    fm
}

/// Make a slug from a filename stem: lowercase, kebab-case, ascii-only.
fn slugify(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len());
    let mut prev_dash = false;
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Make a string safe for use as a single path component.
fn sanitize_filename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "untitled".to_string()
    } else {
        cleaned
    }
}

/// Try to read a PDF's embedded title from its Info dictionary via `lopdf`.
///
/// Returns `Some(title)` when the PDF has a non-empty `/Title` entry, `None`
/// otherwise (including unparseable or image-only PDFs). The caller falls back
/// to the filename stem when this returns `None`, preserving the prior
/// behavior for PDFs without metadata.
fn extract_pdf_title(path: &Path) -> Option<String> {
    let doc = LopdfDocument::load(path).ok()?;
    let info = doc.trailer.get(b"Info").ok()?;
    let resolved = doc.dereference(info).ok()?.1;
    let info_dict = resolved.as_dict().ok()?;
    let title_obj = info_dict.get(b"Title").ok()?;
    let title_resolved = doc.dereference(title_obj).ok()?.1;
    let text = match title_resolved {
        LopdfObject::String(bytes, _) => String::from_utf8_lossy(bytes).to_string(),
        _ => return None,
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Resolve the effective title + slug for a user file.
///
/// For PDFs, prefers the embedded `/Title` from the Info dictionary when it is
/// non-empty (gives a much cleaner title than the filename stem - e.g.
/// "You Can't Build an AI Workforce..." instead of "user-youcantbuild"). Falls
/// back to the filename stem for PDFs without metadata and for all other file
/// types.
fn resolve_user_file_title(stem: &str, path: &Path, kind: RawSourceKind) -> (String, String) {
    if kind == RawSourceKind::UserPdf {
        if let Some(pdf_title) = extract_pdf_title(path) {
            // Derive the slug from the PDF title too, but keep the `user-`
            // prefix so source pages route correctly. Use the existing
            // `slugify` so the result is kebab-case ascii.
            let slug = format!("user-{}", slugify(&pdf_title));
            return (pdf_title, slug);
        }
    }
    let slug = format!("user-{}", slugify(stem));
    (stem.to_string(), slug)
}

/// Process all non-`.md` files in `raw/`: extract each to a companion `.md`.
/// Idempotent via `source_hash`. For PDFs, the embedded `/Title` (when
/// present) is used as the frontmatter `title` and slug source, giving cleaner
/// wiki source-page names than the raw filename stem.
pub fn process_user_files(root: &Path) -> Result<RawExportReport, AppError> {
    let raw_dir = root.join("raw");
    std::fs::create_dir_all(&raw_dir)?;
    let mut report = RawExportReport::default();

    let entries = std::fs::read_dir(&raw_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            continue; // already markdown
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let kind = RawSourceKind::from_extension(ext);
        if !kind.is_supported() {
            report.user_files_unsupported.push(path.to_string_lossy().to_string());
            continue;
        }

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("untitled").to_string();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("untitled").to_string();
        let (title, slug) = resolve_user_file_title(&stem, &path, kind);
        let companion_path = raw_dir.join(format!("{slug}.md"));

        let source_hash = match hash_file(&path) {
            Ok(h) => h,
            Err(_) => continue,
        };

        // Idempotency check.
        if let Ok((existing_fm, _)) = frontmatter::read_file(&companion_path) {
            if existing_fm.get("source_hash") == Some(source_hash.as_str()) {
                report.user_files_skipped += 1;
                continue;
            }
        }

        let (content, kind) = match extract_user_file(&path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let body = user_file_body(&title, &content, kind);
        let fm = user_file_frontmatter(&slug, &title, &file_name, kind, &source_hash);
        frontmatter::write_file(&companion_path, &fm, &body)?;
        report.user_files_written += 1;
    }

    Ok(report)
}

/// Run both on-ramps: article export + user-file processing.
pub fn prepare_all(conn: &Connection, root: &Path) -> Result<RawExportReport, AppError> {
    let article_report = export_included_articles(conn, root)?;
    let user_report = process_user_files(root)?;
    Ok(RawExportReport {
        articles_written: article_report.articles_written,
        articles_skipped: article_report.articles_skipped,
        user_files_written: user_report.user_files_written,
        user_files_skipped: user_report.user_files_skipped,
        user_files_unsupported: user_report.user_files_unsupported,
    })
}

/// Copy a user-selected file into `raw/` and extract its companion `.md` immediately.
/// Returns the companion `.md` path.
pub fn add_user_file(root: &Path, source_path: &Path) -> Result<PathBuf, AppError> {
    let raw_dir = root.join("raw");
    std::fs::create_dir_all(&raw_dir)?;

    if !source_path.exists() {
        return Err(AppError::Import(format!("File not found: {}", source_path.display())));
    }

    let file_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::Import("Invalid file name".to_string()))?
        .to_string();
    let dest = raw_dir.join(&file_name);
    std::fs::copy(source_path, &dest)?;

    // Extract immediately.
    let stem = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("untitled").to_string();
    let ext = source_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let kind = RawSourceKind::from_extension(ext);
    if !kind.is_supported() {
        return Err(AppError::Import(format!("No extractor for extension '.{ext}'")));
    }
    let (content, kind) = extract_user_file(&dest)?;
    let source_hash = hash_file(&dest)?;
    // Use the PDF-title-aware resolver so added PDFs get the same enriched
    // title + slug as batch-processed ones (`process_user_files`).
    let (title, slug) = resolve_user_file_title(&stem, &dest, kind);
    let companion = raw_dir.join(format!("{slug}.md"));
    let body = user_file_body(&title, &content, kind);
    let fm = user_file_frontmatter(&slug, &title, &file_name, kind, &source_hash);
    frontmatter::write_file(&companion, &fm, &body)?;

    Ok(companion)
}

/// Add raw text content directly (e.g. from a fetched URL) as a companion `.md` file.
/// Returns the path to the companion file.
pub fn add_raw_content(
    root: &Path,
    title: &str,
    content: &str,
    source_label: &str,
) -> Result<PathBuf, AppError> {
    let raw_dir = root.join("raw");
    std::fs::create_dir_all(&raw_dir)?;

    let slug = format!("user-{}", slugify(title));
    let source_hash = hash_bytes(content.as_bytes());
    let kind = RawSourceKind::UserText;
    let companion = raw_dir.join(format!("{slug}.md"));
    let body = user_file_body(title, content, kind);
    let fm = user_file_frontmatter(&slug, title, source_label, kind, &source_hash);
    frontmatter::write_file(&companion, &fm, &body)?;

    Ok(companion)
}

/// List all `.md` files in `raw/` with their parsed frontmatter.
pub fn list_raw_files(root: &Path) -> Result<Vec<(PathBuf, Frontmatter)>, AppError> {
    let raw_dir = root.join("raw");
    if !raw_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&raw_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            let (fm, _body) = frontmatter::read_file(&path)?;
            out.push((path, fm));
        }
    }
    // Sort by title for stable display.
    out.sort_by(|a, b| a.1.get("title").unwrap_or("").cmp(b.1.get("title").unwrap_or("")));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ---- article_content ----

    #[test]
    fn article_content_prefers_full_text() {
        let mut a = sample_article();
        a.full_text = Some("full body".to_string());
        a.full_text_ai_summary = Some("{\"summary_150_250_words\":\"ai\"}".to_string());
        let (content, kind) = article_content(&a);
        assert_eq!(content, "full body");
        assert_eq!(kind, "full_text");
    }

    #[test]
    fn article_content_falls_back_to_ai_summary() {
        let mut a = sample_article();
        a.full_text = None;
        a.full_text_ai_summary = Some("{\"summary_150_250_words\":\"ai digest\"}".to_string());
        let (content, kind) = article_content(&a);
        assert_eq!(content, "ai digest");
        assert_eq!(kind, "ai_summary");
    }

    #[test]
    fn article_content_falls_back_to_raw_ai_summary_when_not_json() {
        let mut a = sample_article();
        a.full_text = None;
        a.full_text_ai_summary = Some("plain summary".to_string());
        let (content, kind) = article_content(&a);
        assert_eq!(content, "plain summary");
        assert_eq!(kind, "ai_summary");
    }

    #[test]
    fn article_content_falls_back_to_abstract() {
        let mut a = sample_article();
        a.full_text = None;
        a.full_text_ai_summary = None;
        let (content, kind) = article_content(&a);
        assert_eq!(content, "the abstract");
        assert_eq!(kind, "abstract");
    }

    #[test]
    fn article_content_ignores_empty_full_text() {
        let mut a = sample_article();
        a.full_text = Some("   ".to_string()); // whitespace-only
        let (content, kind) = article_content(&a);
        assert_eq!(kind, "abstract");
        assert_eq!(content, "the abstract");
    }

    // ---- RawSourceKind classification ----

    #[test]
    fn classifies_known_extensions() {
        assert_eq!(RawSourceKind::from_extension("pdf"), RawSourceKind::UserPdf);
        assert_eq!(RawSourceKind::from_extension("PDF"), RawSourceKind::UserPdf);
        assert_eq!(RawSourceKind::from_extension("txt"), RawSourceKind::UserText);
        assert_eq!(RawSourceKind::from_extension("html"), RawSourceKind::UserHtml);
        assert_eq!(RawSourceKind::from_extension("rtf"), RawSourceKind::UserRtf);
        assert_eq!(RawSourceKind::from_extension("csv"), RawSourceKind::UserCsv);
        assert_eq!(RawSourceKind::from_extension("md"), RawSourceKind::UserMarkdown);
        assert_eq!(RawSourceKind::from_extension("json"), RawSourceKind::UserData);
        assert_eq!(RawSourceKind::from_extension("py"), RawSourceKind::UserCode);
        assert_eq!(RawSourceKind::from_extension("docx"), RawSourceKind::Unsupported);
    }

    #[test]
    fn extract_user_file_txt() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("notes.txt");
        std::fs::write(&path, "hello world").unwrap();
        let (content, kind) = extract_user_file(&path).unwrap();
        assert_eq!(kind, RawSourceKind::UserText);
        assert!(content.contains("hello world"));
    }

    #[test]
    fn extract_user_file_md_passthrough() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("note.md");
        std::fs::write(&path, "# Title\ntext").unwrap();
        let (content, kind) = extract_user_file(&path).unwrap();
        assert_eq!(kind, RawSourceKind::UserMarkdown);
        assert!(content.contains("# Title"));
    }

    #[test]
    fn extract_user_file_code_is_fenced() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("script.py");
        std::fs::write(&path, "print('hi')").unwrap();
        let (content, kind) = extract_user_file(&path).unwrap();
        assert_eq!(kind, RawSourceKind::UserCode);
        assert!(content.contains("```py"));
        assert!(content.contains("print('hi')"));
    }

    #[test]
    fn extract_user_file_json_is_fenced() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("data.json");
        std::fs::write(&path, "{\"a\":1}").unwrap();
        let (content, kind) = extract_user_file(&path).unwrap();
        assert_eq!(kind, RawSourceKind::UserData);
        assert!(content.contains("```json"));
    }

    #[test]
    fn extract_user_file_unsupported_errors() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("file.docx");
        std::fs::write(&path, b"PK\x03\x04").unwrap();
        let result = extract_user_file(&path);
        assert!(result.is_err());
    }

    // ---- strip_html / strip_rtf ----

    #[test]
    fn strip_html_removes_tags_and_decodes_entities() {
        let html = "<h1>Title</h1><p>Hello &amp; goodbye</p><p>Second line</p>";
        let text = strip_html(html).unwrap();
        assert!(!text.contains('<'));
        assert!(text.contains("Title"));
        assert!(text.contains("Hello & goodbye"));
        assert!(text.contains("Second line"));
    }

    #[test]
    fn strip_rtf_removes_control_words() {
        let rtf = "{\\rtf1\\b hello\\par world}";
        let text = strip_rtf(rtf).unwrap();
        assert!(text.contains("hello"));
        assert!(!text.contains("\\b"));
        assert!(!text.contains("{\\rtf1"));
    }

    #[test]
    fn csv_to_markdown_table_renders_header_and_rows() {
        let csv = "name,value\nfoo,1\nbar,2";
        let md = csv_to_markdown_table(csv);
        assert!(md.contains("| name | value |"));
        assert!(md.contains("| --- | --- |"));
        assert!(md.contains("| foo | 1 |"));
        assert!(md.contains("| bar | 2 |"));
    }

    // ---- idempotency ----

    #[test]
    fn process_user_files_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("raw")).unwrap();
        std::fs::write(root.join("raw/notes.txt"), "hello").unwrap();

        let r1 = process_user_files(root).unwrap();
        assert_eq!(r1.user_files_written, 1);
        assert_eq!(r1.user_files_skipped, 0);

        // second run: unchanged -> skipped
        let r2 = process_user_files(root).unwrap();
        assert_eq!(r2.user_files_written, 0);
        assert_eq!(r2.user_files_skipped, 1);

        // companion exists and has correct frontmatter
        let companion = root.join("raw/user-notes.md");
        assert!(companion.exists());
        let (fm, body) = frontmatter::read_file(&companion).unwrap();
        assert_eq!(fm.get("source_file"), Some("notes.txt"));
        assert_eq!(fm.get("source_kind"), Some("user_text"));
        assert!(body.contains("hello"));
    }

    #[test]
    fn process_user_files_re_extracts_when_source_changes() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("raw")).unwrap();
        std::fs::write(root.join("raw/notes.txt"), "v1").unwrap();
        process_user_files(root).unwrap();

        // change the source content
        std::fs::write(root.join("raw/notes.txt"), "v2 with more words").unwrap();
        let r = process_user_files(root).unwrap();
        assert_eq!(r.user_files_written, 1);
        assert_eq!(r.user_files_skipped, 0);

        let (_, body) = frontmatter::read_file(&root.join("raw/user-notes.md")).unwrap();
        assert!(body.contains("v2 with more words"));
    }

    #[test]
    fn process_user_files_reports_unsupported() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("raw")).unwrap();
        std::fs::write(root.join("raw/thing.docx"), "PK").unwrap();
        let r = process_user_files(root).unwrap();
        assert_eq!(r.user_files_written, 0);
        assert_eq!(r.user_files_unsupported.len(), 1);
    }

    #[test]
    fn add_user_file_copies_and_extracts() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("raw")).unwrap();
        let src = tmp.path().join("external.txt");
        std::fs::write(&src, "external content").unwrap();

        let companion = add_user_file(root, &src).unwrap();
        assert!(companion.exists());
        // original is copied into raw/
        assert!(root.join("raw/external.txt").exists());
        let (fm, body) = frontmatter::read_file(&companion).unwrap();
        assert_eq!(fm.get("source_file"), Some("external.txt"));
        assert!(body.contains("external content"));
    }

    // ---- slugify ----

    #[test]
    fn slugify_handles_spaces_and_punctuation() {
        assert_eq!(slugify("My Report!"), "my-report");
        assert_eq!(slugify("foo___bar"), "foo-bar");
        assert_eq!(slugify("UPPER"), "upper");
    }

    // ---- T2.4: structure_full_text (Phase 2 structured re-emit) ----

    #[test]
    fn structure_full_text_emits_methods_heading_for_full_text() {
        // When the source is full_text, the structured re-emit runs
        // extract_sections_with_tables and emits `## Methods` headings for
        // detected high-value sections.
        let text = "## Abstract\nAn abstract.\n\n## Methods\nWe did the study.\n\n## Results\nWe found things.";
        let structured = structure_full_text(text);
        assert!(structured.contains("## Methods"), "Methods heading missing: {structured}");
        assert!(structured.contains("## Results"), "Results heading missing: {structured}");
        assert!(structured.contains("We did the study."), "Methods body missing: {structured}");
    }

    #[test]
    fn structure_full_text_preserves_gfm_table() {
        // A detected pipe-delimited table survives into the structured body as
        // a GFM table (under a `## Table N` heading).
        let text = "## Methods\nBody.\n\n| Col1 | Col2 |\n| a | b |\n| c | d |";
        let structured = structure_full_text(text);
        assert!(structured.contains("| Col1 | Col2 |"), "GFM table header missing: {structured}");
        assert!(structured.contains("| a | b |"), "GFM table row missing: {structured}");
        assert!(structured.contains("---"), "GFM delimiter row missing: {structured}");
    }

    #[test]
    fn structure_full_text_emits_figure_caption_lines() {
        // Detected captions are appended as `**Figure N:** caption` lines.
        let text = "## Methods\nBody.\n\nFigure 1. A bar chart of BMI by age group.";
        let structured = structure_full_text(text);
        assert!(structured.contains("**Figure 1:**"), "figure caption line missing: {structured}");
        assert!(structured.contains("A bar chart of BMI"), "caption text missing: {structured}");
    }

    #[test]
    fn structure_full_text_abstract_source_unchanged() {
        // When the source is NOT full_text (abstract/ai_summary), article_body
        // passes the content through unchanged (no structured re-emit).
        let mut a = sample_article();
        a.full_text = None;
        a.abstract_text = "## Methods\nWe did the study.\n\nPlain abstract text.".to_string();
        let body = article_body(&a, &a.abstract_text.clone(), "abstract");
        // The abstract text is present verbatim (no re-classification).
        assert!(body.contains("Plain abstract text."), "abstract content missing: {body}");
        // No synthetic `## Methods` heading was injected by the structure path
        // (the abstract already contains one, but the point is the structure
        // re-emit did not run).
        assert!(
            !body.contains("## Content\n\n## Methods\n\n"),
            "abstract source should not run the structured re-emit: {body}"
        );
    }

    // ---- helpers ----

    fn sample_article() -> Article {
        Article {
            id: "art-1".to_string(),
            sequence_id: 1,
            status: crate::models::article::ArticleStatus::Included,
            screening_error: false,
            title: "Sample".to_string(),
            abstract_text: "the abstract".to_string(),
            authors: vec!["Doe, J".to_string()],
            publication_year: Some(2024),
            doi: Some("10.1/x".to_string()),
            journal: Some("Nature".to_string()),
            volume: None,
            issue: None,
            start_page: None,
            end_page: None,
            keywords: vec!["tag1".to_string()],
            url: None,
            language: None,
            publisher: None,
            publisher_city: None,
            publisher_address: None,
            issn: None,
            eissn: None,
            journal_index_id: None,
            reference_type: None,
            date: None,
            author_address: None,
            affiliation: None,
            accession_number: None,
            custom_field3: None,
            journal_abbreviation: None,
            journal_iso_abbreviation: None,
            notes: None,
            web_of_science_db: None,
            user_notes: None,
            ris_extras: None,
            duplicate_of: None,
            ai_decision: None,
            ai_reasoning: None,
            ai_confidence: None,
            matched_inclusion_criteria: Vec::new(),
            matched_exclusion_criteria: Vec::new(),
            tags: Vec::new(),
            labels: Vec::new(),
            manual_override: false,
            import_source: None,
            imported_at: "2024-01-01T00:00:00Z".to_string(),
            changed_at: "2024-01-01T00:00:00Z".to_string(),
            screened_at: None,
            data_length: None,
            token_estimate: None,
            actual_tokens: None,
            full_text: None,
            full_text_ai_summary: None,
            num_cited: None,
            num_references: None,
            has_citation_details: false,
            has_reference_details: false,
            has_full_text: false,
            full_text_file_name: None,
            has_figures_or_tables: false,
            is_translated: false,
            translation_status: "none".to_string(),
            translation_error: None,
            translated_at: None,
        }
    }
}
