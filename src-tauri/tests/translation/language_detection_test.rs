//! Language detection heuristic tests (language-plan-v2).
//!
//! Covers TC-01 (language metadata import + fallback) and TC-14 (English-
//! abstract skip via the hybrid heuristic).

use bango_lib::translation::language::{is_english_abstract, is_english_language};

#[test]
fn falls_back_to_unknown_when_metadata_missing() {
    // TC-01: absent/blank articles.language -> treat as unknown.
    // `is_english_language` answers the narrow question "is this English?" -
    // absent/blank is NOT English, so it returns false.
    assert!(!is_english_language(None));
    assert!(!is_english_language(Some("")));
    assert!(!is_english_language(Some("   ")));

    // The skip-policy gate (`should_skip_translation`) is what enqueue/engine
    // call sites use: English OR absent/blank -> skip (no translation job).
    // Absent/blank must skip so unknown-language articles are not sent to the
    // LLM (plan §F.2 + §G).
    assert!(bango_lib::translation::language::should_skip_translation(None));
    assert!(bango_lib::translation::language::should_skip_translation(Some("")));
    assert!(bango_lib::translation::language::should_skip_translation(Some("   ")));
}

#[test]
fn metadata_language_wins_when_present() {
    // TC-01: articles.language is the sole original-language source. English
    // variants are recognized; non-English values are not.
    assert!(is_english_language(Some("English")));
    assert!(is_english_language(Some("EN")));
    assert!(is_english_language(Some("en")));
    assert!(is_english_language(Some("english")));
    assert!(!is_english_language(Some("French")));
    assert!(!is_english_language(Some("Spanish")));
    assert!(!is_english_language(Some("ja")));
    assert!(!is_english_language(Some("zh")));
}

#[test]
fn english_abstract_skipped_by_stopword_heuristic() {
    // TC-14: a non-English article (CJK `language`) with an English abstract
    // -> stopword ratio >= 8% -> skip abstract translation.
    let english_abstract = "This study examines the effects of the policy on the \
                            population, and the results indicate that the intervention \
                            was effective for the cohort of interest. The data from \
                            this analysis support further investigation of the topic.";
    // The article's `language` is CJK but the abstract text itself is English;
    // the heuristic should detect it and return true (skip translation).
    assert!(is_english_abstract(english_abstract));
    // Sanity: the language metadata is non-English, but the abstract heuristic
    // is independent of the metadata (per §G "Abstract translation qualifier").
    assert!(!is_english_language(Some("ja")));
}

#[test]
fn latin_script_abstract_translated_by_stopword_heuristic() {
    // TC-14: a French/Spanish abstract passes step 1 (ASCII range) but fails
    // step 2 (stopword ratio < 8%) -> translate.
    let french_abstract = "Cette étude examine les effets de la politique sur la \
                           population, et les résultats indiquent que l'intervention \
                           était efficace pour la cohorte d'intérêt. Les données de \
                           cette analyse soutiennent une investigation plus poussée.";
    assert!(!is_english_abstract(french_abstract));

    let spanish_abstract = "Este estudio examina los efectos de la política en la \
                            población, y los resultados indican que la intervención \
                            fue eficaz para la cohorte de interés. Los datos de este \
                            análisis apoyan una mayor investigación del tema.";
    assert!(!is_english_abstract(spanish_abstract));
}
