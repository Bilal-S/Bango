use crate::db::connection::DbState;
use crate::error::AppError;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub status: String,
    pub article_count: usize,
}

#[tauri::command]
pub fn health_check(db_state: tauri::State<'_, DbState>) -> Result<HealthCheck, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let count: usize =
        conn.query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0)).unwrap_or(0);
    Ok(HealthCheck { status: "ok".to_string(), article_count: count })
}
