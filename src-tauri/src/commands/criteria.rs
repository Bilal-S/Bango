use serde::Deserialize;
use tauri::State;

use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::error::AppError;
use crate::models::criterion::{Criterion, ResearchAim};

#[tauri::command]
pub fn get_research_aims(db_state: State<'_, DbState>) -> Result<Vec<ResearchAim>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::get_all_aims(&conn)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAimRequest {
    pub text: String,
}

#[tauri::command]
pub fn create_research_aim(
    db_state: State<'_, DbState>,
    request: CreateAimRequest,
) -> Result<ResearchAim, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::create_aim(&conn, &request.text)
}

#[tauri::command]
pub fn delete_research_aim(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::delete_aim(&conn, &id)
}

#[tauri::command]
pub fn get_criteria(db_state: State<'_, DbState>) -> Result<Vec<Criterion>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::get_all_criteria(&conn)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCriterionRequest {
    pub criterion_type: String,
    pub text: String,
    pub priority: String,
}

#[tauri::command]
pub fn create_criterion(
    db_state: State<'_, DbState>,
    request: CreateCriterionRequest,
) -> Result<Criterion, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::create_criterion(
        &conn,
        &request.criterion_type,
        &request.text,
        &request.priority,
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCriterionRequest {
    pub id: String,
    pub text: String,
    pub priority: String,
}

#[tauri::command]
pub fn update_criterion(
    db_state: State<'_, DbState>,
    request: UpdateCriterionRequest,
) -> Result<Criterion, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::update_criterion(&conn, &request.id, &request.text, &request.priority)
}

#[tauri::command]
pub fn delete_criterion(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::delete_criterion(&conn, &id)
}
