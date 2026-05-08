use bango_lib::screening::resolution::{resolve_decision, ScreeningInput, CriterionMatch};
use bango_lib::models::criterion::{CriterionType, Priority};

fn make_match(id: &str, c_type: CriterionType, priority: Priority) -> CriterionMatch {
    CriterionMatch {
        id: id.to_string(),
        criterion_type: c_type,
        priority,
    }
}

#[test]
fn test_resolve_no_matches_excludes() {
    let input = ScreeningInput {
        inclusion_matches: vec![],
        exclusion_matches: vec![],
    };
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
