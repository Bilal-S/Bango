use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::db::tag_repo;
use crate::error::AppError;
use crate::llm::client;
use crate::models::tag::Tag;

#[tauri::command]
pub fn get_tags(db_state: State<'_, DbState>) -> Result<Vec<Tag>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    tag_repo::get_all_tags(&conn)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTagRequest {
    pub name: String,
}

#[tauri::command]
pub fn create_tag(
    db_state: State<'_, DbState>,
    request: CreateTagRequest,
) -> Result<Tag, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    tag_repo::create_tag(&conn, &request.name, "user_created")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameTagRequest {
    pub id: String,
    pub new_name: String,
}

#[tauri::command]
pub fn rename_tag(
    db_state: State<'_, DbState>,
    request: RenameTagRequest,
) -> Result<Tag, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    tag_repo::rename_tag(&conn, &request.id, &request.new_name)
}

#[tauri::command]
pub fn delete_tag(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    tag_repo::delete_tag(&conn, &id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestTagsResult {
    pub tags: Vec<Tag>,
}

#[tauri::command]
pub async fn suggest_tags(db_state: State<'_, DbState>) -> Result<SuggestTagsResult, AppError> {
    let (config, keywords, inclusion_criteria, exclusion_criteria) = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let articles = article_repo::get_articles_by_status(&conn, "working")?;
        let keywords: Vec<String> = articles
            .iter()
            .flat_map(|a| a.keywords.iter().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let inc = criteria_repo::get_criteria_by_type(&conn, "inclusion")?;
        let exc = criteria_repo::get_criteria_by_type(&conn, "exclusion")?;
        (config, keywords, inc, exc)
    };

    let keywords_str = keywords.join(", ");
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
Generate a concise set of content-category tags for organizing articles in a systematic literature review. Tags should represent meaningful topic, methodology, or relevance categories.

## Article Keywords
{keywords}

## Inclusion Criteria
{inclusion}

## Exclusion Criteria
{exclusion}

## Response Format
Return JSON exactly matching this schema:
{{
  "tags": ["tag-name-1", "tag-name-2", ...]
}}

Rules:
- Generate 10-30 tags.
- Each tag should be a short, lowercase, hyphenated string (e.g., "machine-learning", "clinical-trial").
- Tags should be derived from the keywords and criteria provided.
- Do not duplicate or overlap concepts."#,
        keywords = keywords_str,
        inclusion = inc_list.join("\n"),
        exclusion = exc_list.join("\n"),
    );

    let system_prompt = "You are a systematic literature review assistant. Generate a set of content-category tags for organizing articles in a literature review.";
    let response = client::send_chat_completion(&config, system_prompt, &user_prompt).await?;

    // Parse response
    let json_str = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| AppError::Import(format!("Failed to parse tag suggestion response: {}", e)))?;
    let tag_names: Vec<String> = parsed["tags"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let tags = tag_repo::create_tags_batch(&conn, &tag_names, "ai_suggested")?;

    Ok(SuggestTagsResult { tags })
}
