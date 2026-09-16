# Wiki Author Enrichment Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

Covers the bibliometric author-page enrichment (co-authors, most-cited, key
references, curated keywords, main themes) in `src-tauri/src/wiki/ingest/authors.rs`.

| File | Test | Description |
|------|------|-------------|
| `src-tauri/tests/wiki/wiki_author_enrichment_test.rs::manifest_build_includes_coauthors_from_biblio_db` | `manifest_build_includes_coauthors_from_biblio_db` | Co-authors come back from the biblio DB ranked by shared papers and render as wikilinks (regression for the swallowed parameter-count bug) |
| `src-tauri/tests/wiki/wiki_author_enrichment_test.rs::manifest_most_cited_uses_num_cited_ranking` | `manifest_most_cited_uses_num_cited_ranking` | Most Cited section ranks the author's own papers by `articles.num_cited`, skipping zero-cited ones |
| `src-tauri/tests/wiki/wiki_author_enrichment_test.rs::manifest_key_references_ranks_reference_papers` | `manifest_key_references_ranks_reference_papers` | Key References ranks external `reference_papers` by usage across the author's articles (type=1 links) |
| `src-tauri/tests/wiki/wiki_author_enrichment_test.rs::manifest_main_themes_link_to_concept_hubs` | `manifest_main_themes_link_to_concept_hubs` | Main Themes link to seeded concept hub pages derived from the author's user-curated tags |
| `src-tauri/tests/wiki/wiki_author_enrichment_test.rs::keyword_curation_filters_generic_terms` | `keyword_curation_filters_generic_terms` | Research Areas drop blocklisted filler terms ("upon", "significant", "years") and keep real ones |
