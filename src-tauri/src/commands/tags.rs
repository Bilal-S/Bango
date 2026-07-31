use serde::{Deserialize, Serialize};
use tauri::State;

use std::collections::HashMap;
use std::sync::Arc;

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::audit_repo;
use crate::db::connection::{lock_conn, DbState};
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::db::tag_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::criterion::CriterionType;
use crate::models::tag::Tag;
use crate::models::tag_label::MergeResult;
use rusqlite::params;

/// Maximum number of top-cited articles to send with full context (title + abstract).
const TOP_CITED_FULL_COUNT: usize = 5;
/// Maximum number of next-most-cited articles to send as titles only.
const NEXT_CITED_TITLES_COUNT: usize = 15;
/// Minimum frequency for a keyword to be included (must appear in 2+ articles).
const MIN_KEYWORD_FREQUENCY: usize = 2;
/// Maximum number of keywords to send to the LLM.
const MAX_KEYWORDS: usize = 200;

/// Standard field-of-study tags that classify content by research methodology
/// and study type. These are commonly used across academic disciplines and
/// should be suggested by the LLM (up to 4) when they match the corpus.
///
/// All entries are lowercase, hyphenated, and ≤ 35 chars so they pass the
/// backend sanitization in `screening::engine::sanitize_tag_or_label_name`.
const STANDARD_STUDY_TAGS: &[&str] = &[
    "systematic-review",
    "meta-analysis",
    "randomized-controlled-trial",
    "cohort-study",
    "case-control-study",
    "cross-sectional-study",
    "qualitative-study",
    "mixed-methods",
    "pilot-study",
    "protocol",
    "scoping-review",
    "umbrella-review",
    "narrative-review",
    "experimental-study",
    "observational-study",
    "longitudinal-study",
    "prevalence-study",
    "cost-effectiveness",
    "validation-study",
    "editorial",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagWithCount {
    pub id: String,
    pub name: String,
    pub source: String,
    pub color: Option<String>,
    pub article_count: usize,
}

#[tauri::command]
pub fn get_tags(db_state: State<'_, DbState>) -> Result<Vec<Tag>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    tag_repo::get_all_tags(&conn)
}

#[tauri::command]
pub fn get_tags_with_counts(db_state: State<'_, DbState>) -> Result<Vec<TagWithCount>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let tags = tag_repo::get_all_tags(&conn)?;
    let result: Vec<TagWithCount> = tags
        .into_iter()
        .map(|tag| {
            let count = tag_repo::get_article_count_for_tag(&conn, &tag.id).unwrap_or(0);
            TagWithCount {
                id: tag.id,
                name: tag.name,
                source: match tag.source {
                    crate::models::tag::TagSource::AiSuggested => "ai_suggested".to_string(),
                    crate::models::tag::TagSource::RisKeyword => "ris_keyword".to_string(),
                    crate::models::tag::TagSource::UserCreated => "user_created".to_string(),
                },
                color: tag.color,
                article_count: count,
            }
        })
        .collect();
    Ok(result)
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    tag_repo::rename_tag(&conn, &request.id, &request.new_name)
}

#[tauri::command]
pub fn delete_tag(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    tag_repo::delete_tag(&conn, &id)?;
    // Tag changes feed the keyword co-occurrence network + the wiki concept
    // hubs. Every other tag/label mutation path sets both staleness flags; the
    // standalone deletes must too so deleting a tag does not silently desync
    // the derived biblio tables and the wiki pre-seed.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTagColorRequest {
    pub id: String,
    pub color: Option<String>,
}

#[tauri::command]
pub fn update_tag_color(
    db_state: State<'_, DbState>,
    request: UpdateTagColorRequest,
) -> Result<Tag, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    tag_repo::update_tag_color(&conn, &request.id, request.color.as_deref())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestTagsResult {
    pub tags: Vec<Tag>,
}

#[tauri::command]
pub async fn suggest_tags(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
) -> Result<SuggestTagsResult, AppError> {
    // ── Data gathering (under DB lock) ──────────────────────────────
    let (config, top_cited_full, next_cited_titles, keywords_str, criteria_text) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;

        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;

        let articles = article_repo::get_articles_by_status(&conn, "working")?;

        // --- Tiered article selection by citation count ---
        // Collect articles that have num_cited > 0, sort descending by citations.
        let mut cited: Vec<_> = articles.iter().filter(|a| a.num_cited.unwrap_or(0) > 0).collect();
        cited.sort_by_key(|b| std::cmp::Reverse(b.num_cited.unwrap_or(0)));

        let top_cited_full: Vec<(String, String)> = cited
            .iter()
            .take(TOP_CITED_FULL_COUNT)
            .map(|a| (a.title.clone(), a.abstract_text.clone()))
            .collect();

        let top_ids: Vec<&str> = top_cited_full.iter().map(|(t, _)| t.as_str()).collect();

        let next_cited_titles: Vec<String> = cited
            .iter()
            .skip(TOP_CITED_FULL_COUNT)
            .take(NEXT_CITED_TITLES_COUNT)
            .filter(|a| !top_ids.contains(&a.title.as_str()))
            .map(|a| a.title.clone())
            .collect();

        // --- Frequency-filtered keywords ---
        let mut keyword_freq: HashMap<String, usize> = HashMap::new();
        for a in &articles {
            for kw in &a.keywords {
                *keyword_freq.entry(kw.to_lowercase()).or_insert(0) += 1;
            }
        }
        let mut freq_keywords: Vec<(String, usize)> =
            keyword_freq.into_iter().filter(|(_, count)| *count >= MIN_KEYWORD_FREQUENCY).collect();
        freq_keywords.sort_by_key(|b| std::cmp::Reverse(b.1));
        let keywords: Vec<String> =
            freq_keywords.into_iter().take(MAX_KEYWORDS).map(|(kw, _)| kw).collect();
        let keywords_str = keywords.join(", ");

        // --- Criteria ---
        let inclusion = criteria_repo::get_criteria_by_type(&conn, "inclusion")
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| {
                if matches!(c.criterion_type, CriterionType::Inclusion) {
                    Some(c.text)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let exclusion = criteria_repo::get_criteria_by_type(&conn, "exclusion")
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| {
                if matches!(c.criterion_type, CriterionType::Exclusion) {
                    Some(c.text)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut criteria_text = String::new();
        if !inclusion.is_empty() {
            criteria_text.push_str("## Inclusion Criteria\n");
            for (i, c) in inclusion.iter().enumerate() {
                criteria_text.push_str(&format!("{}. {}\n", i + 1, c));
            }
        }
        if !exclusion.is_empty() {
            if !criteria_text.is_empty() {
                criteria_text.push('\n');
            }
            criteria_text.push_str("## Exclusion Criteria\n");
            for (i, c) in exclusion.iter().enumerate() {
                criteria_text.push_str(&format!("{}. {}\n", i + 1, c));
            }
        }

        (config, top_cited_full, next_cited_titles, keywords_str, criteria_text)
    };

    // ── Prompt construction ─────────────────────────────────────────
    let mut article_section = String::new();

    if !top_cited_full.is_empty() {
        article_section.push_str("## Most-Cited Articles (full context)\n");
        for (i, (title, abstract_text)) in top_cited_full.iter().enumerate() {
            article_section.push_str(&format!(
                "{}. Title: {}\n   Abstract: {}\n",
                i + 1,
                title,
                abstract_text
            ));
        }
    }

    if !next_cited_titles.is_empty() {
        if !article_section.is_empty() {
            article_section.push('\n');
        }
        article_section.push_str("## Additional Highly-Cited Articles (titles only)\n");
        for title in &next_cited_titles {
            article_section.push_str(&format!("- {}\n", title));
        }
    }

    let criteria_section =
        if criteria_text.is_empty() { String::new() } else { format!("\n{}", criteria_text) };

    let standard_tags_str = STANDARD_STUDY_TAGS.join(", ");

    let user_prompt = format!(
        r#"## Task
Generate a concise set of content-category tags for organizing articles in a systematic literature review.
Tags should represent meaningful topic, methodology, or relevance categories derived from the article data.

{article_section}
## Article Keywords (frequency-ranked, from all working articles)
{keywords}
 {criteria_section}
## Standard Study-Type Tags
The following standard field-of-study tags classify research methodology and study type.
Include up to 4 of these when they are relevant to the corpus (in addition to the corpus-derived tags):
[{standard_tags}]

## Response Format
Return JSON exactly matching this schema:
{{
  "tags": ["tag-name-1", "tag-name-2", ...]
}}

Rules:
- Generate 10-30 tags total (including any standard tags you select).
- Each tag must be a short, lowercase, hyphenated string (e.g., "machine-learning", "clinical-trial").
- Each tag must be at most 35 characters. Do NOT prefix tags with "inclusion:" or "exclusion:".
- Tags should be derived from the articles, keywords, and review criteria shown above, plus any relevant standard study-type tags.
- Do not duplicate or overlap concepts."#,
        article_section = article_section,
        keywords = keywords_str,
        criteria_section = criteria_section,
        standard_tags = standard_tags_str,
    );

    // ── LLM call ────────────────────────────────────────────────────
    let system_prompt = "You are a systematic literature review assistant. Generate a set of \
         content-category tags for organizing articles in a literature review based on the most \
         cited articles, article keywords, and review criteria.";
    let result = orchestrator
        .send(&config, system_prompt, &user_prompt, LlmRequestType::TagGeneration)
        .await;
    if let Err(ref e) = result {
        let err_msg = e.to_string();
        audit_repo::log_error_best_effort(
            &db_state.conn,
            &format!("Tag suggestion failed: {}", err_msg),
        );
    }
    let (response, _) = result?;

    // ── Parse response ──────────────────────────────────────────────
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

    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let tags = tag_repo::create_tags_batch(&conn, &tag_names, "ai_suggested")?;

    Ok(SuggestTagsResult { tags })
}

// ── Merge ("Replace with...") ──────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeTagRequest {
    /// Tag being deleted (its articles are reassigned to `into_id`).
    pub from_id: String,
    /// Surviving tag.
    pub into_id: String,
}

/// Replace one tag with another: all articles carrying `from_id` are
/// reassigned to `into_id`, then `from_id` is deleted. Destructive and
/// irreversible; the frontend shows a confirmation dialog before invoking.
///
/// Returns the per-merge counts so the success toast can report the accurate
/// `reassigned` + `already-had-survivor` split (the pre-confirm dialog can
/// only show an honest upper bound).
#[tauri::command]
pub fn merge_tag(
    db_state: State<'_, DbState>,
    request: MergeTagRequest,
) -> Result<MergeResult, AppError> {
    let conn = lock_conn(&db_state.conn)?;
    merge_tag_inner(&conn, &request.from_id, &request.into_id)
}

/// Core merge logic, extracted so it is testable without `State<DbState>`.
///
/// Implementation contract (all inside one `unchecked_transaction`):
/// 1. Guard `from_id == into_id`.
/// 2. Load `from`/`into` names.
/// 3. Compute true counts BEFORE mutating (the `UPDATE OR IGNORE` in `merge_tags`
///    would otherwise erase the overlap signal).
/// 4. `tag_repo::merge_tags` (repo: `UPDATE OR IGNORE` + `DELETE FROM tags`).
///    `PRAGMA foreign_keys=ON` + `ON DELETE CASCADE` removes the leftover overlap
///    junction rows when the source tag row is deleted; no row is left dangling.
/// 5. One coalesced `tag_remove` audit entry per *reassigned* article via the
///    shared `audit_repo::write_tag_label_audit` helper (single-entry bulk
///    pattern; the detail string carries both halves of the replace).
/// 6. `bump_changed_at` for each reassigned article.
/// 7. Set both staleness flags.
/// 8. Commit + return `MergeResult`.
pub fn merge_tag_inner(
    conn: &rusqlite::Connection,
    from_id: &str,
    into_id: &str,
) -> Result<MergeResult, AppError> {
    if from_id == into_id {
        return Err(AppError::Validation("Cannot replace a tag with itself".to_string()));
    }

    // Load names (also serves as the existence check).
    let from_name: String = conn
        .query_row("SELECT name FROM tags WHERE id = ?1", params![from_id], |row| row.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Tag {from_id} not found"))
            }
            other => AppError::Database(other),
        })?;
    let into_name: String = conn
        .query_row("SELECT name FROM tags WHERE id = ?1", params![into_id], |row| row.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Tag {into_id} not found"))
            }
            other => AppError::Database(other),
        })?;

    // Compute the overlap count BEFORE mutating (the `UPDATE OR IGNORE` in
    // `merge_tags` would otherwise erase the signal). `reassigned_count` is
    // derived from the actual reassigned-ID list below, so it stays accurate
    // even if rows change between the count and the mutate.
    let overlap: i64 = conn.query_row(
        "SELECT COUNT(*) FROM article_tags \
         WHERE tag_id = ?1 AND article_id IN (SELECT article_id FROM article_tags WHERE tag_id = ?2)",
        params![from_id, into_id],
        |row| row.get(0),
    )?;

    // Capture the reassigned article IDs (from-tag minus overlap).
    let reassigned: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT article_id FROM article_tags WHERE tag_id = ?1 \
             AND article_id NOT IN (SELECT article_id FROM article_tags WHERE tag_id = ?2)",
        )?;
        let rows = stmt.query_map(params![from_id, into_id], |row| row.get::<_, String>(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let tx = conn.unchecked_transaction()?;

    tag_repo::merge_tags(&tx, from_id, into_id)?;

    // One coalesced `tag_remove` audit entry per reassigned article. The detail
    // string carries both halves of the replace so the Audit Timeline reads
    // correctly (overlap-only articles were silently de-linked by the cascade
    // and are intentionally NOT audited as tag_remove because no link moved).
    let detail = format!("Replaced tag \"{from_name}\" -> \"{into_name}\" (merge)");
    audit_repo::write_tag_label_audit(&tx, &reassigned, "tag_remove", &detail)?;

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
