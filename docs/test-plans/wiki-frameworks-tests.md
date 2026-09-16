# Wiki Frameworks Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

Covers the frameworks pipeline (`src-tauri/src/wiki/ingest/frameworks.rs` +
blob schema + canonicalization + pre-seed/polish + article links).

| File | Test | Description |
|------|------|-------------|
| `src-tauri/tests/wiki/wiki_frameworks_test.rs::frameworks_preseed_groups_articles_by_canonical_name` | `frameworks_preseed_groups_articles_by_canonical_name` | Variant spellings cluster into ONE framework page listing every naming article |
| `src-tauri/tests/wiki/wiki_frameworks_test.rs::framework_synthesis_writes_polished_page_with_publications_section` | `framework_synthesis_writes_polished_page_with_publications_section` | LLM body lands while the deterministic Publications section stays complete |
| `src-tauri/tests/wiki/wiki_frameworks_test.rs::framework_synthesis_falls_back_to_skeleton_on_failure` | `framework_synthesis_falls_back_to_skeleton_on_failure` | Failing synthesizer still writes the deterministic skeleton page (non-fatal) |
| `src-tauri/tests/wiki/wiki_frameworks_test.rs::backfill_query_targets_articles_missing_framework_field` | `backfill_query_targets_articles_missing_framework_field` | Backfill query targets only included full-text articles whose blob lacks the field |
| `src-tauri/tests/wiki/wiki_frameworks_test.rs::framework_extraction_schema_captures_usage_notes` | `framework_extraction_schema_captures_usage_notes` | Parser accepts `{name, usage}` + plain strings; merge preserves other blob keys and stores empty arrays |
| `src-tauri/tests/wiki/wiki_frameworks_test.rs::framework_alias_merges_unify_acronym_variants` | `framework_alias_merges_unify_acronym_variants` | Deterministic slug clustering + `apply_alias_merges` unify acronym/full-title variants |
| `src-tauri/tests/wiki/wiki_frameworks_test.rs::synthesis_preseed_links_canonical_frameworks` | `synthesis_preseed_links_canonical_frameworks` | Per-article synthesis pages carry `[[framework-slug\|name]]` links |
| `src-tauri/tests/wiki/wiki_summary_export_test.rs::render_summary_blob_includes_theoretical_frameworks` | `render_summary_blob_includes_theoretical_frameworks` | Raw export renders frameworks + usage notes from the blob |
