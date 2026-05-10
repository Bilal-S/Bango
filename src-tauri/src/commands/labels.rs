use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::label_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::client;
use crate::models::label::Label;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelWithCount {
    pub id: String,
    pub name: String,
    pub source: String,
    pub color: Option<String>,
    pub article_count: usize,
}

#[tauri::command]
pub fn get_labels(db_state: State<'_, DbState>) -> Result<Vec<Label>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::get_all_labels(&conn)
}

#[tauri::command]
pub fn get_labels_with_counts(
    db_state: State<'_, DbState>,
) -> Result<Vec<LabelWithCount>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let labels = label_repo::get_all_labels(&conn)?;
    let result: Vec<LabelWithCount> = labels
        .into_iter()
        .map(|label| {
            let count = label_repo::get_article_count_for_label(&conn, &label.id).unwrap_or(0);
            LabelWithCount {
                id: label.id,
                name: label.name,
                source: match label.source {
                    crate::models::label::LabelSource::AiGenerated => "ai_generated".to_string(),
                    crate::models::label::LabelSource::UserCreated => "user_created".to_string(),
                },
                color: label.color,
                article_count: count,
            }
        })
        .collect();
    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLabelRequest {
    pub name: String,
}

#[tauri::command]
pub fn create_label(
    db_state: State<'_, DbState>,
    request: CreateLabelRequest,
) -> Result<Label, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::create_label(&conn, &request.name, "user_created")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameLabelRequest {
    pub id: String,
    pub new_name: String,
}

#[tauri::command]
pub fn rename_label(
    db_state: State<'_, DbState>,
    request: RenameLabelRequest,
) -> Result<Label, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::rename_label(&conn, &request.id, &request.new_name)
}

#[tauri::command]
pub fn delete_label(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::delete_label(&conn, &id)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLabelColorRequest {
    pub id: String,
    pub color: Option<String>,
}

#[tauri::command]
pub fn update_label_color(
    db_state: State<'_, DbState>,
    request: UpdateLabelColorRequest,
) -> Result<Label, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::update_label_color(&conn, &request.id, request.color.as_deref())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestLabelsResult {
    pub labels: Vec<Label>,
}

#[tauri::command]
pub async fn suggest_labels(db_state: State<'_, DbState>) -> Result<SuggestLabelsResult, AppError> {
    let (config, research_aims, inclusion_criteria, exclusion_criteria) = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let aims = criteria_repo::get_all_aims(&conn)?;
        let inc = criteria_repo::get_criteria_by_type(&conn, "inclusion")?;
        let exc = criteria_repo::get_criteria_by_type(&conn, "exclusion")?;
        (config, aims, inc, exc)
    };

    let aims_list: Vec<String> =
        research_aims.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a.text)).collect();
    let inc_list: Vec<String> = inclusion_criteria
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {}", i + 1, c.text))
        .collect();
    let exc_list: Vec<String> = exclusion_criteria
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {}", i + 1, c.text))
        .collect();

    let user_prompt = format!(
        r#"## Task
Generate a set of workflow labels for tracking articles through a systematic literature review screening process.
Labels should represent process states, quality indicators, and decision categories that help researchers organize
their workflow based on the review's research aims and screening criteria.

## Research Aims
{research_aims}

## Inclusion Criteria
{inclusion}

## Exclusion Criteria
{exclusion}

## Response Format
Return JSON exactly matching this schema:
{{
  "labels": ["label-name-1", "label-name-2", ...]
}}

Rules:
- Generate 5-15 labels.
- Each label should be a short, lowercase, hyphenated string (e.g., "priority-read", "strong-methodology", "needs-full-text").
- Labels should be oriented around the research aims and screening criteria — reflecting the types of decisions and
  categorizations a reviewer would need when screening articles against these specific criteria.
- Do not duplicate or overlap concepts.
- Labels should capture workflow states (e.g., review stages), quality assessments (e.g., methodology strength),
  and relevance indicators (e.g., alignment with specific aims)."#,
        research_aims = if aims_list.is_empty() {
            "No research aims defined.".to_string()
        } else {
            aims_list.join("\n")
        },
        inclusion = if inc_list.is_empty() {
            "No inclusion criteria defined.".to_string()
        } else {
            inc_list.join("\n")
        },
        exclusion = if exc_list.is_empty() {
            "No exclusion criteria defined.".to_string()
        } else {
            exc_list.join("\n")
        },
    );

    let system_prompt = "You are a systematic literature review assistant. Generate a set of workflow labels for tracking the screening process based on research aims and screening criteria.";
    let (response, _) = client::send_chat_completion(&config, system_prompt, &user_prompt).await?;

    let json_str = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        AppError::Import(format!("Failed to parse label suggestion response: {}", e))
    })?;
    let label_names: Vec<String> = parsed["labels"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let labels = label_repo::create_labels_batch(&conn, &label_names, "ai_generated")?;

    Ok(SuggestLabelsResult { labels })
}
