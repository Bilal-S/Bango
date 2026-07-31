//! Tier 3 two-stage: re-screen borderline articles with full-text evidence.
//!
//! `run_stage2_borderline` is extracted from `run_sync` to keep the main loop
//! focused. The stage-2 decision overrides stage 1 and passes through
//! `resolve_article_decision` again; stage-2 writes `ai_screen_enhanced`
//! audit entries.
//!
//! **Cancel-polling contract**: the `tokio::select!` LLM-call wrapper stays
//! inline inside `run_stage2_borderline`. Always poll `notified()`, check the
//! token inside the branch body - never use an `if` precondition on the select
//! branch (an unregistered waiter makes `notify_waiters()` a no-op and silently
//! loses the cancel signal).

use std::time::Duration;

use rusqlite::Connection;
use tokio::time::sleep;

use crate::db::audit_repo;
use crate::error::AppError;
use crate::llm::orchestrator::LlmRequestType;
use crate::screening::article_writer::{update_article_after_screening, ScreeningUpdate};
use crate::screening::decision::resolve_article_decision;
use crate::screening::engine::prompt_parts::Stage2Context;
use crate::screening::engine::{log_diag, LlmScreeningResponse, ScreeningEngine};
use crate::screening::json_parse::process_screening_responses;
use crate::screening::prompt::{self, ArticleEntry};
use crate::screening::tags_labels::{create_or_match_label, create_or_match_tag};

impl ScreeningEngine {
    /// Re-screen borderline articles with full-text evidence.
    ///
    /// Returns `Ok(true)` when cancelled (caller MUST `return Ok(())` so no
    /// further batches process); `Ok(false)` on normal completion. The two
    /// cancel-exit sites (mid-LLM-call + inter-article delay) set final
    /// progress here before returning `true`.
    pub(crate) async fn run_stage2_borderline(
        &self,
        conn_mutex: &std::sync::Mutex<Connection>,
        llm: &dyn crate::screening::llm_client::LlmClient,
        batch: &[crate::models::article::Article],
        screenings: &[LlmScreeningResponse],
        ctx: &Stage2Context<'_>,
    ) -> Result<bool, AppError> {
        let borderline: Vec<(&crate::models::article::Article, &LlmScreeningResponse)> = batch
            .iter()
            .zip(screenings.iter())
            .filter(|(a, s)| {
                is_borderline(
                    a.has_full_text,
                    s,
                    ctx.config.two_stage_low,
                    ctx.config.two_stage_high,
                )
            })
            .collect();

        if borderline.is_empty() {
            return Ok(false);
        }

        // Stage-2 progress sub-line.
        let borderline_len = borderline.len();
        self.update_progress(ctx.app_handle, |p| {
            p.stage_total = Some(borderline_len);
            p.stage = Some(format!("Stage 2: 0/{borderline_len} borderline (full text)"));
        })
        .await;

        for (stage2_done, (article, _stage1)) in borderline.iter().enumerate() {
            // Cancel/pause gate between stage-2 articles.
            if *self.cancel_token.lock().await {
                break;
            }
            while *self.pause_token.lock().await {
                sleep(Duration::from_millis(200)).await;
                if *self.cancel_token.lock().await {
                    break;
                }
            }

            // Retrieve evidence for this borderline article.
            let evidence = {
                let c = crate::db::connection::lock_conn(conn_mutex)?;
                crate::screening::evidence::retrieve_evidence_for_article(
                    &c,
                    &article.id,
                    &ctx.prompt_parts.inclusion_texts,
                    &ctx.prompt_parts.exclusion_texts,
                    ctx.config,
                )
            };

            // No evidence survived ranking → skip stage 2 (stage-1 decision stands).
            let evidence = match evidence {
                Some(ev) => ev,
                None => {
                    self.advance_stage2_subline(ctx.app_handle, stage2_done + 1, borderline.len())
                        .await;
                    continue;
                }
            };

            // Build single-article stage-2 prompt.
            let entry = ArticleEntry {
                title: article.title.clone(),
                authors: article.authors.join("; "),
                year: article.publication_year,
                abstract_text: article.abstract_text.clone(),
                full_text_evidence: Some(evidence.text),
            };
            let prompt_input = ctx.prompt_parts.build_prompt_input(vec![entry]);
            let user_prompt = prompt::build_screening_prompt(&prompt_input);
            let system_prompt = prompt::SYSTEM_PROMPT;

            // Stage-2 LLM call wrapped in tokio::select! against cancel_notify.
            let stage2_response = {
                let cancel_notify = self.cancel_notify.clone();
                loop {
                    tokio::select! {
                        biased;
                        () = cancel_notify.notified() => {
                            if *self.cancel_token.lock().await {
                                log_diag!("stage2_llm_call: cancel detected, dropping response + returning");
                                let mut progress = self.progress.lock().await;
                                progress.is_running = false;
                                progress.current_article_titles = vec![];
                                self.emit_progress(ctx.app_handle, &progress);
                                return Ok(true);
                            }
                            continue;
                        }
                        res = llm.send_with_type(
                            system_prompt,
                            &user_prompt,
                            LlmRequestType::EnhancedScreening,
                        ) => break res,
                    }
                }
            };
            // Cancellable stage-2 delay; on cancel drop response (stage-1 stands).
            if self.delay_or_cancel(ctx.app_handle, ctx.request_delay_ms).await {
                return Ok(true);
            }

            let (response_text, total_tokens) = match stage2_response {
                Ok(data) => data,
                Err(e) => {
                    // Non-fatal: log + keep stage-1 decision.
                    {
                        let c = crate::db::connection::lock_conn(conn_mutex)?;
                        let _ = audit_repo::log_error(
                            &c,
                            &format!("Stage-2 screening failed for {}: {}", article.id, e),
                        );
                    }
                    self.advance_stage2_subline(ctx.app_handle, stage2_done + 1, borderline.len())
                        .await;
                    continue;
                }
            };

            // Parse single-article stage-2 response.
            match process_screening_responses(&response_text) {
                Ok(mut stage2_screenings) if stage2_screenings.len() == 1 => {
                    let stage2 = stage2_screenings.swap_remove(0);
                    if stage2.decision == "error" {
                        self.advance_stage2_subline(
                            ctx.app_handle,
                            stage2_done + 1,
                            borderline.len(),
                        )
                        .await;
                        continue;
                    }

                    // Resolve stage-2 decision; override evidence-sections label.
                    let mut decision = resolve_article_decision(
                        &stage2,
                        &article.id,
                        ctx.criteria,
                        ctx.inclusion_criteria,
                        ctx.global_numbering,
                        ctx.has_custom_logic,
                        ctx.enhanced_evidence_labels,
                    );
                    decision.evidence_sections = Some(evidence.sections_label.clone());
                    let final_decision = decision.final_decision.as_str();
                    let augmented_inc = decision.augmented_inc.as_slice();
                    let augmented_exc = decision.augmented_exc.as_slice();
                    let reasoning = decision.reasoning.as_str();

                    // Write stage-2 decision; capture stage1 status for tally fix-up.
                    let stage1_was_include = {
                        let c = crate::db::connection::lock_conn(conn_mutex)?;
                        let stage1_status: Option<String> = c
                            .query_row(
                                "SELECT status FROM articles WHERE id = ?1",
                                rusqlite::params![&article.id],
                                |row| row.get(0),
                            )
                            .ok();
                        let stage1_was_include = stage1_status.as_deref() == Some("included");

                        update_article_after_screening(
                            &c,
                            ScreeningUpdate {
                                article_id: &article.id,
                                decision: final_decision,
                                reasoning,
                                confidence: stage2.confidence,
                                matched_inc: augmented_inc,
                                matched_exc: augmented_exc,
                                actual_tokens: Some(total_tokens),
                                evidence_sections: Some(&evidence.sections_label),
                            },
                        )?;

                        for tag_name in &stage2.suggested_tags {
                            let _ = create_or_match_tag(&c, tag_name, &article.id);
                        }
                        for (prefix, text) in &decision.auto_label_criteria {
                            let label_name = format!("{}: {}", prefix, text);
                            let _ = create_or_match_label(&c, &label_name, &article.id);
                        }
                        stage1_was_include
                    };

                    // Fix up progress include/exclude tallies if decision flipped.
                    let now_include = final_decision == "include";
                    let done = stage2_done + 1;
                    let total = borderline.len();
                    self.update_progress(ctx.app_handle, |p| {
                        if now_include != stage1_was_include {
                            if now_include {
                                p.included += 1;
                                p.rejected = p.rejected.saturating_sub(1);
                            } else {
                                p.rejected += 1;
                                p.included = p.included.saturating_sub(1);
                            }
                        }
                        p.stage = Some(format!("Stage 2: {done}/{total} borderline (full text)"));
                    })
                    .await;
                }
                _ => {
                    // Mismatched count / parse error: keep stage-1 decision.
                    self.advance_stage2_subline(ctx.app_handle, stage2_done + 1, borderline.len())
                        .await;
                }
            }
        }

        Ok(false)
    }

    /// Advance the `Stage 2: done/total` progress sub-line. Shared by all
    /// stage-2 early-exit arms so the sub-line never stalls.
    async fn advance_stage2_subline(
        &self,
        app_handle: &Option<tauri::AppHandle>,
        done: usize,
        total: usize,
    ) {
        self.update_progress(app_handle, |p| {
            p.stage = Some(format!("Stage 2: {done}/{total} borderline (full text)"));
        })
        .await;
    }
}

/// Pure borderline predicate: `has_full_text && decision != "error" &&
/// confidence in [low, high)`. Extracted so the contract is unit-testable.
#[must_use]
pub(crate) fn is_borderline(
    has_full_text: bool,
    screening: &LlmScreeningResponse,
    low: f64,
    high: f64,
) -> bool {
    has_full_text
        && screening.decision != "error"
        && screening.confidence >= low
        && screening.confidence < high
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(decision: &str, confidence: f64) -> LlmScreeningResponse {
        LlmScreeningResponse {
            decision: decision.to_string(),
            reasoning: "r".to_string(),
            matched_inclusion_criteria: vec![],
            matched_exclusion_criteria: vec![],
            suggested_tags: vec![],
            confidence,
            extracted_terms: vec![],
        }
    }

    #[test]
    fn is_borderline_true_for_full_text_confidence_in_range() {
        assert!(is_borderline(true, &resp("include", 0.55), 0.4, 0.7));
        assert!(is_borderline(true, &resp("exclude", 0.4), 0.4, 0.7)); // low bound inclusive
    }

    #[test]
    fn is_borderline_false_for_no_full_text() {
        assert!(!is_borderline(false, &resp("include", 0.55), 0.4, 0.7));
    }

    #[test]
    fn is_borderline_false_for_error_decision() {
        assert!(!is_borderline(true, &resp("error", 0.55), 0.4, 0.7));
    }

    #[test]
    fn is_borderline_false_above_high_bound() {
        assert!(!is_borderline(true, &resp("include", 0.7), 0.4, 0.7)); // high bound exclusive
        assert!(!is_borderline(true, &resp("include", 0.95), 0.4, 0.7));
    }

    #[test]
    fn is_borderline_false_below_low_bound() {
        assert!(!is_borderline(true, &resp("include", 0.39), 0.4, 0.7));
    }
}
