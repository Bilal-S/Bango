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

/// Finalize the screening decision for one article.
///
/// When the user has authored **Custom Screening Instructions**
/// (`has_custom_logic == true`), those combinatorial rules are the supreme
/// authority: the LLM applies them strictly (per the system prompt) and its
/// decision must not be second-guessed by the generic §4.1 priority resolver,
/// which has no understanding of AND/OR gates or hard exclusions. The LLM's
/// decision is returned verbatim in that case.
///
/// When no custom logic is present, the deterministic §4.1 priority resolver
/// runs unchanged (preserving historical behavior for projects that rely on
/// priority levels + the tie-favors-inclusion fallback).
///
/// `llm_decision` must already be lowercased and validated as one of
/// `"include"` / `"exclude"` by the caller (the engine normalizes
/// `"error"` decisions upstream and never reaches this function for them).
///
/// Returns `&str` (with lifetime tied to `llm_decision` when custom logic
/// governs, or the `'static` slice from `resolve_decision` otherwise) so it
/// is a drop-in replacement for `resolve_decision` at the engine call sites -
/// no `String` allocation, no callsite borrow fixups needed.
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
