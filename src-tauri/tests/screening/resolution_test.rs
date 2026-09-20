use bango_lib::models::criterion::{CriterionType, Priority};
use bango_lib::screening::resolution::{
    finalize_decision, finalize_decision_with_failed, resolve_decision,
    resolve_decision_with_failed, CriterionMatch, ScreeningInput,
};

fn make_match(id: &str, c_type: CriterionType, priority: Priority) -> CriterionMatch {
    CriterionMatch { id: id.to_string(), criterion_type: c_type, priority }
}

#[test]
fn test_resolve_no_matches_excludes() {
    let input = ScreeningInput { inclusion_matches: vec![], exclusion_matches: vec![] };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_resolve_only_inclusion_includes() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_resolve_only_exclusion_excludes() {
    let input = ScreeningInput {
        inclusion_matches: vec![],
        exclusion_matches: vec![make_match("1", CriterionType::Exclusion, Priority::Standard)],
    };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_resolve_higher_priority_exclusion_wins() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::High)],
    };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_resolve_higher_priority_inclusion_wins() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Critical)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::High)],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_resolve_equal_priority_favors_inclusion() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::High)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::High)],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_resolve_multiple_matches_uses_highest_priority() {
    let input = ScreeningInput {
        inclusion_matches: vec![
            make_match("i1", CriterionType::Inclusion, Priority::Low),
            make_match("i2", CriterionType::Inclusion, Priority::Standard),
        ],
        exclusion_matches: vec![
            make_match("e1", CriterionType::Exclusion, Priority::High),
            make_match("e2", CriterionType::Exclusion, Priority::Low),
        ],
    };
    // Highest Inc is Standard, Highest Exc is High. High wins.
    assert_eq!(resolve_decision(&input), "exclude");
}

// ── failed-inclusion guard (small-model self-contradiction) ────────────────
//
// The "West-Germany" live case: the LLM's decision field said `include` while
// its matched arrays marked the high-priority inclusion "Geography United
// Kingdom" as FAILED (an inclusion key in the exclusion array). A validated
// failed inclusion that strictly outranks every satisfied inclusion must
// force exclude; equal priority keeps tie-favors-inclusion.

#[test]
fn failed_inclusion_outranking_satisfied_inclusion_excludes() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("policy", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![],
    };
    let failed = vec![make_match("uk", CriterionType::Inclusion, Priority::High)];
    assert_eq!(resolve_decision_with_failed(&input, &failed), "exclude");
    assert_eq!(
        resolve_decision(&input),
        "include",
        "the legacy resolver without the failed list keeps prior behavior"
    );
}

#[test]
fn failed_inclusion_equal_priority_keeps_tie_favors_inclusion() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("policy", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![],
    };
    let failed = vec![make_match("substance", CriterionType::Inclusion, Priority::Standard)];
    assert_eq!(
        resolve_decision_with_failed(&input, &failed),
        "include",
        "equal priority must not override the satisfied inclusion"
    );
}

#[test]
fn failed_inclusion_lower_priority_does_not_override() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("policy", CriterionType::Inclusion, Priority::High)],
        exclusion_matches: vec![],
    };
    let failed = vec![make_match("temporal", CriterionType::Inclusion, Priority::Low)];
    assert_eq!(resolve_decision_with_failed(&input, &failed), "include");
}

#[test]
fn failed_inclusion_without_satisfied_inclusion_excludes() {
    let input = ScreeningInput { inclusion_matches: vec![], exclusion_matches: vec![] };
    let failed = vec![make_match("uk", CriterionType::Inclusion, Priority::High)];
    assert_eq!(resolve_decision_with_failed(&input, &failed), "exclude");
}

#[test]
fn failed_inclusion_guard_is_suppressed_by_custom_logic() {
    // Combinatorial custom rules are the supreme authority; the guard must
    // never second-guess a custom-logic decision.
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("policy", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![],
    };
    let failed = vec![make_match("uk", CriterionType::Inclusion, Priority::Critical)];
    assert_eq!(finalize_decision_with_failed("include", &input, &failed, true), "include");
    assert_eq!(finalize_decision_with_failed("include", &input, &failed, false), "exclude");
}

// ── finalize_decision: Custom Screening Instructions governance ────────────
//
// When the user has authored non-empty Custom Screening Instructions, those
// combinatorial rules are the supreme decision authority. The generic §4.1
// priority resolver (tie-favors-inclusion, higher-priority-wins) must NOT
// override the LLM's decision in that case, because it cannot understand
// AND/OR gates or hard exclusions. The LLM applies the custom rules strictly
// (per the system prompt) and its decision is final.

#[test]
fn finalize_decision_with_custom_logic_honors_llm_exclude() {
    // Regression case from the bug report: a tie that would normally favor
    // inclusion (equal-priority inc + exc matches). With custom logic present,
    // the LLM's `exclude` decision must be final - no override.
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("i1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![make_match("e1", CriterionType::Exclusion, Priority::Standard)],
    };
    assert_eq!(
        finalize_decision("exclude", &input, true),
        "exclude",
        "custom logic must suppress the tie-favors-inclusion override"
    );
}

#[test]
fn finalize_decision_with_custom_logic_honors_llm_include() {
    // Symmetric: when custom logic is present and the LLM says `include`, the
    // resolver must not flip it to `exclude` even if the exclusion criterion
    // has higher priority.
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("i1", CriterionType::Inclusion, Priority::Low)],
        exclusion_matches: vec![make_match("e1", CriterionType::Exclusion, Priority::Critical)],
    };
    assert_eq!(
        finalize_decision("include", &input, true),
        "include",
        "custom logic must suppress the higher-priority-wins override"
    );
}

#[test]
fn finalize_decision_without_custom_logic_uses_standard_resolution() {
    // Regression guard: when no custom logic is present, the standard §4.1
    // priority resolver runs unchanged (tie favors inclusion).
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("i1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![make_match("e1", CriterionType::Exclusion, Priority::Standard)],
    };
    assert_eq!(
        finalize_decision("exclude", &input, false),
        "include",
        "without custom logic, tie must favor inclusion (standard resolver runs)"
    );
}

#[test]
fn finalize_decision_with_custom_logic_honors_llm_when_no_criteria_match() {
    // Edge case: no criteria matched at all. Without custom logic this would
    // exclude; with custom logic the LLM's decision is final.
    let input = ScreeningInput { inclusion_matches: vec![], exclusion_matches: vec![] };
    assert_eq!(
        finalize_decision("include", &input, true),
        "include",
        "custom logic must honor the LLM decision even when no criteria matched"
    );
}
