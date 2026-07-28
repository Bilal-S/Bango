use std::collections::{HashMap, HashSet};

use crate::models::criterion::{Criterion, CriterionType};
use crate::screening::engine::LlmScreeningResponse;
use crate::screening::resolution::{self, CriterionMatch};

/// The finalized per-article decision + the data the caller needs to write it
/// to the DB and update progress.
///
/// Produced by `resolve_article_decision` (pure, no I/O). Both stage-1 and
/// stage-2 call it so the criterion-match -> finalize -> augment ->
/// override-annotate -> auto-label pipeline lives in exactly one place.
#[derive(Debug, Clone)]
pub struct ArticleDecision {
    /// The final `"include"` / `"exclude"` string after the priority resolver
    /// (or the LLM's verbatim decision when custom logic governs).
    pub final_decision: String,
    /// The reasoning text, with the `[App override: ...]` annotation appended
    /// when the resolver disagreed with the LLM. Kept raw (with UUIDs) - the
    /// frontend replaces UUIDs dynamically at display time.
    pub reasoning: String,
    /// Inclusion criterion UUIDs (LLM-matched + augmented from reasoning).
    pub augmented_inc: Vec<String>,
    /// Exclusion criterion UUIDs (LLM-matched + augmented from reasoning).
    pub augmented_exc: Vec<String>,
    /// `(prefix, text)` pairs for auto-labelling (`"Inclusion"` / `"Exclusion"`
    /// + the criterion text). The caller formats as `"Inclusion: {text}"`.
    pub auto_label_criteria: Vec<(String, String)>,
    /// Evidence-sections label captured during retrieval (the sections that
    /// *actually* matched), or `None` for abstract-mode screening. Stage-2
    /// overrides this with its local `evidence.sections_label` after calling
    /// `resolve_article_decision`.
    pub evidence_sections: Option<String>,
}

/// Resolve one screening response into a final decision.
///
/// Pure (no I/O, no locks). Extracts the criterion-matching + resolution +
/// augmentation + override-annotation + auto-label-collection that was
/// previously inlined in both the stage-1 and stage-2 per-article loops of
/// `run_sync`, eliminating the verbatim duplication.
///
/// # Parameters
///
/// - `screening`: The parsed LLM response for one article.
/// - `article_id`: The article UUID (used for the evidence-sections lookup).
/// - `criteria`: All criteria (inclusion + exclusion), for match-by-UUID/text.
/// - `inclusion_criteria`: Inclusion-criterion references (used only for its
///   length to drive the augment-from-reasoning split).
/// - `global_numbering`: UUID -> 1-based global index map.
/// - `has_custom_logic`: When `true`, the LLM decision is final (custom
///   screening rules govern); when `false`, the §4.1 priority resolver runs.
/// - `enhanced_evidence_labels`: Article UUID -> evidence-sections label map
///   (Tier 3 Gap 7: the sections that *actually* matched).
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
    // Apply priority resolution - match by UUID or by criterion text.
    let inc_matches: Vec<CriterionMatch> = screening
        .matched_inclusion_criteria
        .iter()
        .filter_map(|key| criteria.iter().find(|c| c.id == *key || c.text == *key))
        .map(|c| CriterionMatch {
            id: c.id.clone(),
            criterion_type: c.criterion_type.clone(),
            priority: c.priority,
        })
        .collect();

    let exc_matches: Vec<CriterionMatch> = screening
        .matched_exclusion_criteria
        .iter()
        .filter_map(|key| criteria.iter().find(|c| c.id == *key || c.text == *key))
        .map(|c| CriterionMatch {
            id: c.id.clone(),
            criterion_type: c.criterion_type.clone(),
            priority: c.priority,
        })
        .collect();

    // Collect auto-label info before the matches are moved into resolution.
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
    // When custom screening rules are present, they govern the process: the
    // LLM's decision is final and the generic priority resolver is not applied.
    let final_decision =
        resolution::finalize_decision(&screening.decision, &resolution_input, has_custom_logic);

    // Augment matched arrays with any criteria UUIDs mentioned in reasoning
    // but missing from the LLM's matched arrays.
    let inclusion_count = inclusion_criteria.len();
    let (augmented_inc, augmented_exc) = augment_matched_from_reasoning(
        &screening.reasoning,
        &screening.matched_inclusion_criteria,
        &screening.matched_exclusion_criteria,
        global_numbering,
        inclusion_count,
    );

    // Keep raw reasoning with UUIDs - frontend replaces dynamically at display
    // time. Append the override annotation when the resolver disagreed.
    let mut reasoning = screening.reasoning.clone();
    if screening.decision.as_str() != final_decision {
        reasoning.push_str(&format!(
            "\n\n[App override: {} favored due to priority resolution]",
            if final_decision == "include" { "inclusion" } else { "exclusion" }
        ));
    }

    // Tier 3 Gap 7: use the precise evidence-sections label captured during
    // retrieval (the sections that *actually* matched), not the configured
    // allow-list.
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

/// Build a global criterion numbering map: UUID -> 1-based index.
///
/// Inclusion criteria are numbered `[1]..[N]`, then exclusion criteria continue
/// `[N+1]..[N+M]`. This ensures `[3]` always refers to the same criterion
/// regardless of which article is displayed.
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

/// Scan reasoning text for criterion UUIDs mentioned but missing from matched
/// arrays, and return augmented (inclusion, exclusion) tuples.
///
/// The LLM sometimes references criteria in reasoning without listing them in
/// the matched arrays. This ensures every referenced criterion appears in the
/// UI table.
///
/// `inclusion_count` is the number of inclusion criteria in the global
/// numbering, used to distinguish inclusion UUIDs (indices 1..N) from exclusion
/// UUIDs (N+1..M).
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
