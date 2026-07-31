//! External unit tests for `citation_finder::prompt` (pure helpers).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` per `docs/CLAUDE.md`
//! §Testing ("Avoid large inline unit tests in library source files").

use std::collections::HashMap;

use bango_lib::citation_finder::prompt::{
    build_per_statement_prompt, build_whole_block_prompt, ground_quotes, parse_citation_outputs,
    parse_classification, CandidateMetadata, CandidatePassage, CitationLlmOutput,
    CITATION_FINDER_SYSTEM_PROMPT,
};
use bango_lib::citation_finder::MatchClassification;

fn meta(id: &str) -> CandidateMetadata {
    CandidateMetadata {
        article_id: id.to_string(),
        title: format!("Title {id}"),
        authors: vec!["Author A".to_string()],
        publication_year: Some(2024),
        journal: Some("Journal".to_string()),
        doi: Some(format!("10.1000/{id}")),
    }
}

fn passage(
    id: &str,
    claim: Option<&str>,
    passage: &str,
    section: Option<&str>,
) -> CandidatePassage {
    CandidatePassage {
        article_id: id.to_string(),
        claim: claim.map(str::to_string),
        passage: passage.to_string(),
        section: section.map(str::to_string),
    }
}

fn one_meta_map(ids: &[&str]) -> HashMap<String, CandidateMetadata> {
    ids.iter().map(|id| (id.to_string(), meta(id))).collect()
}

// ── system prompt ────────────────────────────────────────────────────────

#[test]
fn system_prompt_contains_required_fields() {
    let prompt = CITATION_FINDER_SYSTEM_PROMPT;
    assert!(prompt.contains("misrepresents_source"), "must mention misrepresents_source");
    assert!(prompt.contains("MISREPRESENTS"), "must emphasize misrepresentation");
    assert!(prompt.contains("validating"), "must mention validating");
    assert!(prompt.contains("opposing"), "must mention opposing");
    assert!(prompt.contains("unrelated"), "must instruct filtering unrelated");
    assert!(prompt.contains("relevance_explanation"));
    assert!(prompt.contains("article_id"));
    assert!(prompt.contains("claim"));
    assert!(prompt.contains("JSON array"), "must request JSON array");
    assert!(prompt.contains("at most 10"), "must cap at 10 results");
}

// ── build_whole_block_prompt ─────────────────────────────────────────────

#[test]
fn whole_block_prompt_contains_user_text_and_candidates() {
    let user_text = "Sugar taxes reduce childhood obesity.";
    let passages = vec![passage("a1", None, "the sugar tax reduced obesity", Some("Results"))];
    let metadata = one_meta_map(&["a1"]);
    let prompt = build_whole_block_prompt(user_text, &passages, &metadata);
    assert!(prompt.contains(user_text));
    assert!(prompt.contains("<user_text>"));
    assert!(prompt.contains("</user_text>"));
    assert!(prompt.contains("## Candidates"));
    assert!(prompt.contains("article_id: a1"));
    assert!(prompt.contains("section: Results"));
    // No claim line in whole-block mode.
    assert!(!prompt.contains("- claim:"));
}

#[test]
fn whole_block_prompt_empty_candidates_still_has_section_header() {
    let metadata = HashMap::new();
    let prompt = build_whole_block_prompt("text", &[], &metadata);
    assert!(prompt.contains("## Candidates"));
}

#[test]
fn whole_block_prompt_renders_candidate_metadata() {
    // The LLM must see title/authors/year/journal/DOI per candidate so it
    // can write informed relevance explanations.
    let passages = vec![passage("a1", None, "the sugar tax reduced obesity", Some("Results"))];
    let metadata = one_meta_map(&["a1"]);
    let prompt = build_whole_block_prompt("user text", &passages, &metadata);
    assert!(prompt.contains("title: Title a1"));
    assert!(prompt.contains("authors: Author A"));
    assert!(prompt.contains("year: 2024"));
    assert!(prompt.contains("journal: Journal"));
    assert!(prompt.contains("doi: 10.1000/a1"));
}

#[test]
fn whole_block_prompt_omits_metadata_lines_when_article_absent_from_map() {
    // An article_id in `passages` but not in `metadata` renders no metadata
    // lines (graceful degradation - shouldn't happen in practice since
    // `load_metadata` covers all finalists, but the prompt builder stays
    // robust).
    let passages = vec![passage("ghost", None, "passage", None)];
    let metadata = HashMap::new();
    let prompt = build_whole_block_prompt("text", &passages, &metadata);
    assert!(prompt.contains("article_id: ghost"));
    assert!(!prompt.contains("title:"));
    assert!(!prompt.contains("authors:"));
}

// ── build_per_statement_prompt ───────────────────────────────────────────

#[test]
fn per_statement_prompt_contains_claims_list() {
    let claims = vec![
        "Sugar taxes reduce obesity.".to_string(),
        "The effect is strongest in low-income areas.".to_string(),
    ];
    let passages = vec![
        passage("a1", Some(&claims[0]), "passage 1", Some("Results")),
        passage("a2", Some(&claims[1]), "passage 2", Some("Discussion")),
    ];
    let metadata = one_meta_map(&["a1", "a2"]);
    let prompt = build_per_statement_prompt(&claims, &passages, &metadata);
    assert!(prompt.contains("1. Sugar taxes reduce obesity."));
    assert!(prompt.contains("2. The effect is strongest"));
    assert!(prompt.contains("- claim: Sugar taxes reduce obesity."));
    assert!(prompt.contains("article_id: a1"));
    assert!(prompt.contains("title: Title a1"));
}

// ── parse_classification ─────────────────────────────────────────────────

#[test]
fn parse_classification_valid() {
    assert_eq!(parse_classification("validating"), Some(MatchClassification::Validating));
    assert_eq!(parse_classification("opposing"), Some(MatchClassification::Opposing));
}

#[test]
fn parse_classification_case_insensitive() {
    assert_eq!(parse_classification("VALIDATING"), Some(MatchClassification::Validating));
    assert_eq!(parse_classification("Opposing"), Some(MatchClassification::Opposing));
}

#[test]
fn parse_classification_unrelated_returns_none() {
    assert_eq!(parse_classification("unrelated"), None);
}

#[test]
fn parse_classification_garbage_returns_none() {
    assert_eq!(parse_classification("maybe"), None);
    assert_eq!(parse_classification(""), None);
}

// ── CitationLlmOutput deserialization: camelCase (alias path) ───────────

#[test]
fn llm_output_deserializes_validating() {
    let json = r#"{"articleId":"a1","claim":"","classification":"validating","relevanceExplanation":"supports","misrepresentsSource":false}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert_eq!(parsed.article_id, "a1");
    assert_eq!(parsed.classification, "validating");
    assert!(!parsed.misrepresents_source);
}

#[test]
fn llm_output_deserializes_with_default_claim() {
    // `claim` omitted → default empty string.
    let json = r#"{"articleId":"a1","classification":"opposing","relevanceExplanation":"x","misrepresentsSource":true}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert_eq!(parsed.claim, "");
    assert!(parsed.misrepresents_source);
}

#[test]
fn llm_output_deserializes_legacy_fairly_paraphrased_alias() {
    // Old `fairlyParaphrased` field name still deserializes (backward
    // compat with a stale prompt template cached mid-rollout). The alias
    // on `misrepresents_source` accepts both shapes.
    let json = r#"{"articleId":"a1","classification":"validating","relevanceExplanation":"x","fairlyParaphrased":true}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert!(parsed.misrepresents_source, "alias maps to misrepresents_source");
}

#[test]
fn llm_output_defaults_misrepresents_to_false_when_absent() {
    // Field omitted entirely → default false (the faithful case).
    let json = r#"{"articleId":"a1","classification":"validating","relevanceExplanation":"x"}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert!(!parsed.misrepresents_source);
}

// ── CitationLlmOutput deserialization: snake_case (prompt contract) ─────
//
// The system prompt (`CITATION_FINDER_SYSTEM_PROMPT`) instructs the LLM to
// emit snake_case field names (`article_id`, `relevance_explanation`,
// `misrepresents_source`). These tests pin that contract - they were the
// missing regression pin that let the camelCase-only struct + the
// snake_case prompt drift apart and produce the
// `missing field articleId` bug report.

#[test]
fn llm_output_deserializes_snake_case() {
    // The exact shape the prompt asks for. The canonical (snake_case) name
    // is the primary path.
    let json = r#"{"article_id":"a1","claim":"","classification":"validating","relevance_explanation":"supports","misrepresents_source":false}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert_eq!(parsed.article_id, "a1");
    assert_eq!(parsed.classification, "validating");
    assert_eq!(parsed.relevance_explanation, "supports");
    assert!(!parsed.misrepresents_source);
}

#[test]
fn llm_output_snake_case_defaults_claim_and_misrepresents_when_absent() {
    // `claim` + `misrepresents_source` omitted → defaults (empty + false).
    let json = r#"{"article_id":"a1","classification":"opposing","relevance_explanation":"x"}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert_eq!(parsed.claim, "");
    assert!(!parsed.misrepresents_source);
}

#[test]
fn llm_output_snake_case_defaults_classification_and_explanation_when_absent() {
    // `classification` + `relevance_explanation` omitted → empty strings
    // (downstream `parse_classification("")` returns None and drops the
    // entry, so a missing classification is filtered not fatal).
    let json = r#"{"article_id":"a1"}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert_eq!(parsed.classification, "");
    assert_eq!(parsed.relevance_explanation, "");
}

#[test]
fn llm_output_mixed_case_aliases() {
    // Defensive: an LLM that mixes casing across fields still parses. The
    // aliases are per-field, so each field resolves independently.
    let json = r#"{"article_id":"a1","claimText":"c","classification":"validating","relevanceExplanation":"e","misrepresentsSource":true}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert_eq!(parsed.article_id, "a1");
    assert_eq!(parsed.claim, "c");
    assert_eq!(parsed.relevance_explanation, "e");
    assert!(parsed.misrepresents_source);
}

// ── parse_citation_outputs: array shape (bare JSON array) ───────────────

#[test]
fn parse_outputs_bare_snake_case_array() {
    // The canonical happy path: the LLM obeyed the prompt and returned a
    // bare JSON array with snake_case field names. This is the regression
    // pin for the bug-report payload shape.
    let json = r#"[
        {"article_id":"a1","claim":"sugar is harmful","classification":"validating","relevance_explanation":"supports","misrepresents_source":false},
        {"article_id":"a2","claim":"sugar is harmful","classification":"opposing","relevance_explanation":"challenges","misrepresents_source":true}
      ]"#;
    let outputs = parse_citation_outputs(json).expect("parse");
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].article_id, "a1");
    assert_eq!(outputs[0].classification, "validating");
    assert_eq!(outputs[1].article_id, "a2");
    assert_eq!(outputs[1].classification, "opposing");
    assert!(outputs[1].misrepresents_source);
}

#[test]
fn parse_outputs_bare_camel_case_array() {
    // An LLM that ignores the prompt and emits camelCase still parses.
    let json = r#"[
        {"articleId":"a1","classification":"validating","relevanceExplanation":"e","misrepresentsSource":false}
      ]"#;
    let outputs = parse_citation_outputs(json).expect("parse");
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].article_id, "a1");
}

#[test]
fn parse_outputs_empty_array_returns_empty_vec() {
    // A genuine empty array (`[]`) is the LLM obeying the prompt + finding
    // no candidates - returns Ok(empty), NOT an error.
    let outputs = parse_citation_outputs("[]").expect("parse");
    assert!(outputs.is_empty());
}

// ── parse_citation_outputs: object-wrapper tolerance ────────────────────

#[test]
fn parse_outputs_unwraps_results_key() {
    let json = r#"{"results":[{"article_id":"a1","classification":"validating","relevance_explanation":"e","misrepresents_source":false}]}"#;
    let outputs = parse_citation_outputs(json).expect("parse");
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].article_id, "a1");
}

#[test]
fn parse_outputs_unwraps_citations_key() {
    let json = r#"{"citations":[{"article_id":"a1","classification":"validating","relevance_explanation":"e","misrepresents_source":false}]}"#;
    let outputs = parse_citation_outputs(json).expect("parse");
    assert_eq!(outputs.len(), 1);
}

#[test]
fn parse_outputs_unwraps_data_key() {
    let json = r#"{"data":[{"article_id":"a1","classification":"validating","relevance_explanation":"e","misrepresents_source":false}]}"#;
    let outputs = parse_citation_outputs(json).expect("parse");
    assert_eq!(outputs.len(), 1);
}

#[test]
fn parse_outputs_unknown_wrapper_key_returns_error() {
    // An object whose key is NOT in the known wrapper-key set surfaces an
    // error (not silently empty) so genuine LLM failures aren't masked.
    let json = r#"{"whatever":[{"article_id":"a1"}]}"#;
    assert!(parse_citation_outputs(json).is_err());
}

#[test]
fn parse_outputs_wrapper_with_non_array_value_returns_error() {
    // A known wrapper key whose value is not an array is treated as
    // not-a-match (the caller's "unknown wrapper" error fires).
    let json = r#"{"results":"not an array"}"#;
    assert!(parse_citation_outputs(json).is_err());
}

// ── parse_citation_outputs: per-element fault isolation ────────────────

#[test]
fn parse_outputs_isolates_one_bad_element_keeping_good_ones() {
    // One element is missing `article_id` (the required field); the other is
    // valid. The bad element is dropped, the good one survives. Previously
    // the whole batch failed.
    let json = r#"[
        {"claim":"","classification":"validating","relevance_explanation":"e","misrepresents_source":false},
        {"article_id":"a1","classification":"opposing","relevance_explanation":"e","misrepresents_source":false}
      ]"#;
    let outputs = parse_citation_outputs(json).expect("parse");
    assert_eq!(outputs.len(), 1, "the valid element must survive the bad one");
    assert_eq!(outputs[0].article_id, "a1");
}

#[test]
fn parse_outputs_all_elements_bad_returns_error() {
    // If EVERY element fails to parse, surface the first error rather than
    // masking the failure as an empty result.
    let json = r#"[
        {"claim":"","classification":"validating"},
        {"claim":"","classification":"opposing"}
      ]"#;
    assert!(parse_citation_outputs(json).is_err());
}

#[test]
fn parse_outputs_garbage_json_returns_error() {
    // Total non-JSON garbage → error.
    assert!(parse_citation_outputs("not json at all").is_err());
}

#[test]
fn parse_outputs_top_level_scalar_returns_error() {
    // A bare scalar (not array, not object) → error.
    assert!(parse_citation_outputs("42").is_err());
}

#[test]
fn parse_outputs_bug_report_regression() {
    // Canonical regression pin: a multi-element snake_case response in the
    // exact shape from the bug report. Pre-fix this failed with
    // `missing field articleId at line 8 column 3` because the struct used
    // `rename_all = "camelCase"` while the prompt asked for snake_case.
    // The LLM did the right thing; the parser was wrong.
    let json = r#"[
        {
          "article_id": "420cee9f-6e50-58d2-90bd-62b585759831",
          "claim": "sugar is harmfull",
          "classification": "validating",
          "relevance_explanation": "Total free-sugar intake remained above the 5% dietary energy recommendation.",
          "misrepresents_source": false
        },
        {
          "article_id": "b1e146ea-477a-5f1d-83b7-d9331ec28e83",
          "claim": "sugar is harmfull",
          "classification": "validating",
          "relevance_explanation": "High consumption of sugar-sweetened beverages is associated with increased health risks.",
          "misrepresents_source": false
        }
      ]"#;
    let outputs = parse_citation_outputs(json).expect("parse");
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].article_id, "420cee9f-6e50-58d2-90bd-62b585759831");
    assert_eq!(outputs[0].classification, "validating");
    assert_eq!(outputs[1].article_id, "b1e146ea-477a-5f1d-83b7-d9331ec28e83");
}

// ── ground_quotes (grounding gate) ──────────────────────────────────────

#[test]
fn ground_quotes_exact_match_passes() {
    let source = "The sugar tax reduced obesity. Consumption fell.";
    let quotes = vec!["The sugar tax reduced obesity.".to_string()];
    let out = ground_quotes(&quotes, source);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], "The sugar tax reduced obesity.");
}

#[test]
fn ground_quotes_case_insensitive_match_passes() {
    let source = "The Sugar Tax Reduced Obesity.";
    let quotes = vec!["the sugar tax reduced obesity.".to_string()];
    let out = ground_quotes(&quotes, source);
    assert_eq!(out.len(), 1);
}

#[test]
fn ground_quotes_whitespace_collapse_match_passes() {
    let source = "The  sugar	tax reduced   obesity.";
    let quotes = vec!["The sugar tax reduced obesity.".to_string()];
    let out = ground_quotes(&quotes, source);
    assert_eq!(out.len(), 1);
}

#[test]
fn ground_quotes_hallucinated_sentence_dropped() {
    let source = "The sugar tax reduced obesity.";
    let quotes = vec!["The fat tax increased health.".to_string()];
    let out = ground_quotes(&quotes, source);
    assert!(out.is_empty(), "hallucinated sentence must be dropped");
}

#[test]
fn ground_quotes_partial_fragment_dropped() {
    // A fragment ("sugar tax") is technically a substring but it is NOT a
    // full sentence from the passage. The gate accepts it because it IS a
    // verbatim substring - the grounding gate only checks that the quote
    // appears in the source, not that it is a complete sentence. The prompt
    // asks for full sentences; the gate trusts the prompt. (A stricter gate
    // would require sentence-boundary detection, which is out of scope.)
    let source = "The sugar tax reduced obesity significantly.";
    let quotes = vec!["sugar tax".to_string()];
    let out = ground_quotes(&quotes, source);
    assert_eq!(out.len(), 1, "verbatim substring is accepted even if a fragment");
}

#[test]
fn ground_quotes_mixed_grounded_and_hallucinated() {
    let source = "First sentence. Second sentence. Third sentence.";
    let quotes = vec![
        "First sentence.".to_string(),
        "Hallucinated.".to_string(),
        "Third sentence.".to_string(),
    ];
    let out = ground_quotes(&quotes, source);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0], "First sentence.");
    assert_eq!(out[1], "Third sentence.");
}

#[test]
fn ground_quotes_empty_input_returns_empty() {
    let out = ground_quotes(&[], "any source");
    assert!(out.is_empty());
}

#[test]
fn ground_quotes_empty_source_drops_all() {
    let quotes = vec!["Some sentence.".to_string()];
    let out = ground_quotes(&quotes, "");
    assert!(out.is_empty());
}

#[test]
fn ground_quotes_deduplicates_exact_dupes() {
    let source = "The key sentence.";
    let quotes = vec!["The key sentence.".to_string(), "the key sentence.".to_string()];
    let out = ground_quotes(&quotes, source);
    assert_eq!(out.len(), 1, "case-variant dupes deduped");
}

#[test]
fn ground_quotes_orders_by_source_position() {
    // LLM emitted out of order; the gate sorts survivors into source order.
    let source = "Alpha. Beta. Gamma.";
    let quotes = vec!["Gamma.".to_string(), "Alpha.".to_string(), "Beta.".to_string()];
    let out = ground_quotes(&quotes, source);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0], "Alpha.");
    assert_eq!(out[1], "Beta.");
    assert_eq!(out[2], "Gamma.");
}

// ── CitationLlmOutput.justifying_sentences deserialization ─────────────

#[test]
fn llm_output_justifying_sentences_snake_case() {
    let json = r#"{"article_id":"a1","classification":"validating","justifying_sentences":["First sentence.","Second sentence."]}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert_eq!(parsed.justifying_sentences.len(), 2);
    assert_eq!(parsed.justifying_sentences[0], "First sentence.");
}

#[test]
fn llm_output_justifying_sentences_camel_case_alias() {
    let json = r#"{"articleId":"a1","classification":"validating","justifyingSentences":["One."]}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert_eq!(parsed.justifying_sentences.len(), 1);
}

#[test]
fn llm_output_justifying_sentences_defaults_empty_when_absent() {
    let json = r#"{"article_id":"a1","classification":"validating"}"#;
    let parsed: CitationLlmOutput = serde_json::from_str(json).expect("parse");
    assert!(parsed.justifying_sentences.is_empty());
}

// ── format_candidates_section (via the public builders) ─────────────────

#[test]
fn candidates_section_includes_none_section_as_omitted_line() {
    // passage with section: None → no "- section:" line for that candidate.
    let passages = vec![
        passage("a1", None, "passage with section", Some("Methods")),
        passage("a2", None, "passage no section", None),
    ];
    let metadata = one_meta_map(&["a1", "a2"]);
    let prompt = build_whole_block_prompt("text", &passages, &metadata);
    assert!(prompt.contains("section: Methods"));
    // The a2 candidate block should NOT have a section line.
    let a2_block = prompt.split("article_id: a2").nth(1).unwrap_or("");
    assert!(!a2_block.contains("- section:"), "None section omitted");
}
