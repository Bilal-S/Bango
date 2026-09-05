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
