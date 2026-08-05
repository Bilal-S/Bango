use crate::models::criterion::{CriterionType, Priority};

/// A criterion matched by the AI during screening.
#[derive(Debug, Clone)]
pub struct CriterionMatch {
    pub id: String,
    pub criterion_type: CriterionType,
    pub priority: Priority,
}

/// Input to the resolution algorithm.
#[derive(Debug, Clone)]
pub struct ScreeningInput {
    pub inclusion_matches: Vec<CriterionMatch>,
    pub exclusion_matches: Vec<CriterionMatch>,
}

/// Deterministic priority conflict resolution:
/// 1. Highest-priority inclusion vs exclusion wins.
/// 2. Tie = favor inclusion. No criteria = exclude.
#[must_use]
pub fn resolve_decision(input: &ScreeningInput) -> &'static str {
    let highest_inclusion = input.inclusion_matches.iter().max_by_key(|m| m.priority);

    let highest_exclusion = input.exclusion_matches.iter().max_by_key(|m| m.priority);

    match (highest_inclusion, highest_exclusion) {
        (None, None) => "exclude",
        (Some(_), None) => "include",
        (None, Some(_)) => "exclude",
        (Some(inc), Some(exc)) => {
            if exc.priority > inc.priority {
                "exclude"
            } else {
                "include"
            }
        }
    }
}

/// Finalize screening decision. Custom logic → LLM verbatim (combinatorial rules
/// transcend priority resolver). No custom logic → §4.1 priority resolver.
/// Returns `&str` tied to `llm_decision` (custom) or `'static` (resolver).
#[must_use]
pub fn finalize_decision<'a>(
    llm_decision: &'a str,
    input: &ScreeningInput,
    has_custom_logic: bool,
) -> &'a str {
    if has_custom_logic {
        llm_decision
    } else {
        resolve_decision(input)
    }
}
