# Wiki Export Test Inventory (v3 two-step process)

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

| File | Test | Description |
|------|------|-------------|
| `src/__tests__/utils/wiki-markdown.test.ts` | `static_mode_emits_href_for_wikilink` | `staticMode` + `slugToHref` emits `href`, not `data-slug` |
| `src/__tests__/utils/wiki-markdown.test.ts` | `static_mode_emits_href_for_art_ref` | `[^art-id]` resolves to stub `href` |
| `src/__tests__/utils/wiki-markdown.test.ts` | `static_mode_renders_ref_missing_for_broken_link` | Missing slug -> `<span class="ref-missing">` |
| `src/__tests__/utils/wiki-markdown.test.ts` | `static_mode_handles_definition_lines` | `[^art-id]:` definition lines get static treatment |
| `src/__tests__/utils/wiki-markdown.test.ts` | `static_mode_depth_aware_links` | Page at depth 1 emits `../` relative hrefs |
| `src/__tests__/utils/wiki-site-export.test.ts` | `build_search_index_includes_all_pages` | Pure helper: index has one entry per page |
| `src/__tests__/utils/wiki-site-export.test.ts` | `render_article_stub_has_metadata_no_full_text` | Stub HTML has DOI/journal, no `full_text` leak |
| `src/__tests__/utils/wiki-site-export.test.ts` | `slug_to_href_is_depth_aware` | Resolver closure computes `../` correctly |
| `src/__tests__/utils/wiki-site-export.test.ts` | `wrapPageHtml_subpage_emits_correct_depth_prefix` | `../../style.css` + `.markdown-content` wrapper |
| `src/__tests__/utils/wiki-site-export.test.ts` | `pageDepth` | Index is 0, subpages are 2 |
| `src/__tests__/utils/wiki-site-export.test.ts` | `slugifyFilename` | Normalizes to kebab-case |
| `src-tauri/tests/wiki/wiki_export_test.rs` | `generate_export_writes_all_files` | Write bundle to `wiki-export/`, assert files exist |
| `src-tauri/tests/wiki/wiki_export_test.rs` | `markdown_tree_excludes_log_and_articles` | `log.md` + `raw/{id}.md` excluded; `wiki/**/*.md` present |
| `src-tauri/tests/wiki/wiki_export_test.rs` | `user_docs_markdown_included` | `source_kind: user_*` companion `.md` copied |
| `src-tauri/tests/wiki/wiki_export_test.rs` | `generate_export_clears_previous_output` | Old files gone on re-generation |
| `src/utils/wiki-site-export.ts` | `STATIC_SITE_CSS` has `color-scheme: light` | Export forces light mode; no `@media (prefers-color-scheme: dark)` |
| `scripts/verify-export-content.mjs` | end-to-end link integrity | All `href` resolve to files; no `data-slug`/`data-art-id` attrs; no dangling `[^...]` footnotes |

## Obsidian Vault Export inventory (binding)

Rows below use the machine-parseable `` `file::test` `` format enforced by
`scripts/check-test-inventory.sh`.

| Test | Assertion |
|------|-----------|
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::slug_map_builds_author_year_title_keys` | Slugs follow the BibTeX `{surname}{year}{title-word}` convention with matching aliases |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::slug_map_letter_suffix_dedup_and_fallbacks` | Colliding keys letter-suffix dedup (`b`); `anon`/`nd` fallbacks; ASCII folding |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::alias_format_multi_single_authorless` | `Surname et al. Year` / `Surname Year` / `Anon n.d.` alias shapes |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::rewrite_frontmatter_maps_uuids_drops_internal` | `id`/`slug`/`source_articles`/`links` remapped; `source_file`/`source_hash` dropped |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::rewrite_body_wikilinks_with_and_without_alias` | Wikilinks rewritten with default + custom alias; unmapped UUIDs dropped (alias kept) |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::rewrite_footnotes_rename_refs_and_definitions` | Footnote refs/definitions renamed; definitions become wikilinks; missing ones appended; unmapped stripped |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::rewrite_bare_raw_path_becomes_wikilink` | Bare `/raw/<uuid>.md` paths become wikilinks (unmapped dropped) |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::write_vault_renames_synthesis_and_excludes_internal` | Synthesis renamed; `log.md`/`index.md`/`raw/`/`templates/` excluded; `Home.md` + `.obsidian/` written |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::write_vault_orphaned_synthesis_fallback` | Orphaned synthesis renamed via frontmatter fallback; title-less ones skipped + counted |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::write_vault_cross_type_collision_gets_suffix` | Synthesis slug colliding with an existing page filename gets a letter suffix |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::write_vault_contains_no_uuids_anywhere` | No UUID-shaped token in any staged filename, frontmatter, or body (ruling 1 guard) |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::write_vault_leaves_user_doc_pages_untouched` | `[^art-user-*]` refs and user source pages pass through unrenamed |
| `src-tauri/tests/wiki/wiki_obsidian_export_test.rs::zip_entries_match_staging_tree` | Zip entry set matches the staged vault tree; entry content round-trips |
| `src/__tests__/composables/use-export.test.ts::export_obsidian_invokes_wiki_command_with_zip_filter` | `exportObsidian()` runs the save dialog with the zip filter + default path and invokes `wiki_export_obsidian` |
| `src/__tests__/composables/use-export.test.ts::export_obsidian_returns_false_when_dialog_cancelled` | Dialog cancel returns false without invoking the command |
| `src/__tests__/components/wiki-toolbar.test.ts::export_to_obsidian_item_invokes_save_dialog_and_command` | Actions-menu item triggers the save dialog + `wiki_export_obsidian` IPC |
| `src/__tests__/components/wiki-toolbar.test.ts::export_to_obsidian_item_disabled_when_wiki_not_initialized` | Actions-menu item follows the `!isInitialized()` disabled pattern |
