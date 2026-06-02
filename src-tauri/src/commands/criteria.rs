use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::client;
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

// ── AI-assisted criteria commands ─────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateCriteriaRequest {
    pub criterion_type: String, // "inclusion" or "exclusion"
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateCriteriaResult {
    pub criteria: Vec<Criterion>,
}

#[tauri::command]
pub async fn generate_criteria(
    db_state: State<'_, DbState>,
    request: GenerateCriteriaRequest,
) -> Result<GenerateCriteriaResult, AppError> {
    let criterion_type = request.criterion_type;
    if criterion_type != "inclusion" && criterion_type != "exclusion" {
        return Err(AppError::Validation(
            "criterion_type must be 'inclusion' or 'exclusion'".to_string(),
        ));
    }

    let (config, aims) = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let aims = criteria_repo::get_all_aims(&conn)?;
        if aims.is_empty() {
            return Err(AppError::Validation(
                "Research aims must be defined before generating criteria".to_string(),
            ));
        }
        (config, aims)
    };

    let aims_list: Vec<String> = aims
        .iter()
        .enumerate()
        .map(|(i, a)| format!("{}. {}", i + 1, a.text))
        .collect();

    let type_label = if criterion_type == "inclusion" {
        "inclusion"
    } else {
        "exclusion"
    };

    let system_prompt = "You are a systematic literature review assistant. Based on the research aims provided, \
        suggest appropriate criteria for screening research papers in a systematic review.";

    let user_prompt = format!(
        r#"## Task
First, determine the field of study based on the research aims below.
Then suggest up to 5 {type_label} criteria that would be appropriate for this type of research.
Criteria should follow common scientific and methodological patterns for systematic reviews in this domain.

## Research Aims
{research_aims}

## Response Format
Return JSON exactly matching this schema:
{{
  "criteria": [
    {{ "text": "Description of the criterion", "priority": "standard" }},
    ...
  ]
}}

Rules:
- Priority values: critical, high, standard, low, optional. Use "standard" unless there's a clear reason for a different priority.
- Each criterion should be clear, specific, and actionable.
- Criteria should be directly relevant to the research aims.
- Do not duplicate or overlap concepts."#,
        type_label = type_label,
        research_aims = aims_list.join("\n"),
    );

    let (response, _) = client::send_chat_completion(&config, system_prompt, &user_prompt).await?;

    let json_str = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        AppError::Import(format!("Failed to parse criteria generation response: {}", e))
    })?;

    let items: Vec<(String, String)> = parsed["criteria"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let text = v["text"].as_str()?.to_string();
                    let priority = v["priority"]
                        .as_str()
                        .unwrap_or("standard")
                        .to_string();
                    Some((text, priority))
                })
                .collect()
        })
        .unwrap_or_default();

    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let criteria: Vec<Criterion> = items
        .into_iter()
        .filter_map(|(text, priority)| {
            criteria_repo::create_criterion(&conn, &criterion_type, &text, &priority).ok()
        })
        .collect();

    Ok(GenerateCriteriaResult { criteria })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CritiqueCriteriaRequest {
    pub criterion_type: String, // "inclusion" or "exclusion"
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CritiqueCriteriaResult {
    pub critique: String,
}

#[tauri::command]
pub async fn critique_criteria(
    db_state: State<'_, DbState>,
    request: CritiqueCriteriaRequest,
) -> Result<CritiqueCriteriaResult, AppError> {
    let criterion_type = request.criterion_type;
    if criterion_type != "inclusion" && criterion_type != "exclusion" {
        return Err(AppError::Validation(
            "criterion_type must be 'inclusion' or 'exclusion'".to_string(),
        ));
    }

    let (config, aims, existing_criteria) = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let aims = criteria_repo::get_all_aims(&conn)?;
        if aims.is_empty() {
            return Err(AppError::Validation(
                "Research aims must be defined before critiquing criteria".to_string(),
            ));
        }
        let criteria = criteria_repo::get_criteria_by_type(&conn, &criterion_type)?;
        if criteria.is_empty() {
            return Err(AppError::Validation(format!(
                "No {} criteria defined to critique",
                criterion_type
            )));
        }
        (config, aims, criteria)
    };

    let aims_list: Vec<String> = aims
        .iter()
        .enumerate()
        .map(|(i, a)| format!("{}. {}", i + 1, a.text))
        .collect();

    let criteria_list: Vec<String> = existing_criteria
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. [{}] {}", i + 1, c.priority.as_str(), c.text))
        .collect();

    let type_label = if criterion_type == "inclusion" {
        "Inclusion"
    } else {
        "Exclusion"
    };

    let system_prompt = "You are a systematic literature review assistant. Critically evaluate the appropriateness \
        of screening criteria for a systematic literature review. Provide specific, actionable feedback.";

    let user_prompt = format!(
        r#"## Task
Evaluate the following {type_label} Criteria for a systematic literature review.
Assess their appropriateness, completeness, clarity, and methodological rigor.
Provide specific, actionable suggestions for improvement.

## Research Aims
{research_aims}

## Current {type_label} Criteria
{criteria}

Provide your critique as plain text with specific recommendations. Include:
1. Overall assessment of the criteria quality
2. Any gaps or missing criteria that should be considered
3. Suggestions for improving clarity or specificity
4. Priority adjustments if warranted
Do not return JSON."#,
        type_label = type_label,
        research_aims = aims_list.join("\n"),
        criteria = criteria_list.join("\n"),
    );

    let (response, _) = client::send_chat_completion(&config, system_prompt, &user_prompt).await?;

    Ok(CritiqueCriteriaResult { critique: response })
}
