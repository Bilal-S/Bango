//! Tests for the Search Strategy Builder pure helpers (spec §8.4).
//!
//! Binding inventory: `docs/test-plans/search-strategy-tests.md` (enforced by
//! `scripts/check-test-inventory.sh` via `npm run check:all`).
//!
//! Per `docs/CLAUDE.md` §Testing, the non-trivial logic is extracted as `pub fn`s
//! so these tests can exercise them without `State<DbState>`.

use bango_lib::commands::search_strategy::{
    build_search_strategy_prompt, parse_search_strategy_response,
};
use bango_lib::models::criterion::{Criterion, CriterionType, Priority, ResearchAim};

/// Build a minimal aim for tests.
fn aim(text: &str) -> ResearchAim {
    ResearchAim {
        id: format!("aim-{text}").replace(' ', "-"),
        text: text.to_string(),
        created_at: "2026-07-07T00:00:00Z".to_string(),
    }
}

/// Build a minimal criterion for tests.
fn criterion(text: &str, criterion_type: CriterionType, priority: Priority) -> Criterion {
    Criterion {
        id: format!("crit-{text}").replace(' ', "-"),
        criterion_type,
        text: text.to_string(),
        priority,
        created_at: "2026-07-07T00:00:00Z".to_string(),
    }
}

/// A complete valid JSON fixture with all 8 database fields.
const VALID_FIXTURE: &str = r#"{
  "picoBreakdown": {
    "population": { "concept": "children", "synonyms": ["adolescents", "youth", "pediatric"] },
    "intervention": { "concept": "sugar tax", "synonyms": ["soda tax", "SSB tax", "sweetened beverage levy"] },
    "comparison": null,
    "outcome": { "concept": "obesity", "synonyms": ["overweight", "BMI", "adiposity"] }
  },
  "strategies": {
    "pubmed": { "oneLine": "(\"sugar tax\"[tiab] OR \"soda tax\"[tiab]) AND (obesity[tiab])", "notes": "Uses [tiab] for title+abstract." },
    "scopus": { "oneLine": "TITLE-ABS-KEY(\"sugar tax\" OR \"soda tax\") AND TITLE-ABS-KEY(obesity)", "notes": "TITLE-ABS-KEY default." },
    "webOfScience": { "oneLine": "TS=(\"sugar tax\" OR \"soda tax\") AND TS=(obesity)", "notes": "TS topic field." },
    "cochrane": { "oneLine": "(\"sugar tax\" OR \"soda tax\"):ti,ab,kw AND (obesity):ti,ab,kw", "notes": ":ti,ab,kw default." },
    "ebscohost": { "oneLine": "TI (\"sugar tax\" OR \"soda tax\") AND AB (obesity)", "notes": "Two-char field codes." },
    "jstor": { "oneLine": "ti:\"sugar tax\" OR ti:\"soda tax\" AND ab:obesity", "notes": "Colon field codes." },
    "sciencedirect": { "oneLine": "TITLE-ABS-KEY(\"sugar tax\" OR \"soda tax\") AND TITLE-ABS-KEY(obesity)", "notes": "Same family as Scopus." },
    "arxiv": { "oneLine": "(ti:\"sugar tax\" OR abs:\"soda tax\") ANDNOT cat:cs.CL", "notes": "ANDNOT not plain NOT." }
  },
  "warnings": [
    { "warningType": "non_boolean_database", "message": "Semantic Scholar: search plain key terms (no Boolean support)." }
  ]
}"#;

#[test]
fn build_prompt_includes_aims_text() {
    let aims = vec![aim("Evaluate the impact of sugar taxes on obesity.")];
    let (system, user) = build_search_strategy_prompt(&aims, &[], &[]);
    // System prompt is non-empty and role-bearing.
    assert!(!system.is_empty());
    assert!(system.contains("search strategist"));
    // User prompt embeds the aim text verbatim.
    assert!(user.contains("Evaluate the impact of sugar taxes on obesity."));
}

#[test]
fn build_prompt_includes_all_eight_databases() {
    let aims = vec![aim("Test aim.")];
    let (system, _user) = build_search_strategy_prompt(&aims, &[], &[]);
    // The system prompt's cheatsheet names all 8 Boolean databases.
    for db in [
        "PubMed",
        "Scopus",
        "Web of Science",
        "Cochrane Library",
        "EBSCOhost",
        "JSTOR",
        "ScienceDirect",
        "arXiv",
    ] {
        assert!(system.contains(db), "system prompt missing database: {db}");
    }
}

#[test]
fn build_prompt_includes_arxiv_andnot() {
    let aims = vec![aim("Test aim.")];
    let (system, _user) = build_search_strategy_prompt(&aims, &[], &[]);
    // The arXiv section of the cheatsheet must call out ANDNOT (not plain NOT).
    assert!(system.contains("ANDNOT"), "system prompt missing arXiv ANDNOT guidance");
}

#[test]
fn build_prompt_includes_semantic_scholar_advisory() {
    let aims = vec![aim("Test aim.")];
    let (system, user) = build_search_strategy_prompt(&aims, &[], &[]);
    // The system prompt warns that Semantic Scholar is non-Boolean.
    assert!(system.contains("Semantic Scholar"), "system prompt missing Semantic Scholar advisory");
    // The user prompt also instructs the model to emit the warning.
    assert!(user.contains("Semantic Scholar"), "user prompt missing Semantic Scholar instruction");
}

#[test]
fn build_prompt_handles_empty_criteria() {
    // Aims-only is a valid input (criteria enrich the prompt but are optional).
    let aims = vec![aim("Only an aim, no criteria.")];
    let (system, user) = build_search_strategy_prompt(&aims, &[], &[]);
    assert!(!system.is_empty());
    // Both criteria sections render the "None defined." placeholder.
    assert!(user.contains("## Inclusion Criteria\nNone defined."));
    assert!(user.contains("## Exclusion Criteria\nNone defined."));
}

#[test]
fn build_prompt_includes_criteria_when_present() {
    let aims = vec![aim("Aim.")];
    let inclusion = vec![criterion("RCTs only", CriterionType::Inclusion, Priority::High)];
    let exclusion =
        vec![criterion("Non-English studies", CriterionType::Exclusion, Priority::Standard)];
    let (_system, user) = build_search_strategy_prompt(&aims, &inclusion, &exclusion);
    // The criterion texts appear, with their priority tags.
    assert!(user.contains("[high] RCTs only"), "inclusion criterion missing");
    assert!(user.contains("[standard] Non-English studies"), "exclusion criterion missing");
}

#[test]
fn parse_response_parses_valid_eight_database_fixture() {
    let parsed = parse_search_strategy_response(VALID_FIXTURE).expect("valid fixture must parse");
    // PICO breakdown.
    assert_eq!(parsed.pico_breakdown.population.as_ref().expect("population").concept, "children");
    assert_eq!(
        parsed.pico_breakdown.intervention.as_ref().expect("intervention").concept,
        "sugar tax"
    );
    // Comparison is null in the fixture.
    assert!(parsed.pico_breakdown.comparison.is_none());
    assert_eq!(parsed.pico_breakdown.outcome.as_ref().expect("outcome").concept, "obesity");
    // All 8 database fields present + non-empty oneLine.
    assert!(!parsed.strategies.pubmed.one_line.is_empty());
    assert!(!parsed.strategies.scopus.one_line.is_empty());
    assert!(!parsed.strategies.web_of_science.one_line.is_empty());
    assert!(!parsed.strategies.cochrane.one_line.is_empty());
    assert!(!parsed.strategies.ebscohost.one_line.is_empty());
    assert!(!parsed.strategies.jstor.one_line.is_empty());
    assert!(!parsed.strategies.sciencedirect.one_line.is_empty());
    assert!(!parsed.strategies.arxiv.one_line.is_empty());
    // Warnings array carries the Semantic Scholar advisory.
    assert_eq!(parsed.warnings.len(), 1);
    assert_eq!(parsed.warnings[0].warning_type, "non_boolean_database");
}

#[test]
fn parse_response_returns_error_on_malformed_json() {
    let result = parse_response_returns_error_on_malformed_json_inner();
    assert!(result.is_err(), "malformed JSON must yield an error, not a panic");
}

/// Helper so the assertion can capture the result type without the test fn
/// returning a Result (which the inventory greps for by name).
fn parse_response_returns_error_on_malformed_json_inner(
) -> Result<bango_lib::models::search_strategy::SearchStrategyResult, bango_lib::error::AppError> {
    parse_search_strategy_response("{ this is not valid json")
}

#[test]
fn parse_response_returns_error_on_missing_database_field() {
    // A fixture missing the `arxiv` field should fail deserialization
    // (all 8 databases are required by `StrategiesByDatabase`).
    let missing_field = r#"{
      "picoBreakdown": {},
      "strategies": {
        "pubmed": { "oneLine": "x", "notes": "" },
        "scopus": { "oneLine": "x", "notes": "" },
        "webOfScience": { "oneLine": "x", "notes": "" },
        "cochrane": { "oneLine": "x", "notes": "" },
        "ebscohost": { "oneLine": "x", "notes": "" },
        "jstor": { "oneLine": "x", "notes": "" },
        "sciencedirect": { "oneLine": "x", "notes": "" }
      },
      "warnings": []
    }"#;
    let result = parse_search_strategy_response(missing_field);
    assert!(result.is_err(), "missing database field must yield an error");
}

#[test]
fn parse_response_tolerates_code_fences() {
    // After the `send_json` migration, fence-stripping moved upstream into
    // `prepare_llm_json` (orchestrator layer). `parse_search_strategy_response`
    // now receives already-cleaned JSON. To keep this test meaningful under
    // the new architecture, we simulate the post-`prepare_llm_json` input by
    // stripping the fences the same way `prepare_llm_json` does, then assert
    // the parse fn handles the cleaned payload. This documents the contract
    // split: the parse fn does NOT strip fences itself; it trusts its caller.
    let fenced = format!("```json\n{VALID_FIXTURE}\n```");
    let cleaned = bango_lib::utils::json_repair::prepare_llm_json(&fenced);
    assert!(
        !cleaned.starts_with("```"),
        "prepare_llm_json must strip fences before parse_search_strategy_response sees the input"
    );
    let parsed = parse_search_strategy_response(&cleaned)
        .expect("cleaned fixture must parse identically to raw");
    assert_eq!(parsed.pico_breakdown.population.as_ref().expect("population").concept, "children");
    assert!(!parsed.strategies.arxiv.one_line.is_empty());
}
