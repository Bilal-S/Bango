//! External-document source pages (Layer 1: pre-seed for uploaded documents).

use std::path::Path;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};
use crate::wiki::raw_export;

use super::slugs::sanitize_slug;

/// A raw source row representing a user-uploaded document.
pub struct UserDocRow {
    pub slug: String,
    pub title: String,
    pub source_kind: String,
    pub source_file: Option<String>,
}

/// Collect user-uploaded documents from `raw/*.md` (files with `source_kind`
/// starting `user_`). Article exports (type `source` but no `source_kind`) are
/// skipped - they are corpus articles, not external documents.
fn collect_user_documents(root: &Path) -> Result<Vec<UserDocRow>, AppError> {
    let raw_files = raw_export::list_raw_files(root)?;
    let mut out = Vec::new();
    for (_path, fm) in raw_files {
        let Some(source_kind) = fm.get("source_kind") else {
            continue;
        };
        if !source_kind.starts_with("user_") {
            continue;
        }
        let slug = fm.get("slug").unwrap_or("").to_string();
        let title = fm.get("title").unwrap_or(&slug).to_string();
        let source_file = fm.get("source_file").map(str::to_string);
        out.push(UserDocRow { slug, title, source_kind: source_kind.to_string(), source_file });
    }
    Ok(out)
}

/// Pre-seed `wiki/sources/{slug}.md` for every user-uploaded document in
/// `raw/`. Each page is a first-class wiki node for the external document so
/// `[[user-slug]]` wikilinks and `[^art-user-slug]` footnote references the
/// LLM emits resolve to a navigable page instead of "Page not found".
///
/// This mirrors how included articles get pre-seeded synthesis pages (slug =
/// article UUID): uploaded documents get pre-seeded source pages (slug =
/// `user-...`). The two on-ramps are symmetric - every raw source has a
/// corresponding wiki node.
///
/// Reviewed (user-edited) source pages are preserved. Documents without a
/// recognized `source_kind` (not starting `user_`) are skipped - they are
/// corpus articles handled by the synthesis pre-seeder.
///
/// Returns the count of pages written.
pub fn preseed_document_source_pages(root: &Path) -> Result<usize, AppError> {
    let sources_dir = root.join("wiki").join("sources");
    std::fs::create_dir_all(&sources_dir)?;
    let docs = collect_user_documents(root)?;
    let mut written = 0;
    for doc in docs {
        let path = sources_dir.join(format!("{}.md", sanitize_slug(&doc.slug)));
        // Respect reviewed pages (user has edited them).
        if let Ok((existing_fm, _)) = frontmatter::read_file(&path) {
            if existing_fm.get("status") == Some("reviewed") {
                continue;
            }
        }
        let (fm, body) = render_document_source_page(&doc);
        frontmatter::write_file(&path, &fm, &body)?;
        written += 1;
    }
    Ok(written)
}

/// Render the frontmatter + body for an external-document source page.
/// Pure function (no I/O) so it is trivially testable.
fn render_document_source_page(doc: &UserDocRow) -> (Frontmatter, String) {
    let mut fm = Frontmatter::default();
    fm.set("id", &doc.slug);
    fm.set("title", &doc.title);
    fm.set("type", "source");
    fm.set("slug", &doc.slug);
    fm.set("summary", &format!("Imported document: {}.", doc.title));
    fm.set("status", "draft");
    // Self-reference: the source_articles field is the document's own slug so
    // the wiki-page-viewer "Sources" footer + provenance chain resolve. The
    // `[^art-{slug}]` footnote ref renderer (Layer 4 routing) also uses this
    // to link the citation back to this page.
    fm.set("source_articles", &format!("[\"{}\"]", doc.slug));
    fm.set("content_source", &doc.source_kind);
    if let Some(ref source_file) = doc.source_file {
        fm.set("source_file", source_file);
    }
    fm.set("links", "[]");

    // NOTE: do NOT emit `# {title}` as the first body line. The page title
    // lives in frontmatter and is rendered separately by the wiki viewer's
    // header (`<h1>{{ page.title }}</h1>`); repeating it in the body would
    // show the title twice on the rendered page.
    let mut body = String::new();
    body.push_str("Imported document added via Add Documents. ");
    body.push_str("The extracted text lives in the corresponding `raw/` companion `.md` ");
    body.push_str("and is available to the wiki as a citable source.\n");
    (fm, body)
}
