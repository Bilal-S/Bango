//! Language detection for translation pipeline. Pure (`#[must_use]`).
//!
//! Body/full-text language from `articles.language` (set at import, immutable
//! after translation). Abstract heuristic: ASCII range + top-20 English-stopword
//! ratio. No external crate.

/// Top-21 English stopwords (includes `or`).
const ENGLISH_STOPWORDS: [&str; 21] = [
    "the", "of", "and", "to", "a", "in", "is", "for", "with", "on", "by", "this", "that", "from",
    "are", "was", "as", "be", "it", "an", "or",
];

/// Stopword ratio threshold: >= 8% → treated as English.
const ENGLISH_STOPWORD_RATIO_THRESHOLD: f64 = 0.08;

/// Non-latin fraction threshold: >90% non-whitespace chars outside Latin-1 → non-English.
const NON_LATIN_FRACTION_THRESHOLD: f64 = 0.90;

/// Returns true when `language` indicates English (matches `"English"`, `"EN"`,
/// `"en"`). Absent/blank → `false`. For skip-policy, use [`should_skip_translation`].
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

/// Skip-policy gate: returns `true` when article should NOT be translated.
///
/// Per plan §F.2 + §G: skip if language is English OR absent/blank (unknown).
/// Returns `false` only for non-English values (e.g. `"French"`, `"ja"`).
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

/// Hybrid heuristic for abstract language. True = English (skip), false = translate.
///
/// Step 1: if >90% non-whitespace chars outside U+0000-U+00FF → non-English.
/// Step 2: top-21 stopword ratio ≥ 8% → English.
/// Empty abstract → false (no LLM call needed).
#[must_use]
pub fn is_english_abstract(abstract_text: &str) -> bool {
    // Collect non-whitespace chars.
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
