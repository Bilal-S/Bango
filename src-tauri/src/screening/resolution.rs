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

/// Applies deterministic priority conflict resolution.
///
/// 1. Find the single highest-priority inclusion criterion that matches.
/// 2. Find the single highest-priority exclusion criterion that matches.
/// 3. The higher-priority side wins.
/// 4. If tied, favor inclusion.
/// 5. If no criteria match at all, exclude.
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
