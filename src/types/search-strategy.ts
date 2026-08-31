/* Search Strategy Builder types. Session-scoped (Pinia store), not persisted.
 * Spec: docs/bango-v5-spec.md §8.4. */

export interface SearchStrategyResult {
  picoBreakdown: PicoBreakdown;
  strategies: StrategiesByDatabase;
  warnings: StrategyWarning[];
}

export interface PicoBreakdown {
  population: ConceptBlock | null;
  intervention: ConceptBlock | null;
  comparison: ConceptBlock | null;
  outcome: ConceptBlock | null;
}

export interface ConceptBlock {
  concept: string;
  synonyms: string[];
}

export interface StrategiesByDatabase {
  pubmed: DatabaseStrategy;
  scopus: DatabaseStrategy;
  webOfScience: DatabaseStrategy;
  cochrane: DatabaseStrategy;
  ebscohost: DatabaseStrategy;
  jstor: DatabaseStrategy;
  sciencedirect: DatabaseStrategy;
  arxiv: DatabaseStrategy;
}

export interface DatabaseStrategy {
  oneLine: string;
  notes: string;
}

export interface StrategyWarning {
  /** Free-form category tag (e.g., `non_boolean_database`, `missing_concept`,
   * `sensitivity_concern`). Not enum-typed so the LLM can introduce new
   * categories without a schema change. */
  warningType: string;
  message: string;
}
