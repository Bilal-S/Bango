//! Language detection helpers for the translation pipeline.
//!
//! Per the language-detection strategy:
//! - Body/full-text language is read directly from `articles.language` (set at
//!   import time from RIS/BibTeX). It is immutable after translation.
//! - Abstract translation qualifier: a hybrid two-step heuristic (ASCII range +
//!   top-20 English-stopword ratio). No external language-detection crate.
//!
//! All helpers are pure and `#[must_use]` per CLAUDE.md.

/// Top-21 English stopwords used by the abstract heuristic (§G step 2).
/// Includes `or` per the plan's stopword list.
const ENGLISH_STOPWORDS: [&str; 21] = [
    "the", "of", "and", "to", "a", "in", "is", "for", "with", "on", "by", "this", "that", "from",
    "are", "was", "as", "be", "it", "an", "or",
];

/// Stopword ratio at/above which the abstract is treated as English (§G).
const ENGLISH_STOPWORD_RATIO_THRESHOLD: f64 = 0.08;

/// ASCII-range non-whitespace fraction above which the abstract is considered
/// non-English (§G step 1). I.e. if more than 90% of non-whitespace chars fall
/// outside U+0000-U+00FF, the abstract is non-English.
const NON_LATIN_FRACTION_THRESHOLD: f64 = 0.90;

/// Returns true when the `articles.language` metadata value indicates English.
///
/// Matches `"English"`, `"EN"`, and `"en"` case-insensitively. Absent or blank
/// values return `false` (the language is unknown, not English). This helper
/// answers the narrow question "is this value an English-language marker?" and
/// does NOT decide translation skip-policy on its own.
///
/// For the skip-policy decision (English OR absent/blank → skip translation),
/// use [`should_skip_translation`].
#[must_use]
pub fn is_english_language(language: Option<&str>) -> bool {
    match language.map(str::trim).filter(|s| !s.is_empty()) {
        Some(lang) => {
            let lower = lang.to_ascii_lowercase();
            lower == "english" || lower == "en"
        }
        None => false,
    }
}

/// Translation skip-policy gate (plan §F.2 + §G).
///
/// Returns `true` when an article with the given `language` metadata should
/// **not** be translated. Per the plan: "Skip translation entirely if `language`
/// is English ... or absent/blank" and "If the field is absent or blank, treat
/// as unknown and skip translation for this article."
///
/// This is the gate all enqueue/engine call sites must use: absent/blank
/// language must NOT trigger a translation job (it wastes LLM tokens on articles
/// that may already be English, and the language is genuinely unknown).
///
/// Returns `false` only when `language` is a non-English value (e.g. `"French"`,
/// `"ja"`), meaning translation should proceed.
#[must_use]
pub fn should_skip_translation(language: Option<&str>) -> bool {
    match language.map(str::trim).filter(|s| !s.is_empty()) {
        Some(lang) => {
            let lower = lang.to_ascii_lowercase();
            lower == "english" || lower == "en"
        }
        // Absent or blank → unknown → skip translation per plan §G.
        None => true,
    }
}

/// Hybrid ASCII-range + English-stopword heuristic for abstract language.
///
/// Returns `true` when the abstract should be treated as English (skip
/// translation), `false` when it should be translated.
///
/// Steps (§G):
/// 1. ASCII range: if more than 90% of non-whitespace characters fall outside
///    U+0000-U+00FF (Basic Latin + Latin-1 Supplement), the abstract is
///    non-English → return `false` (translate).
/// 2. Stopword ratio: tokenize lowercase whitespace-delimited words and compute
///    the ratio of top-20 English stopwords to total words. If the ratio is
///    >= 8%, treat as English → return `true` (skip translation); otherwise return `false` (translate).
///
/// An empty abstract returns `false` (nothing to translate; the engine treats
/// this as "no LLM call needed").
#[must_use]
pub fn is_english_abstract(abstract_text: &str) -> bool {
    // Collect non-whitespace characters.
    let non_whitespace: Vec<char> = abstract_text.chars().filter(|&c| !c.is_whitespace()).collect();
    if non_whitespace.is_empty() {
        return false;
    }

    // Step 1: ASCII-range check.
    let non_latin = non_whitespace.iter().filter(|&&c| c > '\u{00FF}').count();
    let non_latin_fraction = non_latin as f64 / non_whitespace.len() as f64;
    if non_latin_fraction > NON_LATIN_FRACTION_THRESHOLD {
        return false;
    }

    // Step 2: stopword ratio.
    let words: Vec<&str> = abstract_text.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }
    let stopword_hits = words
        .iter()
        .filter(|w| {
            let lower = w.to_ascii_lowercase();
            // Trim surrounding punctuation for robust matching.
            let trimmed = lower.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
            ENGLISH_STOPWORDS.contains(&trimmed.as_str())
        })
        .count();
    let ratio = stopword_hits as f64 / words.len() as f64;
    ratio >= ENGLISH_STOPWORD_RATIO_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_language_variants() {
        assert!(is_english_language(Some("English")));
        assert!(is_english_language(Some("EN")));
        assert!(is_english_language(Some("en")));
        assert!(is_english_language(Some("english")));
        assert!(is_english_language(Some("  en  ")));
    }

    #[test]
    fn non_english_language_values() {
        assert!(!is_english_language(Some("French")));
        assert!(!is_english_language(Some("Spanish")));
        assert!(!is_english_language(Some("ja")));
        assert!(!is_english_language(Some("zh")));
    }

    #[test]
    fn absent_or_blank_language_is_not_english() {
        assert!(!is_english_language(None));
        assert!(!is_english_language(Some("")));
        assert!(!is_english_language(Some("   ")));
    }

    #[test]
    fn should_skip_translation_for_english_or_absent() {
        // English variants → skip (already English).
        assert!(should_skip_translation(Some("English")));
        assert!(should_skip_translation(Some("EN")));
        assert!(should_skip_translation(Some("en")));

        // Absent or blank → skip (unknown language; plan §G).
        assert!(should_skip_translation(None));
        assert!(should_skip_translation(Some("")));
        assert!(should_skip_translation(Some("   ")));

        // Non-English → do NOT skip (translate).
        assert!(!should_skip_translation(Some("French")));
        assert!(!should_skip_translation(Some("Spanish")));
        assert!(!should_skip_translation(Some("ja")));
        assert!(!should_skip_translation(Some("zh")));
    }

    #[test]
    fn empty_abstract_is_not_english() {
        assert!(!is_english_abstract(""));
        assert!(!is_english_abstract("   "));
    }

    #[test]
    fn english_abstract_passes_heuristic() {
        let abs = "This study examines the effects of the policy on the population, \
                   and the results indicate that the intervention was effective for \
                   the cohort of interest.";
        assert!(is_english_abstract(abs));
    }

    #[test]
    fn french_abstract_fails_heuristic() {
        let abs = "Cette étude examine les effets de la politique sur la population, \
                   et les résultats indiquent que l'intervention était efficace pour \
                   la cohorte d'intérêt.";
        assert!(!is_english_abstract(abs));
    }

    #[test]
    fn cjk_abstract_fails_ascii_step() {
        // Mostly CJK characters → non-latin fraction > 90% → non-English.
        let abs = "本研究は政策の影響を調査します。結果は介入が対象集団にとって\
                   有効であったことを示しています。";
        assert!(!is_english_abstract(abs));
    }
}
