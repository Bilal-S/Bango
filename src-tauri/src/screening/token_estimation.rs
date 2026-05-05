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
