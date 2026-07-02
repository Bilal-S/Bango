use crate::db::app_settings_repo::ScreeningMode;

/// Estimates token count using characters/4 heuristic.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4
}

/// Estimates whether a screening run might exceed the context window.
/// Returns a warning message if per-article tokens exceed 80% of context_window_tokens.
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

/// Tier 3 Gap 5 + Tier 4.1: compute the mode-aware worst-case per-article token
/// footprint.
///
/// Per `docs/bango-v4-spec.md` §4.3 (Readiness Check): the worst-case footprint
/// is recomputed by the active screening mode.
/// - **Abstract**: `abstract_tokens + template_tokens` (today's abstract-only
///   estimate).
/// - **Enhanced**: adds the per-article chunk budget converted to tokens
///   (`chunk_budget_words / 4`).
/// - **TwoStage**: adds the chunk budget scaled by the expected borderline
///   fraction (only borderline articles pay the second-pass cost).
///
/// **Tier 4.1 note (formula unchanged):** the complementarity evidence path
/// (AI summary + top-1 chunk, ~400 tokens) is _cheaper_ than the chunks-only
/// path (~600 tokens) it replaces when a summary exists. The worst case per
/// article remains the chunks-only path (article has no summary), which is
/// already bounded by `chunk_budget_words / 4`. So the formula stays correct
/// and conservative; the readiness dialog remains truthful. The typical case
/// improves (articles with summaries cost ~400 not ~600), but the worst case
/// does not, so the readiness check (which guards the worst case) is unchanged.
///
/// Pure helper extracted so it can be unit-tested independently of the command
/// shims. `chunk_budget_words` is the per-article word budget (default 2400);
/// `two_stage_borderline_fraction` is the expected share of borderline articles
/// (default 0.15). The `/ 4` factor mirrors the `chars / 4` heuristic used by
/// `estimate_tokens`.
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
