use std::sync::Arc;

use tauri::{Emitter, Manager, State};

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::biblio_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::gap_analysis_repo;
use crate::db::llm_config_repo;
use crate::db::summary_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::prisma::data;
use crate::summary::engine::{self, GapAnalysisInput, SummaryInput};
use crate::summary::gap_analysis::BiblioContext;
use crate::summary::prompt::{
    build_figure_description_prompt, build_section_context, build_synthesis_prompt,
    ensure_schema_version_v2, filter_high_value_sections, merge_figure_descriptions_into_blob,
    merge_summary_into_blob, merge_unified_blob, parse_figure_descriptions_response,
    ArticleSummary, FigureDescription, ScreeningData, TableDescription,
    ARTICLE_SUMMARY_SYSTEM_PROMPT, ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT,
    FIGURE_DESCRIPTION_SYSTEM_PROMPT,
};
use crate::summary::prompt::{parse_markdown_summary, ARTICLE_SUMMARY_MARKDOWN_FALLBACK_PROMPT};
use crate::utils::sections::{classify_sections, detect_markdown_tables, extract_captions};

#[tauri::command]
pub async fn generate_summary(
    db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    citation_style: Option<String>,
) -> Result<String, AppError> {
    let style = citation_style.unwrap_or_else(|| "APA".to_string());

    // Extract all DB data synchronously while holding the lock
    let summary_input = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;

        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let aim_list = criteria_repo::get_all_aims(&conn)?;
        let aim_texts: Vec<String> = aim_list.iter().map(|a| a.text.clone()).collect();
        let included = article_repo::get_articles_by_status(&conn, "included")?;

        // Shape 0: fetch full criteria definitions so the Methodology narrative
        // can name the actual eligibility rules (not just aggregate exclusion
        // counts). Split by type; each list is threaded into the prompt.
        let all_criteria = criteria_repo::get_all_criteria(&conn)?;
        let inclusion_criteria: Vec<String> = all_criteria
            .iter()
            .filter(|c| {
                matches!(c.criterion_type, crate::models::criterion::CriterionType::Inclusion)
            })
            .map(|c| c.text.clone())
            .collect();
        let exclusion_criteria: Vec<String> = all_criteria
            .iter()
            .filter(|c| {
                matches!(c.criterion_type, crate::models::criterion::CriterionType::Exclusion)
            })
            .map(|c| c.text.clone())
            .collect();

        // Shape A: opt-in evidence enrichment. When `summary_evidence_mode` is
        // `with_summary_facts`, distill each included article's
        // `full_text_ai_summary` blob into a compact evidence string carrying
        // structured study facts (study design, sample size, effect sizes).
        // Default `abstract_only` preserves today's behavior and cost exactly.
        let evidence_mode = app_settings_repo::get_setting(&conn, "summary_evidence_mode")?
            .unwrap_or_else(|| "abstract_only".to_string());
        let use_evidence = evidence_mode == "with_summary_facts";

        let articles: Vec<ArticleSummary> = included
            .iter()
            .map(|a| {
                // Combine RIS-imported keywords and user/AI-added tags into one deduplicated CSV list
                let mut combined: Vec<String> = a.keywords.clone();
                for tag in &a.tags {
                    if !combined.iter().any(|k| k.eq_ignore_ascii_case(tag)) {
                        combined.push(tag.clone());
                    }
                }
                // Shape A: distill the per-article AI-summary blob into evidence.
                // `format_ai_summary_as_evidence` returns None for missing/malformed/
                // empty blobs, so abstract-only articles stay unaffected.
                let evidence = if use_evidence {
                    crate::summary::prompt::format_ai_summary_as_evidence(
                        a.full_text_ai_summary.as_deref(),
                    )
                } else {
                    None
                };
                ArticleSummary {
                    title: a.title.clone(),
                    authors: a.authors.clone(),
                    year: a.publication_year,
                    abstract_text: a.abstract_text.clone(),
                    keywords: combined,
                    evidence,
                }
            })
            .collect();

        // PRISMA / screening statistics
        let prisma = data::compute_prisma_data(&conn)?;

        // AI-screened: articles that have an ai_decision set
        let ai_screened: usize = conn
            .query_row("SELECT COUNT(*) FROM articles WHERE ai_decision IS NOT NULL", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        // Manual review: articles where manual_override = 1
        let manual_reviewed: usize = conn
            .query_row("SELECT COUNT(*) FROM articles WHERE manual_override = 1", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        let screening_data = ScreeningData {
            records_identified: prisma.records_identified,
            duplicates_removed: prisma.duplicates_removed,
            records_screened: prisma.records_screened,
            records_excluded: prisma.records_excluded,
            records_excluded_with_reasons: prisma.records_excluded_with_reasons,
            records_assessed: prisma.records_assessed,
            records_in_progress: prisma.records_in_progress,
            studies_included: prisma.studies_included,
            ai_screened,
            manual_reviewed,
            exclusion_reasons: prisma
                .exclusion_reasons
                .iter()
                .map(|r| (r.criterion_text.clone(), r.count))
                .collect(),
        };

        SummaryInput::new(
            config,
            aim_texts,
            articles,
            screening_data,
            style.clone(),
            inclusion_criteria,
            exclusion_criteria,
        )
    }; // conn lock released here

    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>();
    let result = engine::generate_summary(&orchestrator, summary_input).await?;

    // Save to DB after successful generation
    let generated_at = chrono::Utc::now().to_rfc3339();
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        summary_repo::save_summary(&conn, &result, &style, &generated_at)?;
    }

    Ok(result)
}

#[tauri::command]
pub fn get_saved_summary(
    db_state: State<'_, DbState>,
) -> Result<Option<summary_repo::SavedSummary>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    summary_repo::get_summary(&conn)
}

/// Generate an AI summary for a single article from its full text.
/// Calls the LLM, parses the JSON response, stores it in the database,
/// and emits a Tauri event. Records `ai_summary` audit entry on success;
/// logs to diagnostic audit + emits error event on failure.
#[tauri::command]
pub async fn generate_article_ai_summary(
    db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    article_id: String,
    include_section_summaries: Option<bool>,
) -> Result<String, AppError> {
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>();
    let want_sections = include_section_summaries.unwrap_or(false);
    generate_article_ai_summary_inner(
        &db_state,
        &app_handle,
        &orchestrator,
        &article_id,
        want_sections,
    )
    .await
}

/// Reusable summary-generation core (no Tauri state). Extracted so the
/// batch-import runner can call it per-article without re-implementing the
/// LLM call + parse + store pipeline. Emits the same
/// `article-ai-summary-complete` / `-error` events.
pub async fn generate_article_ai_summary_inner(
    db_state: &State<'_, DbState>,
    app_handle: &tauri::AppHandle,
    orchestrator: &Arc<LlmOrchestrator>,
    article_id: &str,
    include_section_summaries: bool,
) -> Result<String, AppError> {
    // 1. Fetch article full text and LLM config while holding the DB lock
    let (title, full_text, config) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let (t, ft) = article_repo::get_full_text_for_summary(&conn, article_id)?;
        let cfg = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        (t, ft, cfg)
    }; // conn lock released

    /* Build user prompt with article title and full text.
    When `include_section_summaries` is true AND the full text has detectable
    high-value sections (Methods/Results/Discussion), we swap to the
    section-aware system prompt and append a delimited section block so the
    model returns `section_summaries` alongside the standard fields. When
    false OR no sections are detected, behavior is identical to the
    pre-T1.3 path (backward compatible). */
    let want_sections = include_section_summaries;
    let high_value = if want_sections {
        filter_high_value_sections(&classify_sections(&full_text))
    } else {
        Vec::new()
    };
    let (system_prompt, user_prompt, used_section_path) = if high_value.is_empty() {
        // Standard path (whole-paper only).
        let max_chars = ((config.context_window_tokens as usize).saturating_sub(2000)) * 4;
        let truncated =
            if full_text.len() > max_chars { &full_text[..max_chars] } else { &full_text };
        let prompt = format!("## Article Title\n{title}\n\n## Full Text\n{truncated}");
        (ARTICLE_SUMMARY_SYSTEM_PROMPT, prompt, false)
    } else {
        // Section-aware path. Reserve space for the section-context block, then
        // append it after the full text so the model can ground each per-section
        // summary in the corresponding delimited region.
        let section_context = build_section_context(&high_value);
        let section_overhead = section_context.len() + 200;
        let max_chars = ((config.context_window_tokens as usize)
            .saturating_sub(section_overhead / 4 + 2000))
            * 4;
        let truncated =
            if full_text.len() > max_chars { &full_text[..max_chars] } else { &full_text };
        let prompt = format!(
            "## Article Title\n{title}\n\n## Full Text\n{truncated}\n\n## Detected Sections\n\n{section_context}"
        );
        (ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT, prompt, true)
    };

    // 3. Call LLM via orchestrator - catch errors to log them to audit trail.
    // The section-aware path is categorized as `SectionSummary` for diagnostics
    // so per-section requests are distinguishable from monolithic ones.
    let request_type = if used_section_path {
        LlmRequestType::SectionSummary
    } else {
        LlmRequestType::ArticleSummary
    };
    let llm_result =
        orchestrator.send_json(&config, system_prompt, &user_prompt, request_type).await;

    let (response_text, _tokens) = match llm_result {
        Ok(v) => v,
        Err(e) => {
            // Log error to general diagnostic audit (best-effort; the real
            // error is returned below).
            let err_msg = e.to_string();
            crate::db::audit_repo::log_error_best_effort(
                &db_state.conn,
                &format!("AI summary failed for article {article_id} ({title}): {err_msg}"),
            );
            // Emit error event so frontend can react
            let _ = app_handle.emit(
                "article-ai-summary-error",
                serde_json::json!({ "articleId": article_id, "error": err_msg }),
            );
            return Err(e);
        }
    };

    /* Validate JSON - use `strip_code_fences`, NOT `screening_engine::extract_json`.
    The screening helper assumes a top-level JSON array and unwraps the
    first nested array-of-objects out of a JSON object - which corrupts a
    valid summary object (whose `section_summaries` is an array-of-objects)
    into just that array, breaking all top-level field access downstream.
    `send_json` already ran strip_code_fences + escape_control_chars_in_json. */
    let mut parsed: serde_json::Value = match serde_json::from_str(&response_text) {
        Ok(v) => v,
        Err(e) => {
            let err_msg = format!("Invalid JSON response from LLM: {e}");
            crate::db::audit_repo::log_error_best_effort(
                &db_state.conn,
                &format!("AI summary failed for article {article_id} ({title}): {err_msg}"),
            );
            let _ = app_handle.emit(
                "article-ai-summary-error",
                serde_json::json!({ "articleId": article_id, "error": err_msg }),
            );
            return Err(AppError::Import(err_msg));
        }
    };

    /* Per T1.3 contract: the backend MUST guarantee `schema_version: 2` when the
    section-aware path runs, regardless of whether the model emitted the
    field. This keeps frontend `parseAiSummary` gating reliable
    (schema_version >= 2 -> enriched view; absent/1 -> legacy view). */
    if used_section_path {
        ensure_schema_version_v2(&mut parsed);
    }

    /* Tier 1 fallback (T4 E2E 2026-07-01): if the model returned an empty or
    near-empty JSON object (a known failure mode for reasoning models that
    consume their output budget on thinking tokens), retry once with the
    simpler markdown fallback prompt. The markdown response is parsed by
    `parse_markdown_summary` into the same JSON blob shape. */
    let has_substantive_content = parsed
        .get("summary_150_250_words")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        || parsed.get("field").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
    if !has_substantive_content {
        // Retry with the markdown fallback prompt.
        let max_chars = ((config.context_window_tokens as usize).saturating_sub(2000)) * 4;
        let truncated =
            if full_text.len() > max_chars { &full_text[..max_chars] } else { &full_text };
        let fallback_user_prompt =
            format!("## Article Title\n{title}\n\n## Full Text\n{truncated}");
        let fallback_result = orchestrator
            .send(
                &config,
                ARTICLE_SUMMARY_MARKDOWN_FALLBACK_PROMPT,
                &fallback_user_prompt,
                LlmRequestType::ArticleSummary,
            )
            .await;
        if let Ok((fallback_text, _)) = fallback_result {
            // Parse the markdown response into a JSON blob.
            let markdown_blob = parse_markdown_summary(&fallback_text);
            if let Ok(md_value) = serde_json::from_str::<serde_json::Value>(&markdown_blob) {
                // Only use the fallback if it produced substantive content.
                let md_has_content = md_value
                    .get("summary_150_250_words")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                    || md_value
                        .get("field")
                        .and_then(|v| v.as_str())
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                if md_has_content {
                    parsed = md_value;
                    if used_section_path {
                        ensure_schema_version_v2(&mut parsed);
                    }
                }
            }
        }
        /* If the fallback also failed, `parsed` stays as the original empty blob.
        The user sees "No AI summary" but the command doesn't crash. */
    }

    /* Phase 0 footgun fix: merge the freshly-generated summary into the existing
    blob so `figures`/`tables` (from `generate_figure_descriptions`) survive a
    summary regen. The previous direct `set_ai_summary` overwrote the entire
    column, wiping any figures/tables. The merge helper preserves all existing
    keys the summary path does not produce. */
    let summary_json = parsed.to_string();

    /* Store in database. The block returns `preserved_json` (the merged blob)
    so the command can return it to the frontend with figures/tables intact. */
    let preserved_json = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        // Read the existing blob so the merge can preserve `figures`/`tables`.
        let existing_blob: Option<String> = conn
            .query_row(
                "SELECT full_text_ai_summary FROM articles WHERE id = ?1",
                rusqlite::params![&article_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        let merged =
            merge_summary_into_blob(existing_blob.as_deref(), &summary_json, used_section_path);
        article_repo::set_ai_summary(&conn, article_id, &merged)?;
        crate::db::audit_repo::create_entry(
            &conn,
            article_id,
            "ai_summary",
            None,
            None,
            Some("AI summary generated from full text"),
            "ai",
        )?;
        // The AI summary is the 2nd-priority content source for the wiki
        // (full_text → ai_summary → abstract). Regenerating it changes the
        // synthesis page the next ingest produces.
        app_settings_repo::mark_wiki_needs_refresh(&conn);
        app_settings_repo::mark_biblio_needs_refresh(&conn);
        merged
    };

    // 6. Emit success event
    let _ = app_handle.emit(
        "article-ai-summary-complete",
        serde_json::json!({ "articleId": article_id, "title": title }),
    );

    // 7. Fire-and-forget embedding regeneration for this article. The summary
    //    may have enriched the title/abstract signal; the director's
    //    `input_hash` staleness check ensures only genuinely-changed rows
    //    trigger a new embedding call. This hook is non-blocking: the summary
    //    result is returned immediately, and the embedding runs on a detached
    //    task that respects the orchestrator's concurrency + rate limits.
    //    Embeddings are best-effort here: a failure is logged inside the
    //    runner and never surfaces to the summary caller.
    {
        let handle = app_handle.clone();
        let article_id_owned = article_id.to_string();
        tokio::task::spawn(async move {
            let db = handle.state::<crate::db::connection::DbState>();
            let orch = handle.state::<std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>();
            // Wrap the orchestrator into the v2 HttpEmbeddingBatchSender.
            let sender: std::sync::Arc<dyn crate::embedding::runner::EmbeddingBatchSender> =
                std::sync::Arc::new(crate::embedding::runner::HttpEmbeddingBatchSender::new(
                    std::sync::Arc::clone(&orch),
                ));
            let scope = crate::embedding::director::EmbeddingScope {
                article_ids: Some(vec![article_id_owned]),
                status_filter: None,
                force: false,
            };
            let _ = crate::embedding::runner::generate_embeddings_inner(
                &db,
                sender,
                scope,
                Some(&handle),
                false,
                None,
            )
            .await;
        });
    }

    // Return the merged blob so the frontend gets the preserved figures/tables.
    Ok(preserved_json)
}

/// Generate LLM descriptions for figure/table captions extracted from an
/// article's full text (Tier 2 Phase 4). One batched orchestrator call per
/// article, grounded in the caption text (no visual hallucination). The
/// descriptions are merged into the existing `full_text_ai_summary` blob
/// under `figures`/`tables` keys and stamped `schema_version: 2`.
///
/// Emits `article-figure-descriptions-complete` on success and
/// `article-figure-descriptions-error` on failure.
#[tauri::command]
pub async fn generate_figure_descriptions(
    db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    article_id: String,
) -> Result<String, AppError> {
    // 1. Fetch the article's full text + existing AI-summary blob + config.
    let (title, full_text, existing_blob, config) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let (t, ft) = article_repo::get_full_text_for_summary(&conn, &article_id)?;
        // Fetch the existing AI-summary blob directly (no dedicated repo helper
        // exists for this read; the column is `full_text_ai_summary`).
        let existing: Option<String> = conn
            .query_row(
                "SELECT full_text_ai_summary FROM articles WHERE id = ?1",
                rusqlite::params![&article_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        let cfg = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        (t, ft, existing, cfg)
    }; // conn lock released

    // 2. Extract figure/table captions from the full text.
    let captions = extract_captions(&full_text);
    if captions.is_empty() {
        let err_msg = "No figure/table captions detected in the full text.";
        let _ = app_handle.emit(
            "article-figure-descriptions-error",
            serde_json::json!({ "articleId": article_id, "error": err_msg }),
        );
        return Err(AppError::Validation(err_msg.to_string()));
    }

    // 3. Build the batched prompt (one call for all captions).
    let user_prompt = build_figure_description_prompt(&title, &captions);

    // 4. Call the orchestrator with the grounded caption-parser prompt.
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>();
    let llm_result = orchestrator
        .send_json(
            &config,
            FIGURE_DESCRIPTION_SYSTEM_PROMPT,
            &user_prompt,
            LlmRequestType::FigureDescription,
        )
        .await;

    let (response_text, _tokens) = match llm_result {
        Ok(v) => v,
        Err(e) => {
            let err_msg = e.to_string();
            crate::db::audit_repo::log_error_best_effort(
                &db_state.conn,
                &format!(
                    "Figure descriptions failed for article {article_id} ({title}): {err_msg}"
                ),
            );
            let _ = app_handle.emit(
                "article-figure-descriptions-error",
                serde_json::json!({ "articleId": article_id, "error": err_msg }),
            );
            return Err(e);
        }
    };

    // 5. Parse the LLM response into FigureDescription entries.
    let descriptions = match parse_figure_descriptions_response(&response_text) {
        Ok(d) => d,
        Err(e) => {
            let err_msg = e.to_string();
            crate::db::audit_repo::log_error_best_effort(
                &db_state.conn,
                &format!("Figure descriptions parse failed for article {article_id}: {err_msg}"),
            );
            let _ = app_handle.emit(
                "article-figure-descriptions-error",
                serde_json::json!({ "articleId": article_id, "error": err_msg }),
            );
            return Err(e);
        }
    };

    // 6. Attach the verbatim caption text to each description (the LLM only
    //    returns `number` + `description`; we pair them back to the extracted
    //    caption text so the UI can show both). Split by CaptionKind.
    let mut figures: Vec<FigureDescription> = Vec::new();
    let mut tables: Vec<FigureDescription> = Vec::new();
    for desc in descriptions {
        // Find the matching extracted caption by number.
        let matching_caption = captions.iter().find(|c| c.number == desc.number);
        let caption_text = matching_caption.map(|c| c.caption.clone()).unwrap_or_default();
        let is_table = matching_caption.map(|c| c.kind.label() == "Table").unwrap_or(false);
        let enriched = FigureDescription {
            number: desc.number,
            caption: caption_text,
            description: desc.description,
        };
        if is_table {
            tables.push(enriched);
        } else {
            figures.push(enriched);
        }
    }

    // 7. Merge into the existing blob (preserves section_summaries, stamps v2).
    let merged_json =
        merge_figure_descriptions_into_blob(existing_blob.as_deref(), figures, tables);

    // 8. Store + audit + refresh flag.
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        article_repo::set_ai_summary(&conn, &article_id, &merged_json)?;
        crate::db::audit_repo::create_entry(
            &conn,
            &article_id,
            "figure_descriptions",
            None,
            None,
            Some("Figure/table descriptions generated from captions"),
            "ai",
        )?;
        // Figures/tables feed the wiki synthesis pre-seed; flag a refresh.
        app_settings_repo::mark_wiki_needs_refresh(&conn);
    }

    // 9. Emit success.
    let _ = app_handle.emit(
        "article-figure-descriptions-complete",
        serde_json::json!({ "articleId": article_id, "title": title }),
    );

    Ok(merged_json)
}

/// Tier 4.2: Generate a unified AI summary for an article in a single merge
/// write. Pipeline: section-aware summary (T1.3) per high-value section →
/// figure/table descriptions from captions (T2.1) → synthesis call to upgrade
/// `summary_150_250_words` → single `merge_unified_blob` write (no intermediate
/// state with missing keys). Falls back to monolithic
/// `ARTICLE_SUMMARY_SYSTEM_PROMPT` (T4.4) when no sections are detectable.
///
/// Emits `article-ai-summary-complete` / `-error` (same events as the legacy
/// command).
#[tauri::command]
pub async fn generate_unified_summary(
    db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    article_id: String,
) -> Result<String, AppError> {
    // 1. Fetch article full text + existing blob + config.
    let (title, full_text, existing_blob, config) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let (t, ft) = article_repo::get_full_text_for_summary(&conn, &article_id)?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT full_text_ai_summary FROM articles WHERE id = ?1",
                rusqlite::params![&article_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        let cfg = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        (t, ft, existing, cfg)
    }; // conn lock released

    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>();

    // 2. Path selection: sections detectable -> unified; else monolithic fallback.
    let high_value = filter_high_value_sections(&classify_sections(&full_text));
    if high_value.is_empty() {
        // Monolithic fallback (T4.4): single call, no section/figure/synthesis
        // steps. Delegates to the legacy prompt path.
        let max_chars = ((config.context_window_tokens as usize).saturating_sub(2000)) * 4;
        let truncated =
            if full_text.len() > max_chars { &full_text[..max_chars] } else { &full_text };
        let user_prompt = format!("## Article Title\n{title}\n\n## Full Text\n{truncated}");
        let llm_result = orchestrator
            .send_json(
                &config,
                ARTICLE_SUMMARY_SYSTEM_PROMPT,
                &user_prompt,
                LlmRequestType::ArticleSummary,
            )
            .await;
        let (response_text, _tokens) = match llm_result {
            Ok(v) => v,
            Err(e) => {
                let err_msg = e.to_string();
                crate::db::audit_repo::log_error_best_effort(
                    &db_state.conn,
                    &format!(
                        "Unified summary (monolithic fallback) failed for article {article_id} ({title}): {err_msg}"
                    ),
                );
                let _ = app_handle.emit(
                    "article-ai-summary-error",
                    serde_json::json!({ "articleId": article_id, "error": err_msg }),
                );
                return Err(e);
            }
        };
        /* Parse + store via Phase 0 preserve-on-write guard so existing
        figures/tables survive the monolithic regen. Use `strip_code_fences`
        (NOT `screening_engine::extract_json`) - the screening helper corrupts
        object-shaped summary responses. `send_json` already ran
        strip_code_fences + escape_control_chars_in_json. */
        let parsed: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| AppError::Import(format!("Invalid JSON response from LLM: {e}")))?;
        let fresh_json = parsed.to_string();
        let merged = merge_summary_into_blob(existing_blob.as_deref(), &fresh_json, false);
        {
            let conn = crate::db::connection::lock_conn(&db_state.conn)?;
            article_repo::set_ai_summary(&conn, &article_id, &merged)?;
            crate::db::audit_repo::create_entry(
                &conn,
                &article_id,
                "ai_summary",
                None,
                None,
                Some("Unified AI summary (monolithic fallback - no sections detected)"),
                "ai",
            )?;
            app_settings_repo::mark_wiki_needs_refresh(&conn);
            app_settings_repo::mark_biblio_needs_refresh(&conn);
        }
        let _ = app_handle.emit(
            "article-ai-summary-complete",
            serde_json::json!({ "articleId": article_id, "title": title }),
        );
        return Ok(merged);
    }

    /* Unified path: section calls (T1.3) + figure/table descriptions (T2.1) +
    synthesis call (T4.2). Each is a separate orchestrator round-trip.
    The section-aware path uses the same system prompt + delimited-section
    block as `generate_article_ai_summary(include_section_summaries=true)`. */
    let section_context = build_section_context(&high_value);
    let section_overhead = section_context.len() + 200;
    let max_chars =
        ((config.context_window_tokens as usize).saturating_sub(section_overhead / 4 + 2000)) * 4;
    let truncated = if full_text.len() > max_chars { &full_text[..max_chars] } else { &full_text };
    let section_user_prompt = format!(
        "## Article Title\n{title}\n\n## Full Text\n{truncated}\n\n## Detected Sections\n\n{section_context}"
    );
    let section_response = orchestrator
        .send_json(
            &config,
            ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT,
            &section_user_prompt,
            LlmRequestType::SectionSummary,
        )
        .await;
    let (section_text, _section_tokens) = match section_response {
        Ok(v) => v,
        Err(e) => {
            let err_msg = e.to_string();
            crate::db::audit_repo::log_error_best_effort(
                &db_state.conn,
                &format!("Unified summary section call failed for article {article_id}: {err_msg}"),
            );
            let _ = app_handle.emit(
                "article-ai-summary-error",
                serde_json::json!({ "articleId": article_id, "error": err_msg }),
            );
            return Err(e);
        }
    };
    /* Parse the section-aware response; extract `section_summaries` array and
    `field` for the synthesis prompt. Use `strip_code_fences`
    (NOT `screening_engine::extract_json`) - the screening helper corrupts
    object-shaped summary responses. `send_json` already ran
    strip_code_fences + escape_control_chars_in_json. */
    let mut section_value: serde_json::Value = serde_json::from_str(&section_text)
        .map_err(|e| AppError::Import(format!("Invalid section response JSON: {e}")))?;
    ensure_schema_version_v2(&mut section_value);
    let field =
        section_value.get("field").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let section_summaries =
        section_value.get("section_summaries").cloned().unwrap_or(serde_json::Value::Array(vec![]));
    let section_summaries_json = section_summaries.to_string();

    /* Figure/table descriptions (T2.1 Phase 4) + GFM table markdown (T2.2 via
    `detect_markdown_tables`). Skipped when no captions.
    GFM rows extracted by `detect_markdown_tables` are correlated to their
    corresponding table captions by number, so the frontend can render the
    table natively (`TableDescription.markdown`). Tables without a matching
    caption are still emitted (numbered by detection order as a fallback). */
    let captions = extract_captions(&full_text);
    // Detect GFM tables once; the (text_without_tables, table_sections) pair is
    // used below to populate `TableDescription.markdown` by detection order.
    let (_text_without_tables, table_sections) = detect_markdown_tables(&full_text);
    let (figures, tables): (Vec<FigureDescription>, Vec<TableDescription>) = if captions.is_empty()
    {
        (Vec::new(), Vec::new())
    } else {
        let fig_prompt = build_figure_description_prompt(&title, &captions);
        let fig_response = orchestrator
            .send_json(
                &config,
                FIGURE_DESCRIPTION_SYSTEM_PROMPT,
                &fig_prompt,
                LlmRequestType::FigureDescription,
            )
            .await;
        match fig_response {
            Ok((fig_text, _fig_tokens)) => match parse_figure_descriptions_response(&fig_text) {
                Ok(descriptions) => {
                    let mut figs: Vec<FigureDescription> = Vec::new();
                    let mut tabs: Vec<TableDescription> = Vec::new();
                    // Counter for unmatched GFM table sections (fallback
                    // numbering when a table has no caption).
                    let mut unmatched_table_idx = 0usize;
                    for desc in descriptions {
                        let matching = captions.iter().find(|c| c.number == desc.number);
                        let caption_text = matching.map(|c| c.caption.clone()).unwrap_or_default();
                        let is_table = matching.map(|c| c.kind.label() == "Table").unwrap_or(false);
                        if is_table {
                            // Correlate the GFM table markdown by table number
                            // (1-based detection order in `table_sections`).
                            // Try the numeric portion of the caption number first
                            // (e.g. "2a" -> 2), then fall back to the unmatched
                            // counter so every detected table gets a chance.
                            let table_index_from_number = desc
                                .number
                                .chars()
                                .take_while(|c| c.is_ascii_digit())
                                .collect::<String>()
                                .parse::<usize>()
                                .ok()
                                .filter(|&n| n >= 1 && n <= table_sections.len())
                                .unwrap_or_else(|| {
                                    unmatched_table_idx += 1;
                                    unmatched_table_idx.min(table_sections.len())
                                });
                            let markdown = table_sections
                                .get(table_index_from_number.saturating_sub(1))
                                .map(|s| s.body.clone())
                                .unwrap_or_default();
                            tabs.push(TableDescription {
                                number: desc.number,
                                caption: caption_text,
                                markdown,
                                description: desc.description,
                            });
                        } else {
                            figs.push(FigureDescription {
                                number: desc.number,
                                caption: caption_text,
                                description: desc.description,
                            });
                        }
                    }
                    (figs, tabs)
                }
                Err(_) => (Vec::new(), Vec::new()),
            },
            Err(_) => (Vec::new(), Vec::new()),
        }
    };

    // 5. Synthesis call: produce the upgraded digest from the section summaries.
    let synthesis_prompt = build_synthesis_prompt(&title, &field, &section_summaries_json);
    let synthesis_response = orchestrator
        .send_json(
            &config,
            ARTICLE_SUMMARY_SYSTEM_PROMPT,
            &synthesis_prompt,
            LlmRequestType::UnifiedSummary,
        )
        .await;
    let synthesis_digest_json = match synthesis_response {
        Ok((syn_text, _syn_tokens)) => {
            // `send_json` already ran strip_code_fences + escape_control_chars_in_json.
            serde_json::from_str::<serde_json::Value>(&syn_text)
                .map(|v| v.to_string())
                .unwrap_or_else(|_| {
                    // Malformed synthesis: fall back to the section call's digest.
                    serde_json::json!({
                        "summary_150_250_words": section_value
                            .get("summary_150_250_words")
                            .cloned()
                            .unwrap_or(serde_json::Value::String(String::new())),
                        "key_insights": section_value
                            .get("key_insights")
                            .cloned()
                            .unwrap_or(serde_json::Value::Array(vec![])),
                        "keywords": section_value
                            .get("keywords")
                            .cloned()
                            .unwrap_or(serde_json::Value::Array(vec![])),
                    })
                    .to_string()
                })
        }
        Err(_) => {
            // Synthesis call failed: fall back to the section call's digest.
            serde_json::json!({
                "summary_150_250_words": section_value
                    .get("summary_150_250_words")
                    .cloned()
                    .unwrap_or(serde_json::Value::String(String::new())),
                "key_insights": section_value
                    .get("key_insights")
                    .cloned()
                    .unwrap_or(serde_json::Value::Array(vec![])),
                "keywords": section_value
                    .get("keywords")
                    .cloned()
                    .unwrap_or(serde_json::Value::Array(vec![])),
            })
            .to_string()
        }
    };

    // 6. Single merge write: compose all parts into one blob.
    let unified_json = merge_unified_blob(
        existing_blob.as_deref(),
        &section_summaries_json,
        figures,
        tables,
        &synthesis_digest_json,
    );
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        article_repo::set_ai_summary(&conn, &article_id, &unified_json)?;
        crate::db::audit_repo::create_entry(
            &conn,
            &article_id,
            "ai_summary",
            None,
            None,
            Some("Unified AI summary generated (section + figure + synthesis pipeline)"),
            "ai",
        )?;
        app_settings_repo::mark_wiki_needs_refresh(&conn);
        app_settings_repo::mark_biblio_needs_refresh(&conn);
    }

    let _ = app_handle.emit(
        "article-ai-summary-complete",
        serde_json::json!({ "articleId": article_id, "title": title }),
    );

    Ok(unified_json)
}

// ── Research Gap Analysis ──────────────────────────────────────────────────

/// The maximum number of top journals / terms / countries rendered into the
/// `BiblioContext` block. Keeps the prompt bounded without truncating the
/// article list (which is the real signal).
const GAP_TOP_N: i32 = 10;

/// Generate a Research Gap Analysis report over the included corpus.
///
/// Mirrors `generate_summary`'s lock/release shape: short critical section for
/// reads, release before the LLM call, re-lock for the write. The engine fn
/// handles batching + synthesis when the corpus exceeds 80% of the context
/// window.
///
/// The report is persisted in the single-row `gap_analysis` table and returned
/// as Markdown. On success no audit row is written; on error logged via
/// `log_error_best_effort` (no CHECK migration needed).
#[tauri::command]
pub async fn analyze_research_gaps(
    db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    citation_style: Option<String>,
) -> Result<String, AppError> {
    let style = citation_style.unwrap_or_else(|| "APA".to_string());

    // 1. Read all DB data synchronously while holding the lock.
    let gap_input = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;

        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let aim_list = criteria_repo::get_all_aims(&conn)?;
        let aim_texts: Vec<String> = aim_list.iter().map(|a| a.text.clone()).collect();
        let included = article_repo::get_articles_by_status(&conn, "included")?;

        // Split criteria by type (same as `generate_summary`).
        let all_criteria = criteria_repo::get_all_criteria(&conn)?;
        let inclusion_criteria: Vec<String> = all_criteria
            .iter()
            .filter(|c| {
                matches!(c.criterion_type, crate::models::criterion::CriterionType::Inclusion)
            })
            .map(|c| c.text.clone())
            .collect();
        let exclusion_criteria: Vec<String> = all_criteria
            .iter()
            .filter(|c| {
                matches!(c.criterion_type, crate::models::criterion::CriterionType::Exclusion)
            })
            .map(|c| c.text.clone())
            .collect();

        // Shape-A evidence enrichment (same setting as the literature review).
        let evidence_mode = app_settings_repo::get_setting(&conn, "summary_evidence_mode")?
            .unwrap_or_else(|| "abstract_only".to_string());
        let use_evidence = evidence_mode == "with_summary_facts";

        let articles: Vec<ArticleSummary> = included
            .iter()
            .map(|a| {
                let mut combined: Vec<String> = a.keywords.clone();
                for tag in &a.tags {
                    if !combined.iter().any(|k| k.eq_ignore_ascii_case(tag)) {
                        combined.push(tag.clone());
                    }
                }
                let evidence = if use_evidence {
                    crate::summary::prompt::format_ai_summary_as_evidence(
                        a.full_text_ai_summary.as_deref(),
                    )
                } else {
                    None
                };
                ArticleSummary {
                    title: a.title.clone(),
                    authors: a.authors.clone(),
                    year: a.publication_year,
                    abstract_text: a.abstract_text.clone(),
                    keywords: combined,
                    evidence,
                }
            })
            .collect();

        // PRISMA + screening counts (same as `generate_summary`).
        let prisma = data::compute_prisma_data(&conn)?;
        let ai_screened: usize = conn
            .query_row("SELECT COUNT(*) FROM articles WHERE ai_decision IS NOT NULL", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);
        let manual_reviewed: usize = conn
            .query_row("SELECT COUNT(*) FROM articles WHERE manual_override = 1", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);
        let screening_data = ScreeningData {
            records_identified: prisma.records_identified,
            duplicates_removed: prisma.duplicates_removed,
            records_screened: prisma.records_screened,
            records_excluded: prisma.records_excluded,
            records_excluded_with_reasons: prisma.records_excluded_with_reasons,
            records_assessed: prisma.records_assessed,
            records_in_progress: prisma.records_in_progress,
            studies_included: prisma.studies_included,
            ai_screened,
            manual_reviewed,
            exclusion_reasons: prisma
                .exclusion_reasons
                .iter()
                .map(|r| (r.criterion_text.clone(), r.count))
                .collect(),
        };

        let biblio_context = build_biblio_context(&conn);

        GapAnalysisInput::new(
            config,
            aim_texts,
            articles,
            screening_data,
            style.clone(),
            inclusion_criteria,
            exclusion_criteria,
            biblio_context,
        )
    }; // conn lock released

    // 2. Call the engine (handles batching + synthesis).
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>();
    let result = engine::generate_gap_analysis(&orchestrator, gap_input).await?;

    // 3. Persist + flag.
    let generated_at = chrono::Utc::now().to_rfc3339();
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        gap_analysis_repo::save_gap_analysis(&conn, &result, &style, &generated_at)?;
        // The report cites biblio-derived facts; flag a refresh so the next
        // normalize run keeps the biblio tables consistent if the corpus changed.
        app_settings_repo::mark_biblio_needs_refresh(&conn);
    }

    Ok(result)
}

/// Return the persisted gap-analysis report (if any), mirroring
/// `get_saved_summary`.
#[tauri::command]
pub fn get_saved_gap_analysis(
    db_state: State<'_, DbState>,
) -> Result<Option<gap_analysis_repo::SavedGapAnalysis>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    gap_analysis_repo::get_gap_analysis(&conn)
}

/// Build the `BiblioContext` for the gap-analysis prompt from the existing
/// biblio tables. Degrades gracefully when biblio has not been normalized yet
/// (all fields empty/default) so the gap report still runs on a fresh project.
fn build_biblio_context(conn: &rusqlite::Connection) -> BiblioContext {
    // Year range + pubs_by_year + journal distribution from KPIs.
    let (year_range, pubs_by_year, top_journals) = match biblio_repo::get_biblio_kpis(conn) {
        Ok(kpis) => {
            let year_range = kpis.year_from.zip(kpis.year_to);
            let pubs_by_year =
                kpis.pubs_by_year.into_iter().map(|yc| (yc.year, yc.count)).collect::<Vec<_>>();
            // Aggregate journal_distribution (per journal, per year) into
            // top journals by total article count.
            let mut journal_counts: std::collections::HashMap<String, i32> =
                std::collections::HashMap::new();
            for jyd in &kpis.journal_distribution {
                if jyd.journal.is_empty() {
                    continue;
                }
                *journal_counts.entry(jyd.journal.clone()).or_insert(0) += jyd.count;
            }
            let mut top_journals: Vec<(String, i32)> = journal_counts.into_iter().collect();
            top_journals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            top_journals.truncate(GAP_TOP_N as usize);
            (year_range, pubs_by_year, top_journals)
        }
        Err(_) => (None, Vec::new(), Vec::new()),
    };

    // Top terms by article_count (already ordered DESC by `get_all_terms`).
    let top_terms = biblio_repo::get_all_terms(conn)
        .unwrap_or_default()
        .into_iter()
        .take(GAP_TOP_N as usize)
        .map(|t| (t.normalized_term, t.article_count))
        .collect::<Vec<_>>();

    // Geographic distribution: group biblio_institutions.country by the number
    // of distinct included articles linked (via biblio_author_affiliations).
    // Direct SQL is cleaner here than threading through the model layer.
    let geographic_distribution = conn
        .prepare(
            "SELECT bi.country, COUNT(DISTINCT baa.article_id) AS cnt \
             FROM biblio_institutions bi \
             JOIN biblio_author_affiliations baa ON baa.institution_id = bi.id \
             JOIN articles a ON a.id = baa.article_id \
             WHERE a.status = 'included' AND bi.country IS NOT NULL AND bi.country != '' \
             GROUP BY bi.country \
             ORDER BY cnt DESC \
             LIMIT ?1",
        )
        .and_then(|mut stmt| {
            let rows = stmt.query_map(rusqlite::params![GAP_TOP_N], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
            })?;
            Ok(rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    BiblioContext { year_range, pubs_by_year, top_journals, top_terms, geographic_distribution }
}
