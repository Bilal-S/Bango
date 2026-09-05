//! Tests for the AI criteria generation prompt helper.
//!
//! Validates the harmonization contract: the prompt must surface existing
//! opposite-type criteria and instruct the LLM not to produce negations of
//! them (fixes the "exclusion negates inclusion" bug that bloated
//! search-strategy queries with self-canceling NOT clauses).
//!
//! Binding inventory: `docs/test-plans/criteria-generation-tests.md` (enforced
//! by `scripts/check-test-inventory.sh` via `npm run check:all`).

use bango_lib::commands::criteria::{build_check_rules_prompt, build_criteria_generation_prompt};
use bango_lib::models::criterion::{Criterion, CriterionType, Priority, ResearchAim};

/// Build a minimal aim for tests.
fn aim(text: &str) -> ResearchAim {
    ResearchAim {
        id: format!("aim-{text}").replace(' ', "-"),
        text: text.to_string(),
        created_at: "2026-07-07T00:00:00Z".to_string(),
    }
}

/// Build a minimal criterion for tests.
fn criterion(text: &str, criterion_type: CriterionType, priority: Priority) -> Criterion {
    Criterion {
        id: format!("crit-{text}").replace(' ', "-"),
        criterion_type,
        text: text.to_string(),
        priority,
        created_at: "2026-07-07T00:00:00Z".to_string(),
    }
}

#[test]
fn build_criteria_prompt_includes_opposite_criteria() {
    // When generating exclusion criteria, the existing inclusion criteria
    // must appear in the prompt so the LLM can avoid mirroring them.
    let aims = vec![aim("Designing LLM-assisted screening tools.")];
    let inclusions = vec![
        criterion("LLM-assisted screening", CriterionType::Inclusion, Priority::High),
        criterion("Human-in-the-loop oversight", CriterionType::Inclusion, Priority::Critical),
    ];
    let (system, user) = build_criteria_generation_prompt(&aims, "exclusion", &inclusions);
    assert!(!system.is_empty());
    // Both inclusion texts appear so the LLM sees what not to negate.
    assert!(user.contains("LLM-assisted screening"), "opposite criterion text missing");
    assert!(user.contains("Human-in-the-loop oversight"), "opposite criterion text missing");
    // The section header labels them as the opposite type.
    assert!(user.contains("Existing inclusion Criteria"));
}

#[test]
fn build_criteria_prompt_harmonization_guidance_present() {
    // The prompt must carry the division-of-labor and "do not negate" guidance.
    let aims = vec![aim("Evaluate sugar taxes on obesity.")];
    let (system, user) = build_criteria_generation_prompt(&aims, "inclusion", &[]);
    assert!(system.contains("systematic literature review assistant"));
    assert!(user.contains("must NEVER merely"), "harmonization (no-negation) guidance missing");
    assert!(user.contains("Division of Labor"), "division-of-labor guidance missing");
}

#[test]
fn build_criteria_prompt_aims_only_degrades_gracefully() {
    // Empty opposite list is valid (first generation of either type).
    let aims = vec![aim("Only an aim, no opposite criteria yet.")];
    let (system, user) = build_criteria_generation_prompt(&aims, "inclusion", &[]);
    assert!(!system.is_empty());
    // Placeholder renders so the section is never empty.
    assert!(user.contains("None defined yet."));
}

#[test]
fn build_criteria_prompt_inclusion_and_exclusion_branches() {
    // Both criterion_type values produce valid, distinct prompts with
    // correctly flipped opposite-type labels.
    let aims = vec![aim("Test aim.")];
    let (sys_inc, user_inc) = build_criteria_generation_prompt(&aims, "inclusion", &[]);
    let (sys_exc, user_exc) = build_criteria_generation_prompt(&aims, "exclusion", &[]);
    // Same system prompt role for both.
    assert!(sys_inc.contains("systematic literature review assistant"));
    assert!(sys_exc.contains("systematic literature review assistant"));
    // User prompt names the type being generated.
    assert!(user_inc.contains("inclusion criteria"));
    assert!(user_exc.contains("exclusion criteria"));
    // The opposite-label header flips correctly.
    assert!(user_inc.contains("Existing exclusion Criteria"));
    assert!(user_exc.contains("Existing inclusion Criteria"));
}

#[test]
fn build_check_rules_prompt_flags_negation_guidance() {
    // The holistic review must flag exclusion criteria that merely negate an
    // inclusion criterion and recommend deleting them. This is the
    // defense-in-depth layer that catches negations in ALREADY-EXISTING
    // rulesets (the generation guard only prevents new negations).
    let aims = vec![aim("Evaluate LLM-assisted screening tools.")];
    let inclusion =
        vec![criterion("LLM-assisted screening", CriterionType::Inclusion, Priority::Critical)];
    let exclusion = vec![criterion(
        "No application of LLMs to screening",
        CriterionType::Exclusion,
        Priority::Standard,
    )];
    let (system, user) = build_check_rules_prompt(&aims, &inclusion, &exclusion, None);
    assert!(system.contains("ruleset reviewer"));
    // Negation-detection guidance is present.
    assert!(user.contains("NEGATE"), "negation-detection guidance missing");
    assert!(user.contains("recommend DELETING"), "must recommend deleting negating exclusions");
    // Both criteria texts appear so the LLM can compare them side by side.
    assert!(user.contains("LLM-assisted screening"));
    assert!(user.contains("No application of LLMs to screening"));
}

#[test]
fn build_check_rules_prompt_renders_custom_logic() {
    // Custom screening instructions render when present and fall back to a
    // placeholder when absent; numbering/empty-list handling still degrades
    // gracefully with no criteria.
    let aims = vec![aim("Test aim.")];
    let (_system, user_with) =
        build_check_rules_prompt(&aims, &[], &[], Some("Exclude if publication year < 2010"));
    let (_system, user_without) = build_check_rules_prompt(&aims, &[], &[], None);
    assert!(user_with.contains("Exclude if publication year < 2010"), "custom logic text missing");
    assert!(user_without.contains("(none defined)"), "placeholder missing when no custom logic");
    // Empty criteria lists degrade to the placeholder, not a crash.
    assert!(user_with.contains("(none defined)"));
}
