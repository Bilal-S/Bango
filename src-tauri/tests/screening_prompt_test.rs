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
            // Global numbering: inclusion is 1..N. Single inclusion criterion → 1.
            global_number: 1,
        }],
        exclusion_criteria: vec![CriterionEntry {
            id: "exc-1".to_string(),
            text: "Not about children".to_string(),
            priority: Priority::Standard,
            // Global numbering: exclusion continues N+1..N+M. After 1 inclusion
            // criterion, this exclusion criterion is number 2 (NOT 1).
            global_number: 2,
        }],
        articles: vec![article],
        existing_tags: vec![],
        existing_labels: vec![],
        custom_logic: None,
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

// ── Global numbering + Custom Screening Instructions ──────────────────────

#[test]
fn build_prompt_uses_global_numbering_across_inclusion_and_exclusion() {
    // With 1 inclusion + 1 exclusion, the prompt must label the inclusion
    // criterion `1.` and the exclusion criterion `2.` (continuing the global
    // scheme), NOT `1.` and `1.` (the old per-type restart behavior).
    let prompt = build_screening_prompt(&sample_input(sample_article()));
    assert!(
        prompt.contains("1. [inc-1] Must be about SSB taxes"),
        "inclusion criterion must be numbered with its global number (1): {prompt}"
    );
    assert!(
        prompt.contains("2. [exc-1] Not about children"),
        "exclusion criterion must continue the global numbering at 2 (not restart at 1): {prompt}"
    );
    assert!(!prompt.contains("1. [exc-1]"), "exclusion must NOT restart numbering at 1");
}

#[test]
fn build_prompt_omits_custom_logic_section_when_empty() {
    // `custom_logic: None` → no `## Custom Screening Instructions` header so
    // abstract-mode prompts stay byte-identical to pre-feature shape.
    let prompt = build_screening_prompt(&sample_input(sample_article()));
    assert!(
        !prompt.contains("## Custom Screening Instructions"),
        "prompt must NOT include the custom-logic header when custom_logic is None"
    );
}

#[test]
fn build_prompt_includes_custom_logic_section_when_present() {
    // Non-empty custom_logic → header appears after `## Priority Rules`.
    let mut input = sample_input(sample_article());
    input.custom_logic = Some(
        "Inclusion 1 AND 2 must match. Only then consider inclusion 1 OR exclusion 2.".to_string(),
    );
    let prompt = build_screening_prompt(&input);
    assert!(
        prompt.contains("## Custom Screening Instructions"),
        "prompt must include the custom-logic header when custom_logic is Some(non-empty)"
    );
    assert!(
        prompt.contains("Inclusion 1 AND 2 must match"),
        "prompt must include the user-authored custom-logic text verbatim"
    );
    // The section must come AFTER priority rules (so the LLM reads the base
    // rules first, then the custom override).
    let priority_idx = prompt.find("## Priority Rules");
    let custom_idx = prompt.find("## Custom Screening Instructions");
    assert!(
        priority_idx.is_some()
            && custom_idx.is_some()
            && priority_idx.unwrap() < custom_idx.unwrap(),
        "custom-logic section must appear after the priority-rules section"
    );
}

#[test]
fn build_prompt_omits_custom_logic_section_when_whitespace_only() {
    // Whitespace-only custom_logic must be treated as empty (no section).
    let mut input = sample_input(sample_article());
    input.custom_logic = Some("   \n\t  ".to_string());
    let prompt = build_screening_prompt(&input);
    assert!(
        !prompt.contains("## Custom Screening Instructions"),
        "whitespace-only custom_logic must NOT emit the section"
    );
}

#[test]
fn system_prompt_references_custom_screening_instructions() {
    // The system prompt must instruct the LLM to apply custom screening
    // instructions when present, referencing criteria by their numbered position.
    assert!(
        SYSTEM_PROMPT.contains("Custom Screening Instructions"),
        "SYSTEM_PROMPT must mention the Custom Screening Instructions feature"
    );
    assert!(
        SYSTEM_PROMPT.contains("numbered position"),
        "SYSTEM_PROMPT must instruct the LLM to reference criteria by numbered position"
    );
}

#[test]
fn system_prompt_defines_failed_inclusion_semantics() {
    // The exclusion array doubles as the rejection-reason channel: violated
    // exclusion criteria plus failed inclusion criteria (required but not met).
    assert!(
        SYSTEM_PROMPT.contains("required inclusion criteria the article FAILED"),
        "SYSTEM_PROMPT must define failed-inclusion semantics for the exclusion array"
    );
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

// ── Tag/Label guideline tests ──

#[test]
fn system_prompt_includes_tag_length_instruction() {
    // The system prompt must instruct the LLM that tags are at most 35 chars
    // so the backend sanitization (MAX_NEW_TAG_LABEL_LEN = 35) never has to
    // silently truncate a too-long name and lose context.
    assert!(
        SYSTEM_PROMPT.contains("35 characters"),
        "SYSTEM_PROMPT must specify the 35-char tag limit: {SYSTEM_PROMPT}"
    );
}

#[test]
fn system_prompt_includes_no_prefix_instruction() {
    // The system prompt must tell the LLM NOT to prefix tags with
    // "inclusion:" or "exclusion:" - those prefixes are for labels, not tags.
    assert!(
        SYSTEM_PROMPT.contains("Do NOT prefix tags with"),
        "SYSTEM_PROMPT must instruct the LLM not to prefix tags: {SYSTEM_PROMPT}"
    );
}

#[test]
fn system_prompt_includes_concise_descriptor_instruction() {
    // Tags must be concise descriptors, not full justifications or criterion text.
    assert!(
        SYSTEM_PROMPT.contains("concise descriptors"),
        "SYSTEM_PROMPT must instruct that tags are concise descriptors: {SYSTEM_PROMPT}"
    );
    assert!(
        SYSTEM_PROMPT.contains("NOT justifications"),
        "SYSTEM_PROMPT must instruct that tags are not justifications: {SYSTEM_PROMPT}"
    );
}
