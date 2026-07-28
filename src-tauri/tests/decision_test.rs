use std::collections::HashMap;

use bango_lib::models::criterion::{Criterion, CriterionType, Priority};
use bango_lib::screening::decision::{
    augment_matched_from_reasoning, build_global_criterion_numbering, resolve_article_decision,
};
use bango_lib::screening::engine::LlmScreeningResponse;

fn criterion(id: &str, text: &str, ct: CriterionType, priority: Priority) -> Criterion {
    Criterion {
        id: id.into(),
        text: text.into(),
        criterion_type: ct,
        priority,
        created_at: String::new(),
    }
}

fn response(decision: &str, reasoning: &str, inc: &[&str], exc: &[&str]) -> LlmScreeningResponse {
    LlmScreeningResponse {
        decision: decision.into(),
        reasoning: reasoning.into(),
        matched_inclusion_criteria: inc.iter().map(|s| s.to_string()).collect(),
        matched_exclusion_criteria: exc.iter().map(|s| s.to_string()).collect(),
        suggested_tags: vec![],
        confidence: 0.9,
        extracted_terms: vec![],
    }
}

fn numbering(inc: &[&Criterion], exc: &[&Criterion]) -> HashMap<String, usize> {
    build_global_criterion_numbering(inc, exc)
}

#[test]
fn resolve_article_decision_override_annotates_when_resolver_differs() {
    // LLM says include, but only an exclusion criterion matches. Without custom
    // logic, the resolver should flip to exclude and annotate the reasoning.
    let inc1 = criterion(
        "inc-1",
        "Must be about sugar taxes",
        CriterionType::Inclusion,
        Priority::Standard,
    );
    let exc1 =
        criterion("exc-1", "Not about children", CriterionType::Exclusion, Priority::Critical);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);
    let ev_labels = HashMap::new();

    let screening = response("include", "Matches exclusion.", &[], &["exc-1"]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, false, &ev_labels,
    );

    assert_eq!(
        decision.final_decision, "exclude",
        "resolver should flip to exclude (critical exc wins)"
    );
    assert!(
        decision.reasoning.contains("[App override: exclusion favored due to priority resolution]"),
        "reasoning must carry the override annotation: {}",
        decision.reasoning
    );
}

#[test]
fn resolve_article_decision_override_absent_when_resolver_agrees() {
    // LLM says include, an inclusion criterion matches, no exclusion. Resolver
    // agrees; no override annotation.
    let inc1 = criterion(
        "inc-1",
        "Must be about sugar taxes",
        CriterionType::Inclusion,
        Priority::Standard,
    );
    let exc1 =
        criterion("exc-1", "Not about children", CriterionType::Exclusion, Priority::Standard);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);
    let ev_labels = HashMap::new();

    let screening = response("include", "Matches inclusion.", &["inc-1"], &[]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, false, &ev_labels,
    );

    assert_eq!(decision.final_decision, "include");
    assert!(
        !decision.reasoning.contains("[App override"),
        "no override annotation when resolver agrees: {}",
        decision.reasoning
    );
}

#[test]
fn resolve_article_decision_custom_logic_honors_llm_exclude() {
    // With custom logic in force, the LLM decision is final even if the
    // resolver would have flipped it.
    let inc1 = criterion(
        "inc-1",
        "Must be about sugar taxes",
        CriterionType::Inclusion,
        Priority::Critical,
    );
    let exc1 = criterion("exc-1", "Not about children", CriterionType::Exclusion, Priority::Low);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);
    let ev_labels = HashMap::new();

    // LLM says exclude despite a critical inclusion match + low exclusion.
    // Without custom logic the resolver would include; with custom logic the
    // LLM exclude stands.
    let screening = response("exclude", "Custom rule rejects.", &["inc-1"], &["exc-1"]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, true, &ev_labels,
    );

    assert_eq!(decision.final_decision, "exclude", "custom logic must honor the LLM decision");
    assert!(
        !decision.reasoning.contains("[App override"),
        "no override annotation when custom logic governs"
    );
}

#[test]
fn resolve_article_decision_custom_logic_honors_llm_include() {
    let inc1 = criterion(
        "inc-1",
        "Must be about sugar taxes",
        CriterionType::Inclusion,
        Priority::Standard,
    );
    let exc1 =
        criterion("exc-1", "Not about children", CriterionType::Exclusion, Priority::Critical);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);
    let ev_labels = HashMap::new();

    // LLM says include despite a critical exclusion match. Custom logic honors it.
    let screening = response("include", "Custom rule includes.", &["inc-1"], &["exc-1"]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, true, &ev_labels,
    );

    assert_eq!(decision.final_decision, "include", "custom logic must honor the LLM include");
}

#[test]
fn resolve_article_decision_no_custom_logic_uses_priority_resolver() {
    // No custom logic: critical exclusion beats standard inclusion -> exclude.
    let inc1 = criterion(
        "inc-1",
        "Must be about sugar taxes",
        CriterionType::Inclusion,
        Priority::Standard,
    );
    let exc1 =
        criterion("exc-1", "Not about children", CriterionType::Exclusion, Priority::Critical);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);
    let ev_labels = HashMap::new();

    let screening = response("include", "Both match.", &["inc-1"], &["exc-1"]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, false, &ev_labels,
    );

    assert_eq!(
        decision.final_decision, "exclude",
        "critical exclusion wins over standard inclusion"
    );
}

#[test]
fn resolve_article_decision_augments_from_reasoning_global_numbers() {
    // UUID mentioned in reasoning but missing from matched arrays should be
    // augmented into the matched set.
    let inc1 = criterion("uuid-1", "Inclusion 1", CriterionType::Inclusion, Priority::Standard);
    let exc1 = criterion("uuid-2", "Exclusion 1", CriterionType::Exclusion, Priority::Standard);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);
    let ev_labels = HashMap::new();

    // Reasoning mentions uuid-1 but it's not in the matched arrays.
    let screening = response("include", "uuid-1 is relevant.", &[], &[]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, false, &ev_labels,
    );

    assert!(
        decision.augmented_inc.contains(&"uuid-1".to_string()),
        "uuid-1 should be augmented into inclusion: {:?}",
        decision.augmented_inc
    );
}

#[test]
fn resolve_article_decision_collects_auto_label_criteria() {
    let inc1 = criterion("inc-1", "sugar tax policy", CriterionType::Inclusion, Priority::Standard);
    let exc1 = criterion("exc-1", "animal study", CriterionType::Exclusion, Priority::Standard);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);
    let ev_labels = HashMap::new();

    let screening = response("include", "Matches.", &["inc-1"], &["exc-1"]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, false, &ev_labels,
    );

    // Both matches should produce auto-label pairs.
    let labels: Vec<_> = decision.auto_label_criteria.iter().map(|(p, _)| p.as_str()).collect();
    assert!(
        labels.contains(&"Inclusion"),
        "auto-label should include Inclusion prefix: {:?}",
        labels
    );
    assert!(
        labels.contains(&"Exclusion"),
        "auto-label should include Exclusion prefix: {:?}",
        labels
    );
}

#[test]
fn resolve_article_decision_evidence_sections_from_map() {
    let inc1 = criterion(
        "inc-1",
        "Must be about sugar taxes",
        CriterionType::Inclusion,
        Priority::Standard,
    );
    let exc1 =
        criterion("exc-1", "Not about children", CriterionType::Exclusion, Priority::Standard);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);

    let mut ev_labels = HashMap::new();
    ev_labels.insert("art-1".to_string(), "§Methods, §Results".to_string());

    let screening = response("include", "Matches.", &["inc-1"], &[]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, false, &ev_labels,
    );

    assert_eq!(decision.evidence_sections.as_deref(), Some("§Methods, §Results"));
}

#[test]
fn resolve_article_decision_evidence_sections_none_when_absent() {
    let inc1 = criterion(
        "inc-1",
        "Must be about sugar taxes",
        CriterionType::Inclusion,
        Priority::Standard,
    );
    let exc1 =
        criterion("exc-1", "Not about children", CriterionType::Exclusion, Priority::Standard);
    let criteria = vec![inc1.clone(), exc1.clone()];
    let inc_refs = vec![&inc1];
    let exc_refs = vec![&exc1];
    let global = numbering(&inc_refs, &exc_refs);
    let ev_labels = HashMap::new();

    let screening = response("include", "Matches.", &["inc-1"], &[]);
    let decision = resolve_article_decision(
        &screening, "art-1", &criteria, &inc_refs, &global, false, &ev_labels,
    );

    assert!(decision.evidence_sections.is_none(), "abstract-mode (no evidence) should yield None");
}

// Re-verify the moved helpers still work via the decision module path.
#[test]
fn build_global_criterion_numbering_sequential_via_decision_module() {
    let inc1 = criterion("a", "I1", CriterionType::Inclusion, Priority::Standard);
    let inc2 = criterion("b", "I2", CriterionType::Inclusion, Priority::Standard);
    let exc1 = criterion("c", "E1", CriterionType::Exclusion, Priority::Standard);
    let map = build_global_criterion_numbering(&[&inc1, &inc2], &[&exc1]);
    assert_eq!(map.get("a"), Some(&1));
    assert_eq!(map.get("b"), Some(&2));
    assert_eq!(map.get("c"), Some(&3));
}

#[test]
fn augment_matched_from_reasoning_via_decision_module() {
    let mut global = HashMap::new();
    global.insert("uuid-1".to_string(), 1);
    let (inc, _exc) = augment_matched_from_reasoning("uuid-1 mentioned", &[], &[], &global, 1);
    assert!(inc.contains(&"uuid-1".to_string()));
}
