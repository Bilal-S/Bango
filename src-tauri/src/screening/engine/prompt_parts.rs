//! Shared prompt parts built once per run + the `Stage2Context` bundle.
//!
//! `ScreeningPromptParts` is built once at run start (aims, criteria entries,
//! existing tags/labels, custom logic, evidence texts). It feeds both the
//! stage-1 batch prompt and each stage-2 single-article prompt so neither
//! rebuilds from locals.

use std::collections::HashMap;

use crate::models::criterion::{Criterion, ResearchAim};
use crate::screening::engine::ScreeningConfig;
use crate::screening::prompt::{AimEntry, ArticleEntry, CriterionEntry, ScreeningPromptInput};

/// Shared prompt parts built once per run. Cloned into each stage-1 batch
/// prompt and each stage-2 single-article prompt via `build_prompt_input`.
#[derive(Clone)]
pub(crate) struct ScreeningPromptParts {
    aim_entries: Vec<AimEntry>,
    inc_entries: Vec<CriterionEntry>,
    exc_entries: Vec<CriterionEntry>,
    existing_tag_names: Vec<String>,
    existing_label_names: Vec<String>,
    custom_logic: Option<String>,
    /// Consumed by `run_sync` (enhanced-mode evidence retrieval) + stage-2
    /// (borderline evidence retrieval) — hence `pub(crate)`.
    pub(crate) inclusion_texts: Vec<String>,
    pub(crate) exclusion_texts: Vec<String>,
}

impl ScreeningPromptParts {
    /// Build from criteria + aims + settings. Pure (no I/O).
    pub(crate) fn new(
        inclusion_criteria: &[&Criterion],
        exclusion_criteria: &[&Criterion],
        aims: &[ResearchAim],
        global_numbering: &HashMap<String, usize>,
        existing_tags: Vec<String>,
        existing_labels: Vec<String>,
        custom_logic: Option<String>,
    ) -> Self {
        let aim_entries = aims.iter().map(|a| AimEntry { text: a.text.clone() }).collect();
        let inc_entries = inclusion_criteria
            .iter()
            .map(|c| CriterionEntry {
                id: c.id.clone(),
                text: c.text.clone(),
                priority: c.priority,
                global_number: *global_numbering.get(&c.id).unwrap_or(&0),
            })
            .collect();
        let exc_entries = exclusion_criteria
            .iter()
            .map(|c| CriterionEntry {
                id: c.id.clone(),
                text: c.text.clone(),
                priority: c.priority,
                global_number: *global_numbering.get(&c.id).unwrap_or(&0),
            })
            .collect();
        let inclusion_texts = inclusion_criteria.iter().map(|c| c.text.clone()).collect();
        let exclusion_texts = exclusion_criteria.iter().map(|c| c.text.clone()).collect();
        Self {
            aim_entries,
            inc_entries,
            exc_entries,
            existing_tag_names: existing_tags,
            existing_label_names: existing_labels,
            custom_logic,
            inclusion_texts,
            exclusion_texts,
        }
    }

    /// Build a `ScreeningPromptInput` for a batch of article entries, cloning
    /// the shared parts. Used by both stage-1 (batch) and stage-2 (single).
    pub(crate) fn build_prompt_input(&self, articles: Vec<ArticleEntry>) -> ScreeningPromptInput {
        ScreeningPromptInput {
            aims: self.aim_entries.clone(),
            inclusion_criteria: self.inc_entries.clone(),
            exclusion_criteria: self.exc_entries.clone(),
            articles,
            existing_tags: self.existing_tag_names.clone(),
            existing_labels: self.existing_label_names.clone(),
            custom_logic: self.custom_logic.clone(),
        }
    }
}

/// Bundles shared decision data + run-control params for `run_stage2_borderline`
/// so the method stays under `clippy::too_many_arguments`. Built once per batch.
pub(crate) struct Stage2Context<'a> {
    pub(crate) prompt_parts: &'a ScreeningPromptParts,
    pub(crate) criteria: &'a [Criterion],
    pub(crate) inclusion_criteria: &'a [&'a Criterion],
    pub(crate) global_numbering: &'a HashMap<String, usize>,
    pub(crate) has_custom_logic: bool,
    pub(crate) enhanced_evidence_labels: &'a HashMap<String, String>,
    pub(crate) config: &'a ScreeningConfig,
    pub(crate) request_delay_ms: u64,
    pub(crate) app_handle: &'a Option<tauri::AppHandle>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::criterion::{CriterionType, Priority};

    fn criterion(id: &str, text: &str, ctype: CriterionType, priority: Priority) -> Criterion {
        Criterion {
            id: id.to_string(),
            text: text.to_string(),
            criterion_type: ctype,
            priority,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    fn aim(text: &str) -> ResearchAim {
        ResearchAim {
            id: "aim-1".to_string(),
            text: text.to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn build_prompt_input_clones_shared_parts() {
        let inc =
            criterion("inc-1", "Must be about ML", CriterionType::Inclusion, Priority::Standard);
        let exc = criterion(
            "exc-1",
            "Not about healthcare",
            CriterionType::Exclusion,
            Priority::Standard,
        );
        let inc_ref = &inc;
        let exc_ref = &exc;
        let numbering = HashMap::from([("inc-1".to_string(), 1usize), ("exc-1".to_string(), 2)]);
        let parts = ScreeningPromptParts::new(
            &[inc_ref],
            &[exc_ref],
            &[aim("Study AI")],
            &numbering,
            vec!["existing-tag".to_string()],
            vec!["existing-label".to_string()],
            Some("custom rules".to_string()),
        );
        let article = ArticleEntry {
            title: "Article 1".to_string(),
            authors: "Author".to_string(),
            year: Some(2024),
            abstract_text: "Abstract".to_string(),
            full_text_evidence: None,
        };
        let input = parts.build_prompt_input(vec![article.clone()]);
        // Aims + criteria entries + existing tags/labels + custom logic cloned.
        assert_eq!(input.aims.len(), 1);
        assert_eq!(input.inclusion_criteria.len(), 1);
        assert_eq!(input.inclusion_criteria[0].global_number, 1);
        assert_eq!(input.exclusion_criteria[0].global_number, 2);
        assert_eq!(input.articles.len(), 1);
        assert_eq!(input.existing_tags, vec!["existing-tag".to_string()]);
        assert_eq!(input.existing_labels, vec!["existing-label".to_string()]);
        assert_eq!(input.custom_logic.as_deref(), Some("custom rules"));
    }

    #[test]
    fn new_handles_missing_global_numbering_gracefully() {
        // A criterion id absent from the numbering map should default to 0.
        let inc = criterion("orphan", "text", CriterionType::Inclusion, Priority::Standard);
        let inc_ref = &inc;
        let empty_numbering = HashMap::new();
        let parts =
            ScreeningPromptParts::new(&[inc_ref], &[], &[], &empty_numbering, vec![], vec![], None);
        let input = parts.build_prompt_input(vec![]);
        assert_eq!(input.inclusion_criteria[0].global_number, 0);
    }

    #[test]
    fn build_prompt_input_multiple_articles_preserves_order() {
        let parts = ScreeningPromptParts::new(&[], &[], &[], &HashMap::new(), vec![], vec![], None);
        let arts: Vec<_> = (0..3)
            .map(|i| ArticleEntry {
                title: format!("A{i}"),
                authors: "X".to_string(),
                year: Some(2024),
                abstract_text: "abs".to_string(),
                full_text_evidence: None,
            })
            .collect();
        let input = parts.build_prompt_input(arts);
        assert_eq!(
            input.articles.iter().map(|a| a.title.clone()).collect::<Vec<_>>(),
            vec!["A0", "A1", "A2"]
        );
    }

    #[test]
    fn build_prompt_input_clones_evidence_texts_independent_of_input() {
        // The inclusion/exclusion text vectors are built from criteria at
        // construction; build_prompt_input should not touch them (they are
        // consumed via the public field, not the prompt input).
        let inc = criterion("i", "inclusion text", CriterionType::Inclusion, Priority::Standard);
        let inc_ref = &inc;
        let numbering = HashMap::from([("i".to_string(), 1)]);
        let parts =
            ScreeningPromptParts::new(&[inc_ref], &[], &[], &numbering, vec![], vec![], None);
        // `inclusion_texts` is used by evidence retrieval, not by the prompt
        // input; verify it was captured.
        assert_eq!(parts.inclusion_texts.len(), 1);
        assert_eq!(parts.inclusion_texts[0], "inclusion text");
    }
}
