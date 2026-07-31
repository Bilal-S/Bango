use serde::{Deserialize, Serialize};
use tauri::State;

use std::sync::Arc;

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::audit_repo;
use crate::db::connection::{lock_conn, DbState};
use crate::db::criteria_repo;
use crate::db::label_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::label::Label;
use crate::models::tag_label::MergeResult;
use rusqlite::params;

/// Standard workflow labels that classify articles by review process state,
/// quality assessment, and decision category. These complement the corpus-
/// derived labels and should be suggested by the LLM (up to 4) when they
/// match the review's screening workflow.
///
/// All entries are lowercase, hyphenated, and ≤ 35 chars so they pass the
/// backend sanitization in `screening::engine::sanitize_tag_or_label_name`.
const STANDARD_WORKFLOW_LABELS: &[&str] = &[
    "priority-read",
    "strong-methodology",
    "weak-methodology",
    "needs-full-text",
    "disputed",
    "key-paper",
    "borderline",
    "duplicate-suspect",
    "excluded-by-criteria",
    "included-by-criteria",
    "needs-discussion",
    "flagged",
];

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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    label_repo::get_all_labels(&conn)
}

#[tauri::command]
pub fn get_labels_with_counts(
    db_state: State<'_, DbState>,
) -> Result<Vec<LabelWithCount>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    label_repo::rename_label(&conn, &request.id, &request.new_name)
}

#[tauri::command]
pub fn delete_label(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    label_repo::delete_label(&conn, &id)?;
    // Label changes are part of article metadata used by bibliometrics + the
    // wiki. Every other tag/label mutation path sets both staleness flags; the
    // standalone deletes must too so deleting a label does not silently desync
    // the derived biblio tables and the wiki pre-seed.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    label_repo::update_label_color(&conn, &request.id, request.color.as_deref())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestLabelsResult {
    pub labels: Vec<Label>,
}

#[tauri::command]
pub async fn suggest_labels(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
) -> Result<SuggestLabelsResult, AppError> {
    let (config, research_aims, inclusion_criteria, exclusion_criteria) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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

    let standard_labels_str = STANDARD_WORKFLOW_LABELS.join(", ");

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

## Standard Workflow Labels
The following standard workflow labels classify articles by review process state, quality assessment, and decision category.
Include up to 4 of these when they are relevant to the review workflow (in addition to the criteria-derived labels):
[{standard_labels}]

## Response Format
Return JSON exactly matching this schema:
{{
  "labels": ["label-name-1", "label-name-2", ...]
}}

Rules:
- Generate 5-15 labels total (including any standard labels you select).
- Each label must be a short, lowercase, hyphenated string (e.g., "priority-read", "strong-methodology", "needs-full-text").
- Each label must be at most 35 characters. Do NOT prefix labels with "inclusion:" or "exclusion:".
- Labels should be oriented around the research aims and screening criteria - reflecting the types of decisions and
  categorizations a reviewer would need when screening articles against these specific criteria, plus any relevant standard workflow labels.
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
        standard_labels = standard_labels_str,
    );

    let system_prompt = "You are a systematic literature review assistant. Generate a set of workflow labels for tracking the screening process based on research aims and screening criteria.";
    let result = orchestrator
        .send(&config, system_prompt, &user_prompt, LlmRequestType::LabelGeneration)
        .await;
    if let Err(ref e) = result {
        let err_msg = e.to_string();
        audit_repo::log_error_best_effort(
            &db_state.conn,
            &format!("Label suggestion failed: {}", err_msg),
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
        AppError::Import(format!("Failed to parse label suggestion response: {}", e))
    })?;
    let label_names: Vec<String> = parsed["labels"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let labels = label_repo::create_labels_batch(&conn, &label_names, "ai_generated")?;

    Ok(SuggestLabelsResult { labels })
}

// ── Merge ("Replace with...") ──────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeLabelRequest {
    /// Label being deleted (its articles are reassigned to `into_id`).
    pub from_id: String,
    /// Surviving label.
    pub into_id: String,
}

/// Replace one label with another: all articles carrying `from_id` are
/// reassigned to `into_id`, then `from_id` is deleted. Destructive and
/// irreversible; the frontend shows a confirmation dialog before invoking.
///
/// Mirrors `commands::tags::merge_tag`. Returns the shared `MergeResult` so
/// the success toast can report the accurate `reassigned` + `already-had-
/// survivor` split.
#[tauri::command]
pub fn merge_label(
    db_state: State<'_, DbState>,
    request: MergeLabelRequest,
) -> Result<MergeResult, AppError> {
    let conn = lock_conn(&db_state.conn)?;
    merge_label_inner(&conn, &request.from_id, &request.into_id)
}

/// Core merge logic, extracted so it is testable without `State<DbState>`.
/// Mirrors `commands::tags::merge_tag_inner` (see that function's doc-comment
/// for the full contract).
pub fn merge_label_inner(
    conn: &rusqlite::Connection,
    from_id: &str,
    into_id: &str,
) -> Result<MergeResult, AppError> {
    if from_id == into_id {
        return Err(AppError::Validation("Cannot replace a label with itself".to_string()));
    }

    // Load names (also serves as the existence check).
    let from_name: String = conn
        .query_row("SELECT name FROM labels WHERE id = ?1", params![from_id], |row| row.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Label {from_id} not found"))
            }
            other => AppError::Database(other),
        })?;
    let into_name: String = conn
        .query_row("SELECT name FROM labels WHERE id = ?1", params![into_id], |row| row.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Label {into_id} not found"))
            }
            other => AppError::Database(other),
        })?;

    // Compute the overlap count BEFORE mutating (the `UPDATE OR IGNORE` in
    // `merge_labels` would otherwise erase the signal). `reassigned_count` is
    // derived from the actual reassigned-ID list below, so it stays accurate
    // even if rows change between the count and the mutate.
    let overlap: i64 = conn.query_row(
        "SELECT COUNT(*) FROM article_labels \
         WHERE label_id = ?1 AND article_id IN (SELECT article_id FROM article_labels WHERE label_id = ?2)",
        params![from_id, into_id],
        |row| row.get(0),
    )?;

    // Capture the reassigned article IDs (from-label minus overlap).
    let reassigned: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT article_id FROM article_labels WHERE label_id = ?1 \
             AND article_id NOT IN (SELECT article_id FROM article_labels WHERE label_id = ?2)",
        )?;
        let rows = stmt.query_map(params![from_id, into_id], |row| row.get::<_, String>(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let tx = conn.unchecked_transaction()?;

    label_repo::merge_labels(&tx, from_id, into_id)?;

    let detail = format!("Replaced label \"{from_name}\" -> \"{into_name}\" (merge)");
    audit_repo::write_tag_label_audit(&tx, &reassigned, "label_remove", &detail)?;

    for id in &reassigned {
        article_repo::bump_changed_at(&tx, id)?;
    }

    app_settings_repo::mark_biblio_needs_refresh(&tx);
    app_settings_repo::mark_wiki_needs_refresh(&tx);

    tx.commit()?;

    Ok(MergeResult {
        from_name,
        into_name,
        reassigned_count: reassigned.len(),
        already_had_survivor_count: overlap as usize,
    })
}
