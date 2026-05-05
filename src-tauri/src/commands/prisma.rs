use tauri::State;

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::prisma::data::{self, PrismaData};
use crate::prisma::svg;

#[tauri::command]
pub fn get_prisma_data(db_state: State<'_, DbState>) -> Result<PrismaData, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    data::compute_prisma_data(&conn)
}

#[tauri::command]
pub fn get_prisma_svg(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let prisma_data = data::compute_prisma_data(&conn)?;
    Ok(svg::render_prisma_svg(&prisma_data))
}
