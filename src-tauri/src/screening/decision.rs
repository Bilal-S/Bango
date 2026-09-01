use std::collections::{HashMap, HashSet};

use crate::models::criterion::{Criterion, CriterionType};
use crate::screening::engine::LlmScreeningResponse;
use crate::screening::resolution::{self, CriterionMatch};

/// Finalized per-article decision + data needed by caller for DB write + progress update.
/// Pure; produced by `resolve_article_decision`. Both stage-1 and stage-2 call it.
#[derive(Debug, Clone)]
pub struct ArticleDecision {
    /// Final `"include"` / `"exclude"` after priority resolver (or LLM verbatim when custom logic governs).
    pub final_decision: String,
    /// Reasoning text; `[App override: ...]` appended when resolver disagreed with LLM.
    pub reasoning: String,
    /// Inclusion criterion UUIDs (LLM-matched + augmented from reasoning).
    pub augmented_inc: Vec<String>,
    /// Exclusion criterion UUIDs (LLM-matched + augmented from reasoning).
    pub augmented_exc: Vec<String>,
    /// `(prefix, text)` pairs for auto-labelling (`"Inclusion"` / `"Exclusion"` + criterion text).
    pub auto_label_criteria: Vec<(String, String)>,
    /// Evidence-sections label from retrieval (sections that *actually* matched),
    /// or `None` for abstract mode. Stage-2 overrides with its local label.
    pub evidence_sections: Option<String>,
}

/// Resolve one screening response into a final decision. Pure.
///
/// Extracts criterion-matching + resolution + augmentation + override-annotation +
/// auto-label-collection. When `has_custom_logic`, LLM decision is final;
/// otherwise §4.1 priority resolver runs.
#[must_use]
pub fn resolve_article_decision(
    screening: &LlmScreeningResponse,
    article_id: &str,
    criteria: &[Criterion],
    inclusion_criteria: &[&Criterion],
    global_numbering: &HashMap<String, usize>,
    has_custom_logic: bool,
    enhanced_evidence_labels: &HashMap<String, String>,
) -> ArticleDecision {
    /* Normalize LLM keys (UUID, exact text, or global criterion number) into
    satisfied-inclusion / violated-exclusion / FAILED-inclusion lists; junk is
    dropped. */
    let (resolved_inc, resolved_exc, failed_inc) = resolve_matched_keys(
        &screening.matched_inclusion_criteria,
        &screening.matched_exclusion_criteria,
        criteria,
        global_numbering,
    );

    // Apply priority resolution - match by resolved UUID.
    let inc_matches: Vec<CriterionMatch> = resolved_inc
        .iter()
        .filter_map(|key| criteria.iter().find(|c| c.id == *key))
        .map(|c| CriterionMatch {
            id: c.id.clone(),
            criterion_type: c.criterion_type.clone(),
            priority: c.priority,
        })
        .collect();

    let exc_matches: Vec<CriterionMatch> = resolved_exc
        .iter()
        .filter_map(|key| criteria.iter().find(|c| c.id == *key))
        .map(|c| CriterionMatch {
            id: c.id.clone(),
            criterion_type: c.criterion_type.clone(),
            priority: c.priority,
        })
        .collect();

    // Collect auto-label info before matches are moved into resolution.
    let auto_label_criteria: Vec<(String, String)> = inc_matches
        .iter()
        .chain(exc_matches.iter())
        .filter_map(|m| {
            criteria.iter().find(|cr| cr.id == m.id).map(|cr| {
                (
                    if matches!(cr.criterion_type, CriterionType::Inclusion) {
                        "Inclusion"
                    } else {
                        "Exclusion"
                    }
                    .to_string(),
                    cr.text.clone(),
                )
            })
        })
        .collect();

    let resolution_input = resolution::ScreeningInput {
        inclusion_matches: inc_matches,
        exclusion_matches: exc_matches,
    };
    // Custom logic: LLM decision final; priority resolver not applied.
    let final_decision =
        resolution::finalize_decision(&screening.decision, &resolution_input, has_custom_logic);

    /* Augment matched arrays with criteria UUIDs mentioned in reasoning
    but missing from the resolved matched arrays. */
    let inclusion_count = inclusion_criteria.len();
    let (augmented_inc, mut augmented_exc) = augment_matched_from_reasoning(
        &screening.reasoning,
        &resolved_inc,
        &resolved_exc,
        global_numbering,
        inclusion_count,
    );

    /* Failed inclusion criteria (required but not met) merge into the stored
    exclusion array - implicit cross-type storage, resolved by criterion type
    at display/report time. They never join `inc_matches`/`exc_matches`, so
    they cannot influence the priority resolver or generate auto-labels. */
    for id in failed_inc {
        if !augmented_exc.contains(&id) {
            augmented_exc.push(id);
        }
    }

    /* Raw reasoning kept with UUIDs - frontend replaces at display time.
    Append override annotation when resolver disagreed. */
    let mut reasoning = screening.reasoning.clone();
    if screening.decision.as_str() != final_decision {
        reasoning.push_str(&format!(
            "\n\n[App override: {} favored due to priority resolution]",
            if final_decision == "include" { "inclusion" } else { "exclusion" }
        ));
    }

    /* Use evidence-sections label from retrieval (sections that actually matched),
    not the configured allow-list. */
    let evidence_sections = enhanced_evidence_labels.get(article_id).cloned();

    ArticleDecision {
        final_decision: final_decision.to_string(),
        reasoning,
        augmented_inc,
        augmented_exc,
        auto_label_criteria,
        evidence_sections,
    }
}

/// UUID -> 1-based global index map. Inclusion `[1]..[N]`, exclusion `[N+1]..[N+M]`.
#[must_use]
pub fn build_global_criterion_numbering(
    inclusion_criteria: &[&Criterion],
    exclusion_criteria: &[&Criterion],
) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    let mut n = 1usize;
    for c in inclusion_criteria {
        map.insert(c.id.clone(), n);
        n += 1;
    }
    for c in exclusion_criteria {
        map.insert(c.id.clone(), n);
        n += 1;
    }
    map
}

/// Resolve raw LLM matched-criteria keys to criterion UUIDs split by meaning:
/// satisfied inclusion (inclusion key via the inclusion array), violated
/// exclusion (exclusion key via the exclusion array), and FAILED inclusion
/// (inclusion key via the exclusion array - the required criterion was not met
/// and drove the rejection). A key may be a criterion UUID, the exact
/// criterion text, or the criterion's global number in any common shape
/// ("3", "[3]", "#3", "3."). Exclusion keys via the inclusion array have no
/// defined meaning and are dropped, as is unresolvable junk. Dedupes across
/// all three lists, preserving first-seen order.
#[must_use]
pub fn resolve_matched_keys(
    inclusion_keys: &[String],
    exclusion_keys: &[String],
    criteria: &[Criterion],
    global_numbering: &HashMap<String, usize>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let by_number: HashMap<usize, &str> =
        global_numbering.iter().map(|(uuid, &n)| (n, uuid.as_str())).collect();

    let mut seen: HashSet<&str> = HashSet::new();
    let mut satisfied_inc = Vec::new();
    let mut violated_exc = Vec::new();
    let mut failed_inc = Vec::new();

    let keyed = inclusion_keys
        .iter()
        .map(|key| (key, false))
        .chain(exclusion_keys.iter().map(|key| (key, true)));
    for (key, from_exclusion_array) in keyed {
        let Some(criterion) = lookup_criterion(key.trim(), criteria, &by_number) else {
            continue;
        };
        if seen.contains(criterion.id.as_str()) {
            continue;
        }
        let is_inclusion = matches!(criterion.criterion_type, CriterionType::Inclusion);
        // A key in the undefined slot (exclusion key via the inclusion array)
        // is dropped WITHOUT marking seen, so a later meaningful placement of
        // the same criterion still records.
        let recorded = match (is_inclusion, from_exclusion_array) {
            (true, false) => {
                satisfied_inc.push(criterion.id.clone());
                true
            }
            (false, true) => {
                violated_exc.push(criterion.id.clone());
                true
            }
            (true, true) => {
                failed_inc.push(criterion.id.clone());
                true
            }
            (false, false) => false,
        };
        if recorded {
            seen.insert(criterion.id.as_str());
        }
    }

    (satisfied_inc, violated_exc, failed_inc)
}

/// Find the criterion one raw LLM key refers to: exact UUID, exact text, or
/// global number via the reverse numbering map.
fn lookup_criterion<'a>(
    key: &str,
    criteria: &'a [Criterion],
    by_number: &HashMap<usize, &str>,
) -> Option<&'a Criterion> {
    if let Some(c) = criteria.iter().find(|c| c.id == key || c.text == key) {
        return Some(c);
    }
    let n = parse_criterion_number(key)?;
    let uuid = by_number.get(&n).copied()?;
    criteria.iter().find(|c| c.id == uuid)
}

/// Parse a raw LLM key as a 1-based global criterion number.
/// Accepts "3", "[3]", "#3", " 3 ", "3."; rejects 0 and negatives.
#[must_use]
pub fn parse_criterion_number(key: &str) -> Option<usize> {
    let core = key.trim().trim_start_matches(['[', '#']).trim_end_matches([']', '.']);
    core.parse::<usize>().ok().filter(|&n| n > 0)
}

/// Scan reasoning for criterion UUIDs missing from matched arrays, return augmented tuples.
/// `inclusion_count` distinguishes inclusion UUIDs (indices 1..N) from exclusion (N+1..M).
#[must_use]
pub fn augment_matched_from_reasoning(
    reasoning: &str,
    matched_inclusion_ids: &[String],
    matched_exclusion_ids: &[String],
    global_map: &HashMap<String, usize>,
    inclusion_count: usize,
) -> (Vec<String>, Vec<String>) {
    let inc_set: HashSet<&str> = matched_inclusion_ids.iter().map(|s| s.as_str()).collect();
    let exc_set: HashSet<&str> = matched_exclusion_ids.iter().map(|s| s.as_str()).collect();

    let mut extra_inclusion = Vec::new();
    let mut extra_exclusion = Vec::new();

    for (uuid, &idx) in global_map {
        if inc_set.contains(uuid.as_str()) || exc_set.contains(uuid.as_str()) {
            continue; // Already in matched arrays
        }
        if reasoning.contains(uuid.as_str()) {
            // Inclusion criteria have indices 1..inclusion_count
            if idx <= inclusion_count {
                extra_inclusion.push(uuid.clone());
            } else {
                extra_exclusion.push(uuid.clone());
            }
        }
    }

    let mut augmented_inc = matched_inclusion_ids.to_vec();
    let mut augmented_exc = matched_exclusion_ids.to_vec();
    augmented_inc.extend(extra_inclusion);
    augmented_exc.extend(extra_exclusion);

    (augmented_inc, augmented_exc)
}
