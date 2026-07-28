//! Data types for the screening engine: config, run context, progress, LLM response.

use serde::{Deserialize, Serialize};

use crate::db::app_settings_repo::ScreeningMode;

/// Tier 3 screening configuration. Built by the command layer from `app_settings`;
/// pure value type (no I/O).
#[derive(Debug, Clone)]
pub struct ScreeningConfig {
    pub mode: ScreeningMode,
    pub enhanced_top_k: usize,
    pub enhanced_sections: Vec<String>,
    pub two_stage_low: f64,
    pub two_stage_high: f64,
    pub chunk_budget_per_article: usize,
    /// Optional cap on articles to screen this run. `Some(n)` stops after `n`
    /// processed (included + rejected + errors); progress `total` is
    /// `min(n, unscreened_count)`. `None` = screen all (legacy default).
    pub max_articles: Option<usize>,
}

impl Default for ScreeningConfig {
    fn default() -> Self {
        Self {
            mode: ScreeningMode::Abstract,
            enhanced_top_k: crate::screening::chunk_retrieval::DEFAULT_TOP_K,
            enhanced_sections: vec!["Methods".to_string(), "Results".to_string()],
            two_stage_low: 0.4,
            two_stage_high: 0.7,
            chunk_budget_per_article:
                crate::screening::chunk_retrieval::DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
            max_articles: None,
        }
    }
}

/// Per-invocation run-control params. Bundles `request_delay_ms` + `app_handle`
/// + `target_article_id` so `run_sync` stays under `clippy::too_many_arguments`.
#[derive(Clone, Default)]
pub struct RunSyncContext {
    /// Inter-batch throttle (ms); cancellable. Applied between stage-1 batches,
    /// between stage-1 and stage-2, and between stage-2 articles.
    pub request_delay_ms: u64,
    /// When `Some`, emits `screening:progress` events after each mutation.
    /// `None` in tests.
    pub app_handle: Option<tauri::AppHandle>,
    /// `Some(id)` screens one specific article by UUID (per-article "Screen"
    /// button). `None` = normal batch mode.
    pub target_article_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreeningProgress {
    pub total: usize,
    pub completed: usize,
    pub included: usize,
    pub rejected: usize,
    pub errors: usize,
    /// Transient-error deferrals (429, transient 401/403, 5xx, timeout, transport).
    /// NOT counted in `completed`/`errors`; left unscreened for the next run.
    #[serde(default)]
    pub deferred: usize,
    /// Fatal error that stopped the run. `None` for normal completion/cancel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fatal_error: Option<String>,
    /// Non-fatal warning (e.g. slow LLM). Cleared on next success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    pub is_running: bool,
    pub current_article_titles: Vec<String>,
    pub elapsed_ms: u64,
    pub estimated_remaining_ms: Option<u64>,
    /// Two-stage stage label (e.g. `"Stage 2: 3/12 borderline (full text)"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// Two-stage per-stage total (borderline count). `None` for single-stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_total: Option<usize>,
    /// Coarse run-phase label: `"preparing:translating"` / `"preparing:chunking"`
    /// / `"screening"` / `"stage2"`. Diagnostics-only; no behavioral contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmScreeningResponse {
    pub decision: String,
    pub reasoning: String,
    #[serde(
        default,
        alias = "matched_inclusion_criteria",
        alias = "inclusionCriteria",
        alias = "inclusion_criteria"
    )]
    pub matched_inclusion_criteria: Vec<String>,
    #[serde(
        default,
        alias = "matched_exclusion_criteria",
        alias = "exclusionCriteria",
        alias = "exclusion_criteria"
    )]
    pub matched_exclusion_criteria: Vec<String>,
    #[serde(default, alias = "suggested_tags", alias = "tags")]
    pub suggested_tags: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default, alias = "extracted_terms", alias = "extractedTerms")]
    pub extracted_terms: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screening_progress_defaults_deferred_to_zero() {
        let p = ScreeningProgress::default();
        assert_eq!(p.deferred, 0);
        assert_eq!(p.stage, None);
        assert_eq!(p.stage_total, None);
        assert_eq!(p.phase, None);
    }

    #[test]
    fn screening_progress_phase_round_trips_through_serde() {
        let p = ScreeningProgress {
            phase: Some("screening".to_string()),
            stage: Some("Stage 2: 1/3 borderline (full text)".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&p).expect("serialize");
        // `phase` + `stage` should be present (not skipped).
        assert!(json.contains(r#""phase":"screening""#), "phase missing: {json}");
        assert!(json.contains(r#""stage":"Stage 2"#), "stage missing: {json}");
        let back: ScreeningProgress = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.phase.as_deref(), Some("screening"));
        assert_eq!(back.stage.as_deref(), Some("Stage 2: 1/3 borderline (full text)"));
    }

    #[test]
    fn screening_progress_skips_none_optional_fields() {
        let p = ScreeningProgress::default();
        let json = serde_json::to_string(&p).expect("serialize");
        // All Option fields are None → skipped.
        assert!(!json.contains("fatalError"), "fatalError should be skipped: {json}");
        assert!(!json.contains("warning"), "warning should be skipped: {json}");
        assert!(!json.contains("stage"), "stage should be skipped: {json}");
        assert!(!json.contains("phase"), "phase should be skipped: {json}");
    }

    #[test]
    fn llm_response_accepts_legacy_field_aliases() {
        // The LLM may return `inclusionCriteria` / `exclusionCriteria` / `tags`.
        let raw = r#"{"decision":"include","reasoning":"r","inclusionCriteria":["c1"],"exclusionCriteria":[],"tags":["t1"],"confidence":0.9}"#;
        let resp: LlmScreeningResponse = serde_json::from_str(raw).expect("parse");
        assert_eq!(resp.matched_inclusion_criteria, vec!["c1".to_string()]);
        assert_eq!(resp.suggested_tags, vec!["t1".to_string()]);
    }

    #[test]
    fn screening_config_default_is_abstract_mode() {
        let c = ScreeningConfig::default();
        assert_eq!(c.mode, ScreeningMode::Abstract);
        assert!(c.max_articles.is_none());
    }
}
