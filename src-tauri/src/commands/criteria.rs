use serde::{Deserialize, Serialize};
use tauri::State;

use std::sync::Arc;

use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::criterion::{Criterion, ResearchAim};

#[tauri::command]
pub fn get_research_aims(db_state: State<'_, DbState>) -> Result<Vec<ResearchAim>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    criteria_repo::create_aim(&conn, &request.text)
}

#[tauri::command]
pub fn delete_research_aim(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    criteria_repo::delete_aim(&conn, &id)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAimRequest {
    pub id: String,
    pub text: String,
}

#[tauri::command]
pub fn update_research_aim(
    db_state: State<'_, DbState>,
    request: UpdateAimRequest,
) -> Result<ResearchAim, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    criteria_repo::update_aim(&conn, &request.id, &request.text)
}

#[tauri::command]
pub fn get_criteria(db_state: State<'_, DbState>) -> Result<Vec<Criterion>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    criteria_repo::update_criterion(&conn, &request.id, &request.text, &request.priority)
}

#[tauri::command]
pub fn delete_criterion(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    request: GenerateCriteriaRequest,
) -> Result<GenerateCriteriaResult, AppError> {
    let criterion_type = request.criterion_type;
    if criterion_type != "inclusion" && criterion_type != "exclusion" {
        return Err(AppError::Validation(
            "criterion_type must be 'inclusion' or 'exclusion'".to_string(),
        ));
    }

    let (config, aims) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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

    let aims_list: Vec<String> =
        aims.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a.text)).collect();

    let type_label = if criterion_type == "inclusion" { "inclusion" } else { "exclusion" };

    let system_prompt = "You are a systematic literature review assistant. Based on the research aims provided, \
        suggest appropriate criteria for screening research papers in a systematic review. \
        Write criterion text concisely and directly - do not prefix with phrases like \
        'Include studies that…' or 'Exclude studies that…'. The criterion type is already known from context. \
        Do not use any EmDash characters your response. Example: write 'Randomized controlled trials only' not 'Include studies that use randomized controlled trials'.";

    let user_prompt = format!(
        r#"## Task
First, determine the field of study based on the research aims below.
Then suggest up to 8 {type_label} criteria that would be appropriate for this type of research.
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
- Do not duplicate or overlap concepts.
- Do not use EmDash chracters in your response.
- Write criterion text concisely. Do NOT start with "Include studies that…" or "Exclude studies that…" - state the essential condition directly."#,
        type_label = type_label,
        research_aims = aims_list.join("\n"),
    );

    let result = orchestrator
        .send(&config, system_prompt, &user_prompt, LlmRequestType::CriteriaGeneration)
        .await;
    if let Err(ref e) = result {
        let err_msg = e.to_string();
        audit_repo::log_error_best_effort(
            &db_state.conn,
            &format!("Criteria generation failed: {}", err_msg),
        );
    }
    let (response, _) = result?;

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
                    let priority = v["priority"].as_str().unwrap_or("standard").to_string();
                    Some((text, priority))
                })
                .collect()
        })
        .unwrap_or_default();

    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

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
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    request: CritiqueCriteriaRequest,
) -> Result<CritiqueCriteriaResult, AppError> {
    let criterion_type = request.criterion_type;
    if criterion_type != "inclusion" && criterion_type != "exclusion" {
        return Err(AppError::Validation(
            "criterion_type must be 'inclusion' or 'exclusion'".to_string(),
        ));
    }

    let (config, aims, existing_criteria) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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

    let aims_list: Vec<String> =
        aims.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a.text)).collect();

    let criteria_list: Vec<String> = existing_criteria
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. [{}] {}", i + 1, c.priority.as_str(), c.text))
        .collect();

    let (type_label, opposite_label) = if criterion_type == "inclusion" {
        ("Inclusion", "exclusion")
    } else {
        ("Exclusion", "inclusion")
    };

    let system_prompt = "You are a systematic literature review assistant. Critically evaluate the appropriateness \
        of screening criteria for a systematic literature review. Provide specific, actionable feedback. \
        Only evaluate the specific criteria type provided. Never suggest adding criteria of the opposite type.";

    let user_prompt = format!(
        r#"## Task
Evaluate the following {type_label} Criteria for a systematic literature review.
Assess their appropriateness, completeness, clarity, and methodological rigor.
Provide specific, actionable suggestions for improvement.

IMPORTANT: Focus exclusively on {type_label} criteria. Do NOT suggest adding {opposite_label} criteria - those are managed separately.

## Research Aims
{research_aims}

## Current {type_label} Criteria
{criteria}

Provide your critique as plain text with specific recommendations. Include:
1. Overall assessment of the criteria quality
2. Any gaps or missing {type_label} criteria that should be considered
3. Suggestions for improving clarity or specificity
4. Priority adjustments if warranted
Do not return JSON."#,
        type_label = type_label,
        opposite_label = opposite_label,
        research_aims = aims_list.join("\n"),
        criteria = criteria_list.join("\n"),
    );

    let result = orchestrator
        .send(&config, system_prompt, &user_prompt, LlmRequestType::CriteriaGeneration)
        .await;
    if let Err(ref e) = result {
        let err_msg = e.to_string();
        audit_repo::log_error_best_effort(
            &db_state.conn,
            &format!("Criteria critique failed: {}", err_msg),
        );
    }
    let (response, _) = result?;

    Ok(CritiqueCriteriaResult { critique: response })
}
