//! Wiki static-site export (generate + zip + file helpers).
//!
//! Extracted from the pre-split `wiki_cmd.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use std::path::PathBuf;

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::{frontmatter, storage};

use super::{emit_wiki_progress, WIKI_PIPELINE_TOTAL_STEPS};

/// A single text file to write into the export directory.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExportFile {
    /// Relative path within the export (e.g. "pages/concepts/sugar.html").
    pub path: String,
    /// File content (UTF-8 text).
    pub content: String,
}

/// The complete export bundle sent from the frontend.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteExportBundle {
    /// All HTML/CSS/JS/JSON files to write.
    pub files: Vec<ExportFile>,
    /// Project title for the zip filename + index header.
    pub project_title: String,
}

/// Result of `wiki_generate_export`: the absolute paths + file count.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateExportResult {
    /// Absolute path to the `wiki-export/` directory.
    pub export_dir: String,
    /// Absolute path to `index.html` inside `wiki-export/`.
    pub index_path: String,
    /// Total number of files written (HTML + CSS + JS + JSON + markdown).
    pub file_count: usize,
}

/// Step 1 of the two-step wiki export: generate the static site to
/// `wiki-root/wiki-export/` (persistent, NOT a temp dir).
///
/// The frontend renders all HTML (reusing `renderWikiMarkdown` with
/// `staticMode`), builds a `SiteExportBundle`, and passes it here. The backend
/// writes the files to `wiki-export/`, copies the original Markdown sources,
/// and returns the path so the user can open `index.html` in a browser for
/// testing before zipping.
///
/// The `wiki-export/` directory is cleared on each call (fresh generation).
/// Emits `wiki:progress` events at each step.
#[tauri::command]
pub async fn wiki_generate_export(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
    bundle: SiteExportBundle,
) -> Result<GenerateExportResult, AppError> {
    // Resolve root (brief lock).
    let root = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        storage::resolve_root(&conn)?
    };

    emit_wiki_progress(&app_handle, 10, "Writing HTML pages...");

    let result = generate_export_inner(&root, &bundle);

    match &result {
        Ok(res) => {
            emit_wiki_progress(
                &app_handle,
                100,
                &format!("Generated {} files at {}", res.file_count, res.export_dir),
            );
        }
        Err(e) => {
            emit_wiki_progress(&app_handle, WIKI_PIPELINE_TOTAL_STEPS, &format!("Error: {e}"));
        }
    }
    result
}

/// Step 2 of the two-step wiki export: zip `wiki-export/` into a user-chosen
/// `.zip` file.
///
/// The `path` comes from the frontend's `save()` dialog, matching the
/// established `export_ris_to_file` / `export_project_to_file` pattern.
/// The `wiki-export/` directory is kept on disk after zipping so the user can
/// re-test or re-zip without regenerating.
#[tauri::command]
pub async fn wiki_zip_export(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<String, AppError> {
    // Resolve root (brief lock).
    let root = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        storage::resolve_root(&conn)?
    };

    let export_dir = root.join("wiki-export");
    if !export_dir.exists() {
        return Err(AppError::Validation(
            "Wiki export directory not found. Please generate the website first.".to_string(),
        ));
    }

    emit_wiki_progress(&app_handle, 50, "Zipping export directory...");

    // Zip to a temp file next to the export dir, then move (cross-volume safe).
    let temp_zip = export_dir.with_extension("zip.tmp");
    zip_directory(&export_dir, &temp_zip)?;

    let dest = std::path::PathBuf::from(&path);
    std::fs::rename(&temp_zip, &dest).or_else(|_| {
        std::fs::copy(&temp_zip, &dest)?;
        std::fs::remove_file(&temp_zip)
    })?;

    emit_wiki_progress(&app_handle, 100, &format!("Zip saved to {}", dest.display()));
    Ok(dest.to_string_lossy().to_string())
}

/// Inner worker for `wiki_generate_export`. Writes all bundle files to
/// `wiki-export/`, copies the Markdown sources, and returns the result.
/// Exposed as `pub` so the integration tests can call it directly without a
/// Tauri `State<DbState>` wrapper.
pub fn generate_export_inner(
    root: &std::path::Path,
    bundle: &SiteExportBundle,
) -> Result<GenerateExportResult, AppError> {
    let export_dir = root.join("wiki-export");

    // 1. Clear the export dir for a fresh generation.
    if export_dir.exists() {
        std::fs::remove_dir_all(&export_dir)?;
    }
    std::fs::create_dir_all(&export_dir)?;

    // 2. Write all text files from the bundle.
    for file in &bundle.files {
        let dest = export_dir.join(&file.path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &file.content)?;
    }

    // 3. Copy wiki-generated .md pages into markdown/{type}/ (skip log.md).
    let wiki_dir = root.join("wiki");
    if wiki_dir.exists() {
        copy_wiki_markdown_tree(&wiki_dir, &export_dir.join("markdown"))?;
    }

    // 4. Copy user-document companion .md from raw/ into markdown/sources/.
    copy_user_doc_markdown(root, &export_dir.join("markdown"))?;

    // 5. Count files written.
    let file_count = count_files_recursive(&export_dir)?;

    let index_path = export_dir.join("index.html");
    Ok(GenerateExportResult {
        export_dir: export_dir.to_string_lossy().to_string(),
        index_path: index_path.to_string_lossy().to_string(),
        file_count,
    })
}

/// Recursively count files under a directory.
fn count_files_recursive(dir: &std::path::Path) -> Result<usize, AppError> {
    let mut count = 0;
    count_files_inner(dir, &mut count)?;
    Ok(count)
}

fn count_files_inner(dir: &std::path::Path, count: &mut usize) -> Result<(), AppError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            count_files_inner(&path, count)?;
        } else {
            *count += 1;
        }
    }
    Ok(())
}

/// Recursively copy `wiki/**/*.md` into `dest/`, preserving the directory
/// structure. Skips `log.md` (the system audit log). Reuses
/// `storage::walk_markdown`.
fn copy_wiki_markdown_tree(
    src: &std::path::Path,
    dest_base: &std::path::Path,
) -> Result<(), AppError> {
    let pages = storage::walk_markdown(src, true);
    for path in pages {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "log.md" {
            continue;
        }
        let rel = path.strip_prefix(src).unwrap_or(&path);
        let dest = dest_base.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(path, &dest)?;
    }
    Ok(())
}

/// Copy user-uploaded document companion `.md` files from `raw/` into
/// `dest/sources/`. Only files whose frontmatter `source_kind` starts with
/// `user_` are copied. Article-export `.md` files (no `source_kind` or
/// `type: source` with `content_source: full_text/abstract`) are excluded to
/// avoid redistributing article text.
fn copy_user_doc_markdown(
    root: &std::path::Path,
    dest: &std::path::Path,
) -> Result<usize, AppError> {
    let raw_dir = root.join("raw");
    if !raw_dir.exists() {
        return Ok(0);
    }
    let dest_sources = dest.join("sources");
    std::fs::create_dir_all(&dest_sources)?;
    let mut count = 0;
    for entry in std::fs::read_dir(&raw_dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok((fm, _)) = frontmatter::read_file(&path) else { continue };
        let kind = fm.get("source_kind").unwrap_or("");
        if !kind.starts_with("user_") {
            continue;
        }
        let dest_path = dest_sources.join(path.file_name().unwrap_or_default());
        std::fs::copy(&path, &dest_path)?;
        count += 1;
    }
    Ok(count)
}

/// Zip a directory recursively into `zip_path`. Each file's zip entry name is
/// its path relative to `src_dir`. Maps `zip::result::ZipError` to
/// `AppError::Import` since `AppError` has no `From<ZipError>` impl (the error
/// type is library-specific and does not warrant a dedicated variant).
fn zip_directory(src_dir: &std::path::Path, zip_path: &std::path::Path) -> Result<(), AppError> {
    let file = std::fs::File::create(zip_path)?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let entries = collect_files(src_dir)?;
    for (abs_path, rel_name) in entries {
        writer
            .start_file(&rel_name, options)
            .map_err(|e| AppError::Import(format!("Zip start_file '{rel_name}' failed: {e}")))?;
        let bytes = std::fs::read(&abs_path)?;
        std::io::Write::write_all(&mut writer, &bytes)?;
    }
    writer.finish().map_err(|e| AppError::Import(format!("Zip finish failed: {e}")))?;
    Ok(())
}

/// Recursively collect `(absolute_path, zip_entry_name)` pairs under `base`.
fn collect_files(base: &std::path::Path) -> Result<Vec<(PathBuf, String)>, AppError> {
    let mut out = Vec::new();
    collect_files_inner(base, "", &mut out)?;
    Ok(out)
}

/// Recursive walker for `collect_files`.
fn collect_files_inner(
    dir: &std::path::Path,
    prefix: &str,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), AppError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let zip_name = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if path.is_dir() {
            collect_files_inner(&path, &zip_name, out)?;
        } else {
            out.push((path, zip_name));
        }
    }
    Ok(())
}
