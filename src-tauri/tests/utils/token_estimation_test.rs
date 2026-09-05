//! Tier 3 Gap 5: tests for the mode-aware worst-case per-article token
//! footprint (`token_estimation::worst_case_per_article_tokens`).
//!
//! Per `docs/bango-v5-spec.md` §4.3 (Readiness Check): the worst-case footprint
//! is recomputed by the active screening mode - Abstract uses the abstract-only
//! estimate; Enhanced adds the per-article chunk budget; Two-stage adds the
//! budget times the expected borderline fraction.

use bango_lib::db::app_settings_repo::ScreeningMode;
use bango_lib::screening::token_estimation::worst_case_per_article_tokens;

const ABSTRACT: usize = 100;
const TEMPLATE: usize = 200;
const BUDGET_WORDS: usize = 2_400;
const BORDERLINE_FRACTION: f64 = 0.15;

#[test]
fn abstract_mode_is_abstract_plus_template_only() {
    let got = worst_case_per_article_tokens(
        ScreeningMode::Abstract,
        ABSTRACT,
        TEMPLATE,
        BUDGET_WORDS,
        BORDERLINE_FRACTION,
    );
    assert_eq!(got, 300, "abstract mode must NOT add chunk budget");
}

#[test]
fn enhanced_mode_adds_full_chunk_budget_as_tokens() {
    let got = worst_case_per_article_tokens(
        ScreeningMode::Enhanced,
        ABSTRACT,
        TEMPLATE,
        BUDGET_WORDS,
        BORDERLINE_FRACTION,
    );
    // 2400 words / 4 = 600 tokens of chunk budget added.
    assert_eq!(got, 300 + 600, "enhanced mode adds the full chunk budget");
}

#[test]
fn two_stage_mode_adds_borderline_share_of_chunk_budget() {
    let got = worst_case_per_article_tokens(
        ScreeningMode::TwoStage,
        ABSTRACT,
        TEMPLATE,
        BUDGET_WORDS,
        BORDERLINE_FRACTION,
    );
    // 2400 words / 4 = 600 chunk tokens; 600 * 0.15 = 90 borderline tokens.
    assert_eq!(got, 300 + 90, "two-stage adds borderline-fraction of the chunk budget");
}

#[test]
fn two_stage_mode_with_zero_borderline_fraction_equals_abstract() {
    // 0.0 → no articles pay the second-pass cost → identical to abstract mode.
    let got = worst_case_per_article_tokens(
        ScreeningMode::TwoStage,
        ABSTRACT,
        TEMPLATE,
        BUDGET_WORDS,
        0.0,
    );
    assert_eq!(got, 300, "two-stage with 0 borderline fraction equals abstract");
}
