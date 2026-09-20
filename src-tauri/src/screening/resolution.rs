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
/// 1. A failed inclusion that outranks every satisfied inclusion wins (exclude).
/// 2. Highest-priority inclusion vs exclusion wins.
/// 3. Tie = favor inclusion. No criteria = exclude.
#[must_use]
pub fn resolve_decision(input: &ScreeningInput) -> &'static str {
    resolve_decision_with_failed(input, &[])
}

/// `resolve_decision` with the LLM's FAILED-inclusion list.
///
/// Small models can self-contradict: the decision field says `include` while
/// the matched arrays mark a high-priority inclusion criterion as not met
/// (the "West-Germany" live case: UK geography failed, a standard inclusion
/// satisfied, decision `include`). When the failed inclusion strictly
/// outranks every satisfied inclusion the article cannot be included, so the
/// engine forces `exclude`; equal priority keeps the tie-favors-inclusion
/// rule. The LIST is the validated failed set, never raw LLM keys (junk and
/// exclusion-type keys never reach here).
#[must_use]
pub fn resolve_decision_with_failed(
    input: &ScreeningInput,
    failed_inclusions: &[CriterionMatch],
) -> &'static str {
    let highest_inclusion = input.inclusion_matches.iter().max_by_key(|m| m.priority);

    if let Some(failed) = failed_inclusions.iter().max_by_key(|m| m.priority) {
        let outranks_satisfied = match highest_inclusion {
            Some(inc) => failed.priority > inc.priority,
            None => true,
        };
        if outranks_satisfied {
            return "exclude";
        }
    }

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
    finalize_decision_with_failed(llm_decision, input, &[], has_custom_logic)
}

/// `finalize_decision` with the LLM's validated FAILED-inclusion list.
/// Custom logic still suppresses the guard: combinatorial rules are the
/// supreme authority and their decision is final.
#[must_use]
pub fn finalize_decision_with_failed<'a>(
    llm_decision: &'a str,
    input: &ScreeningInput,
    failed_inclusions: &[CriterionMatch],
    has_custom_logic: bool,
) -> &'a str {
    if has_custom_logic {
        llm_decision
    } else {
        resolve_decision_with_failed(input, failed_inclusions)
    }
}
