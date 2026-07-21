use bango_lib::models::criterion::{CriterionType, Priority};
use bango_lib::screening::resolution::{
    finalize_decision, resolve_decision, CriterionMatch, ScreeningInput,
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
