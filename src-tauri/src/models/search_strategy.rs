//! Response types for the Search Strategy Builder (spec §8.4).
//!
//! Produced by `commands::search_strategy::suggest_search_strategy` and
//! consumed by the frontend `src/components/search-strategy-card.vue`. The
//! result is session-scoped (Pinia store), NOT persisted to the DB.

use serde::{Deserialize, Serialize};

/// Top-level result: PICO breakdown + one Boolean strategy per supported
/// database + advisory warnings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStrategyResult {
    pub pico_breakdown: PicoBreakdown,
    pub strategies: StrategiesByDatabase,
    #[serde(default)]
    pub warnings: Vec<StrategyWarning>,
}

/// PICO concept decomposition of the research aims/criteria. Each arm is
/// optional; the LLM may omit arms that do not apply (e.g., observational
/// reviews often have no `Comparison`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PicoBreakdown {
    #[serde(default)]
    pub population: Option<ConceptBlock>,
    #[serde(default)]
    pub intervention: Option<ConceptBlock>,
    #[serde(default)]
    pub comparison: Option<ConceptBlock>,
    #[serde(default)]
    pub outcome: Option<ConceptBlock>,
}

/// One PICO concept: a canonical name plus 3-8 synonyms/variants the LLM
/// surfaced for that concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConceptBlock {
    pub concept: String,
    #[serde(default)]
    pub synonyms: Vec<String>,
}

/// One Boolean strategy string per supported database (8 total). All eight
/// fields are populated by the LLM in a single response. Semantic Scholar is
/// intentionally absent because it does not support Boolean operators.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategiesByDatabase {
    pub pubmed: DatabaseStrategy,
    pub scopus: DatabaseStrategy,
    pub web_of_science: DatabaseStrategy,
    pub cochrane: DatabaseStrategy,
    pub ebscohost: DatabaseStrategy,
    pub jstor: DatabaseStrategy,
    pub sciencedirect: DatabaseStrategy,
    pub arxiv: DatabaseStrategy,
}

/// A single database's Boolean query string plus a short note explaining
/// any database-specific choices (field codes, proximity operators, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStrategy {
    pub one_line: String,
    #[serde(default)]
    pub notes: String,
}

/// A advisory warning surfaced to the user (e.g., a missing PICO concept, a
/// sensitivity concern, or the Semantic Scholar non-Boolean advisory).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyWarning {
    /// Free-form category tag. Common values: `sensitivity_concern`,
    /// `missing_concept`, `non_boolean_database`. Not enum-typed so the LLM
    /// can introduce new categories without a schema change.
    pub warning_type: String,
    pub message: String,
}
