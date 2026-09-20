//! Raw source preparation for the wiki. Two on-ramps feed `wiki-root/raw/`:
//! 1. Article exports: included articles → `raw/{article_id}.md` (full AI-summary blob > abstract; full text is never exported).
//! 2. User-added files: PDF/TXT/HTML/RTF/CSV/MD/JSON/XML/code → companion `.md` (pdf_extract + regex).
//!
//! Both idempotent via `source_hash` in companion frontmatter.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use lopdf::{Document as LopdfDocument, Object as LopdfObject};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db::article_repo;
use crate::error::AppError;
use crate::models::article::Article;
use crate::utils::pdf_extract;
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
    /// Whether the operation was cancelled mid-loop (Phase A cancel).
    #[serde(default)]
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// Article export (Phase 2a)
// ---------------------------------------------------------------------------

/// Resolve article content for wiki export: full AI-summary blob rendered as
/// structured Markdown -> legacy plain-text blob -> abstract_text. Article
/// full text is NEVER exported (wikifix-final Change 1).
#[must_use]
pub fn article_content(article: &Article) -> (String, &'static str) {
    if let Some(ref s) = article.full_text_ai_summary {
        let t = s.trim();
        if !t.is_empty() {
            if let Some(rendered) = render_summary_blob(s) {
                return (rendered, "ai_summary");
            }
            // Legacy plain-text blob (pre-JSON schema): pass through verbatim.
            if !t.starts_with('{') {
                return (s.clone(), "ai_summary");
            }
            // Valid JSON blob with no usable fields: fall through to abstract.
        }
    }
    (article.abstract_text.clone(), "abstract")
}

/// Render the unified AI-summary blob as structured Markdown: summary, key
/// insights, keywords, field, then per-section summaries with key points and
/// typed facts. No word-count caps (user ruling 3). `None` when the blob
/// yields no usable content.
#[must_use]
fn render_summary_blob(raw: &str) -> Option<String> {
    let parsed = crate::wiki::ingest::synthesis::parse_ai_summary(raw)?;
    let mut out = String::new();
    if let Some(summary) = parsed.summary.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("## Summary\n\n{summary}\n\n"));
    }
    if !parsed.key_insights.is_empty() {
        out.push_str("## Key Insights\n\n");
        for insight in &parsed.key_insights {
            out.push_str(&format!("- {insight}\n"));
        }
        out.push('\n');
    }
    if !parsed.keywords.is_empty() {
        out.push_str(&format!("## Keywords\n\n{}\n\n", parsed.keywords.join(", ")));
    }
    let field = match (&parsed.field, &parsed.subfield) {
        (Some(f), Some(sf)) => format!("{f} > {sf}"),
        (Some(f), None) => f.clone(),
        (None, Some(sf)) => sf.clone(),
        (None, None) => String::new(),
    };
    if !field.is_empty() {
        out.push_str(&format!("## Field\n\n{field}\n\n"));
    }
    if !parsed.theoretical_frameworks.is_empty() {
        out.push_str("## Theoretical Frameworks\n\n");
        for fw in &parsed.theoretical_frameworks {
            match fw.usage.as_deref().filter(|u| !u.trim().is_empty()) {
                Some(usage) => out.push_str(&format!("- {}: {usage}\n", fw.name)),
                None => out.push_str(&format!("- {}\n", fw.name)),
            }
        }
        out.push('\n');
    }
    if !parsed.section_summaries.is_empty() {
        out.push_str("## Section Summaries\n\n");
        for sec in &parsed.section_summaries {
            out.push_str(&format!("### {}\n\n", sec.section));
            if !sec.summary.trim().is_empty() {
                out.push_str(&sec.summary);
                out.push_str("\n\n");
            }
            for point in &sec.key_points {
                out.push_str(&format!("- {point}\n"));
            }
            let typed_facts = [
                ("Study design", sec.study_design.as_deref()),
                ("Sample size", sec.sample_size.as_deref()),
                ("Effect size", sec.effect_size.as_deref()),
                ("95% CI", sec.confidence_interval.as_deref()),
            ];
            for (label, value) in typed_facts {
                if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
                    out.push_str(&format!("- {label}: {v}\n"));
                }
            }
            out.push('\n');
        }
    }
    let rendered = out.trim_end().to_string();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
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
    /* Store abstract in frontmatter so static-site exporter can render metadata-only
    article stub pages without a second DB query. Body carries full content
    (full_text/ai_summary/abstract); abstract stays separate for copyright-safe stub. */
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

/// Build the Markdown body for an article raw page. Content is summary-scale
/// (blob render or abstract) and passes through unchanged; full text is never
/// exported (wikifix-final Change 1).
fn article_body(article: &Article, content: &str) -> String {
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

    format!("# {}\n\n{}\n\n## Content\n\n{}", article.title, meta_line, content)
}

/// Hash a string for content-based idempotency checks.
fn hash_str(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex_encode(&hasher.finalize())
}

/// Hash raw bytes for content-based idempotency checks.
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

/// Progress callback `(index, total, article_id)` for `write_article_exports`. Mirrors
/// `ChunkProgressCb`. Caller emits `wiki:progress` events in the 0-15% range.
pub type ArticleExportProgressCb<'a> = Option<&'a dyn Fn(usize, usize, &str)>;

/// Load all `status = 'included'` articles. Only phase needing DB lock; returned
/// `Vec<Article>` carries `full_text` in memory so `write_article_exports` runs lock-free.
/// Splitting load from write follows the `attach_full_text_split` pattern.
pub fn load_included_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    article_repo::get_articles_by_status(conn, "included")
}

/// Write loaded articles to `raw/{article_id}.md`. Idempotent: skips when content hash is
/// unchanged. Does NOT touch DB - `Article` carries `full_text` in memory. `cancel` checked
/// at each iteration; on signal returns `Ok(report)` with `report.cancelled = true`.
/// Prefer `load_included_articles` + `write_article_exports` in new callers.
pub fn write_article_exports(
    root: &Path,
    articles: &[Article],
    progress_cb: ArticleExportProgressCb<'_>,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<RawExportReport, AppError> {
    let raw_dir = root.join("raw");
    std::fs::create_dir_all(&raw_dir)?;

    let total = articles.len();
    let mut report = RawExportReport::default();

    for (idx, article) in articles.iter().enumerate() {
        /* Phase A cancel: checked before each article so Stop works during
        "Preparing raw sources..." (0-15%). Already-written files preserved;
        cancelled report surfaces via `report.cancelled`. */
        if cancel.is_some_and(|t| t.load(Ordering::SeqCst)) {
            report.cancelled = true;
            return Ok(report);
        }

        let (content, content_source) = article_content(article);
        let body = article_body(article, &content);
        let source_hash = hash_str(&body);

        let path = raw_dir.join(format!("{}.md", sanitize_filename(&article.id)));

        // Idempotency: skip if existing companion has the same hash.
        if let Ok((existing_fm, _)) = frontmatter::read_file(&path) {
            if existing_fm.get("source_hash") == Some(source_hash.as_str()) {
                report.articles_skipped += 1;
                if let Some(cb) = progress_cb {
                    cb(idx + 1, total, &article.id);
                }
                continue;
            }
        }

        let mut fm = article_frontmatter(article, content_source);
        fm.set("source_hash", &source_hash);
        frontmatter::write_file(&path, &fm, &body)?;
        report.articles_written += 1;
        if let Some(cb) = progress_cb {
            cb(idx + 1, total, &article.id);
        }
    }

    Ok(report)
}

/// Export all included articles to `raw/{article_id}.md`. Legacy single-call wrapper.
/// Prefer `load_included_articles` + `write_article_exports` in new callers.
pub fn export_included_articles(
    conn: &Connection,
    root: &Path,
) -> Result<RawExportReport, AppError> {
    let articles = load_included_articles(conn)?;
    write_article_exports(root, &articles, None, None)
}

// ---------------------------------------------------------------------------
// User-added files (Phase 2b)
// ---------------------------------------------------------------------------

/// Classification of a user-added file by extension.
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
        RawSourceKind::Unsupported => {
            // Unreachable in practice (callers gate on `is_supported()`), but a
            // hard error beats a panic if that gate ever regresses.
            return Err(AppError::Validation(
                "unsupported raw source kind passed the is_supported() gate".to_string(),
            ));
        }
    };
    Ok((content, kind))
}

/// Strip HTML tags and decode common entities. Returns plain text.
pub fn strip_html(html: &str) -> Result<String, AppError> {
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
pub fn strip_rtf(rtf: &str) -> Result<String, AppError> {
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
pub fn csv_to_markdown_table(csv: &str) -> String {
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
pub fn slugify(stem: &str) -> String {
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

/// Try to read a PDF's embedded `/Title` from its Info dictionary via `lopdf`.
/// Returns `None` when absent, empty, or unparseable (caller falls back to stem).
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

/// Resolve title + slug for a user file. PDFs: prefer embedded `/Title` from Info dictionary.
/// Falls back to filename stem for PDFs without metadata and for all other file types.
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

/// Process non-`.md` files in `raw/`: extract to companion `.md`. Idempotent via `source_hash`.
/// PDFs use embedded `/Title` for cleaner wiki source-page names.
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

/// Copy a user-selected file into `raw/` and immediately extract its companion `.md`.
/// Returns the companion path.
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

/// Add raw text content (e.g. from fetched URL) as a companion `.md` file. Returns the companion path.
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

/// List all `.md` files in `raw/` with parsed frontmatter, sorted by title.
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
