//! Tier 3 §T3.7 binding inventory tests for `screening::prompt`.
//!
//! Covers:
//! - `build_prompt_includes_evidence_block_when_present`
//! - `build_prompt_omits_evidence_block_when_absent` (backward compat)
//! - `build_prompt_evidence_block_labels_section` (`[§Methods]` prefix)

use bango_lib::models::criterion::Priority;
use bango_lib::screening::prompt::{
    build_screening_prompt, AimEntry, ArticleEntry, CriterionEntry, ScreeningPromptInput,
    SYSTEM_PROMPT,
};

fn sample_input(article: ArticleEntry) -> ScreeningPromptInput {
    ScreeningPromptInput {
        aims: vec![AimEntry { text: "Study sugar taxes".to_string() }],
        inclusion_criteria: vec![CriterionEntry {
            id: "inc-1".to_string(),
            text: "Must be about SSB taxes".to_string(),
            priority: Priority::Standard,
        }],
        exclusion_criteria: vec![CriterionEntry {
            id: "exc-1".to_string(),
            text: "Not about children".to_string(),
            priority: Priority::Standard,
        }],
        articles: vec![article],
        existing_tags: vec![],
        existing_labels: vec![],
    }
}

fn sample_article() -> ArticleEntry {
    ArticleEntry {
        title: "Sugar Tax Effects".to_string(),
        authors: "Doe J".to_string(),
        year: Some(2024),
        abstract_text: "We studied the effect of the UK sugar levy on childhood obesity."
            .to_string(),
        full_text_evidence: None,
    }
}

#[test]
fn build_prompt_includes_evidence_block_when_present() {
    let mut article = sample_article();
    article.full_text_evidence = Some(
        "[§Methods] We conducted a quasi-experimental study usingInterrupted time-series."
            .to_string(),
    );
    let prompt = build_screening_prompt(&sample_input(article));
    assert!(
        prompt.contains("Supporting Evidence from Full Text"),
        "prompt must include the evidence block header when evidence is present"
    );
    assert!(
        prompt.contains("verify criteria"),
        "prompt must include the abstract-primary instruction"
    );
}

#[test]
fn build_prompt_omits_evidence_block_when_absent() {
    // Abstract-mode: full_text_evidence = None. The evidence block must be
    // absent so abstract-mode prompts stay byte-identical to pre-Tier-3 shape.
    let prompt = build_screening_prompt(&sample_input(sample_article()));
    assert!(
        !prompt.contains("Supporting Evidence from Full Text"),
        "abstract-mode prompt must NOT include the evidence block header"
    );
}

#[test]
fn build_prompt_evidence_block_labels_section() {
    let mut article = sample_article();
    article.full_text_evidence = Some(
        "[§Methods] Cohort study, N=1234 children.\n[§Results] BMI z-score fell by 0.18."
            .to_string(),
    );
    let prompt = build_screening_prompt(&sample_input(article));
    assert!(prompt.contains("[§Methods]"), "evidence block must label the Methods section");
    assert!(prompt.contains("[§Results]"), "evidence block must label the Results section");
}

// ── Tier 4.1: system-prompt cross-check amendment ────────────────────────────

#[test]
fn system_prompt_includes_cross_check_instruction() {
    // The T4.1 complementarity amendment: when both an AI summary and a verbatim
    // chunk are present, the system prompt must instruct the model to cross-check
    // summary facts against the verbatim chunk. This is the hallucination-
    // propagation mitigation documented in the T4.1 plan.
    assert!(
        SYSTEM_PROMPT.contains("cross-check any summary fact against"),
        "SYSTEM_PROMPT must include the T4.1 cross-check instruction: {SYSTEM_PROMPT}"
    );
    assert!(
        SYSTEM_PROMPT.contains("[Source: AI Summary]"),
        "SYSTEM_PROMPT must reference the AI Summary provenance label: {SYSTEM_PROMPT}"
    );
    assert!(
        SYSTEM_PROMPT.contains("[Source: Full Text - verbatim]"),
        "SYSTEM_PROMPT must reference the verbatim provenance label: {SYSTEM_PROMPT}"
    );
}
