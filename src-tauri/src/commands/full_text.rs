use std::path::PathBuf;

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::utils::pdf_extract;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullTextAttachResult {
    pub success: bool,
    pub message: String,
    pub word_count: usize,
}

/// Compute the fulltext storage directory using the same logic as app_settings.
fn compute_storage_dir(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    let configured = app_settings_repo::get_setting(conn, "fulltext_storage_dir")?;
    let default_path = dirs::document_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Bango")
        .join("documents");

    let effective = if let Some(ref p) = configured {
        if !p.is_empty() {
            PathBuf::from(p)
        } else {
            default_path
        }
    } else {
        default_path
    };

    std::fs::create_dir_all(&effective)
        .map_err(|e| AppError::Import(format!("Failed to create storage dir: {e}")))?;
    Ok(effective)
}

/// Attach a full-text file (PDF or TXT) to an article.
/// Extracts text content, stores in DB, and copies file to storage directory.
#[tauri::command]
pub fn attach_full_text(
    db_state: tauri::State<'_, DbState>,
    _app_handle: tauri::AppHandle,
    article_id: String,
    file_path: String,
) -> Result<FullTextAttachResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let storage_dir = compute_storage_dir(&conn)?;

    let source_path = PathBuf::from(&file_path);
    if !source_path.exists() {
        return Err(AppError::Import(format!("File not found: {file_path}")));
    }

    let extension = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    // Extract text based on file type
    let full_text = match extension.as_str() {
        "pdf" => pdf_extract::extract_pdf_text(&source_path).map_err(AppError::Import),
        "txt" => {
            let content = std::fs::read_to_string(&source_path)?;
            Ok(pdf_extract::extract_txt_text(&content))
        }
        other => Err(AppError::Import(format!(
            "Unsupported file type: .{other}. Only .pdf and .txt files are supported."
        ))),
    }?;

    let word_count = full_text.split_whitespace().count();

    // Build destination filename: {article_id}_{original_filename}
    let original_name = source_path.file_name().and_then(|n| n.to_str()).unwrap_or("document");
    let dest_filename = format!("{article_id}_{original_name}");
    let dest_path = storage_dir.join(&dest_filename);

    // Copy file to storage directory
    std::fs::copy(&source_path, &dest_path)
        .map_err(|e| AppError::Import(format!("Failed to copy file to storage: {e}")))?;

    // Update database
    article_repo::update_full_text(&conn, &article_id, &full_text, &dest_filename)?;

    // Create audit entry
    crate::db::audit_repo::create_entry(
        &conn,
        &article_id,
        "import",
        None,
        None,
        Some(&format!("Full text attached: {original_name}")),
        "user",
    )?;

    Ok(FullTextAttachResult {
        success: true,
        message: format!("Full text extracted ({word_count} words)"),
        word_count,
    })
}

/// Delete the full-text attachment for an article.
/// Removes file from storage and clears DB references.
#[tauri::command]
pub fn delete_full_text(
    db_state: tauri::State<'_, DbState>,
    _app_handle: tauri::AppHandle,
    article_id: String,
) -> Result<bool, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    // Get the file name before clearing
    let file_name = article_repo::get_full_text_file_name(&conn, &article_id)?;

    if let Some(ref name) = file_name {
        let storage_dir = compute_storage_dir(&conn)?;
        let file_path = storage_dir.join(name);

        // Delete the file
        if file_path.exists() {
            std::fs::remove_file(&file_path)
                .map_err(|e| AppError::Import(format!("Failed to delete file: {e}")))?;
        }
    }

    // Clear DB references
    article_repo::clear_full_text(&conn, &article_id)?;

    // Create audit entry
    crate::db::audit_repo::create_entry(
        &conn,
        &article_id,
        "import",
        None,
        None,
        Some("Full text attachment removed"),
        "user",
    )?;

    Ok(true)
}

/// Read the full-text content for an article from the database.
#[tauri::command]
pub fn read_full_text(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<Option<String>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let article = article_repo::get_article_by_id(&conn, &article_id)?;
    Ok(article.full_text)
}

/// Read the bytes of a full-text attachment file.
/// Used by the frontend to create Blob URLs for inline PDF viewing.
#[tauri::command]
pub fn read_full_text_file_bytes(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<Option<Vec<u8>>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let file_name = article_repo::get_full_text_file_name(&conn, &article_id)?;

    match file_name {
        Some(name) => {
            let storage_dir = compute_storage_dir(&conn)?;
            let file_path = storage_dir.join(&name);
            if !file_path.exists() {
                return Ok(None);
            }
            let bytes = std::fs::read(&file_path)?;
            Ok(Some(bytes))
        }
        None => Ok(None),
    }
}

/// Get the absolute file path for a full-text attachment.
/// Returns None if no file is attached.
#[tauri::command]
pub fn get_full_text_file_path(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<Option<String>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let file_name = article_repo::get_full_text_file_name(&conn, &article_id)?;

    match file_name {
        Some(name) => {
            let storage_dir = compute_storage_dir(&conn)?;
            let file_path = storage_dir.join(&name);
            Ok(Some(file_path.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}
