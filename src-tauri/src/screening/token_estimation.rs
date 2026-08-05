use crate::db::app_settings_repo::ScreeningMode;

/// Estimates token count using characters/4 heuristic.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4
}

/// Warning when worst-case per-article tokens exceed 80% of context window.
#[must_use]
pub fn check_context_window(
    template_tokens: usize,
    articles: &[usize],
    context_window_tokens: usize,
) -> Option<String> {
    let worst_case = articles.iter().copied().max().unwrap_or(0) + template_tokens;
    let threshold = (context_window_tokens as f64 * 0.8) as usize;

    if worst_case > threshold {
        Some(format!(
            "Estimated worst-case per-article tokens ({}) exceed 80% of context window ({}). \
             Articles with large abstracts may produce truncated responses.",
            worst_case, threshold,
        ))
    } else {
        None
    }
}

/// Mode-aware worst-case per-article token footprint (§4.3 Readiness Check).
/// Abstract: `abstract + template`. Enhanced: adds `chunk_budget/4`.
/// TwoStage: adds `chunk_budget/4 * borderline_fraction` (only borderline articles
/// pay second-pass cost). Tier 4.1 complementarity is cheaper than chunks-only but
/// worst case unchanged, so formula stays conservative.
#[must_use]
pub fn worst_case_per_article_tokens(
    mode: ScreeningMode,
    abstract_tokens: usize,
    template_tokens: usize,
    chunk_budget_words: usize,
    two_stage_borderline_fraction: f64,
) -> usize {
    let chunk_tokens = chunk_budget_words / 4;
    match mode {
        ScreeningMode::Abstract => abstract_tokens + template_tokens,
        ScreeningMode::Enhanced => abstract_tokens + template_tokens + chunk_tokens,
        ScreeningMode::TwoStage => {
            // Expected per-article cost: abstract (always) + borderline-share
            // of the chunk budget. `(f64 * usize as f64) as usize` truncates,
            // which is the conservative direction (under- rather than
            // over-estimating).
            let borderline_tokens = (chunk_tokens as f64 * two_stage_borderline_fraction) as usize;
            abstract_tokens + template_tokens + borderline_tokens
        }
    }
}
