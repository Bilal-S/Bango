# Zotero Import + Export - Test Inventory

Binding test inventory for the Zotero local API import/export feature
(plan: Zotero Import + Export Plan v3). Enforced by
`scripts/check-test-inventory.sh` via `npm run check:all` (registered in
the `PLAN_DOCS` array).

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function/`it(` name. Any missing test blocks
the PR.

### Tier 1 - mapping + client + path resolution

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/zotero/zotero_mapping_test.rs::map_item_type_table_covers_scholarly_types` | Every scholarly Zotero itemType maps to a RIS reference type from the table |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::map_journal_article_to_ris_record` | Full `journalArticle` JSON maps title/abstract/journal/volume/issue/pages/ISSN/DOI |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::map_authors_prefer_author_creators` | `creatorType=author` names map to "Lastname, Firstname" |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::map_authors_fall_back_to_editors` | No authors but editors present -> editors used |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::map_parsed_date_to_year_and_date` | `meta.parsedDate` -> `publication_year` + `date` |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::map_pages_split_start_end` | "1-10" -> start 1 / end 10 |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::map_doi_normalized` | DOI passes through `normalize_doi` |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::map_tags_not_written_to_keywords` | Zotero tags do NOT land in `RisRecord.keywords` |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::unsupported_item_type_maps_to_none` | `interview`/`artwork` -> None (skipped) |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::attachment_and_note_item_types_skipped` | `attachment`/`note`/`annotation` -> None |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::sanitize_zotero_tag_lowercase_hyphenated` | "Machine Learning" -> "machine-learning" |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::sanitize_zotero_tag_strips_inclusion_prefix` | `inclusion:foo` -> `foo` |
| `src-tauri/tests/zotero/zotero_mapping_test.rs::sanitize_zotero_tag_truncates_to_35_chars` | Overlong tag truncates at word boundary |
| `src-tauri/tests/zotero/zotero_client_test.rs::resolve_attachment_path_unix_plain` | `file:///home/u/z/a.pdf` -> `/home/u/z/a.pdf` |
| `src-tauri/tests/zotero/zotero_client_test.rs::resolve_attachment_path_unix_percent_encoded` | `file:///home/u/z/a%20b.pdf` -> `/home/u/z/a b.pdf` |
| `src-tauri/tests/zotero/zotero_client_test.rs::resolve_attachment_path_unicode_filename` | Non-ASCII percent-encoded filename decodes |
| `src-tauri/tests/zotero/zotero_client_test.rs::resolve_attachment_path_windows_drive_letter` | `file:///C:/Users/u/z/a.pdf` -> `C:/Users/u/z/a.pdf` (runs on all platforms) |
| `src-tauri/tests/zotero/zotero_client_test.rs::resolve_attachment_path_windows_unc` | `file://server/share/a.pdf` -> `\\server\share\a.pdf` (runs on all platforms) |
| `src-tauri/tests/zotero/zotero_client_test.rs::resolve_attachment_path_non_file_scheme_rejected` | `http://` Location -> Err |
| `src-tauri/tests/zotero/zotero_client_test.rs::parse_collections_response` | Collections JSON parses to flat `{key, name, parentKey}` list; `parentCollection: false` -> null parent |
| `src-tauri/tests/zotero/zotero_client_test.rs::parse_items_response` | Items JSON parses to `ZoteroItem` vec incl. `meta.parsedDate` |
| `src-tauri/tests/zotero/zotero_client_test.rs::parse_attachment_list_response` | Bulk `itemType=attachment` JSON parses; children grouped by `data.parentItem` |

### Tier 2 - connection + collections + preview

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/zotero/zotero_connection_test.rs::check_connection_ok` | 200 probe -> status `ok`, `apiVersion` from response header |
| `src-tauri/tests/zotero/zotero_connection_test.rs::check_connection_not_running` | Connection refused/timeout -> status `not_running` |
| `src-tauri/tests/zotero/zotero_connection_test.rs::check_connection_api_disabled` | 403 -> status `api_disabled` with preference hint |
| `src-tauri/tests/zotero/zotero_connection_test.rs::check_connection_unexpected_status_falls_back_to_error` | 500 with body -> status `error` carrying status + snippet |
| `src-tauri/tests/zotero/zotero_connection_test.rs::get_collections_returns_flat_tree` | Collections list maps to flat tree with `parentKey` |
| `src-tauri/tests/zotero/zotero_connection_test.rs::get_collection_preview_counts_articles` | Preview returns mapped article count + attachment count from one bulk fetch |
| `src-tauri/tests/zotero/zotero_connection_test.rs::get_collection_preview_validates_like_ris` | Missing abstract -> validation error group (not importable) |
| `src-tauri/tests/zotero/zotero_connection_test.rs::get_collection_preview_skips_unsupported_types` | Unsupported item types surface as skipped, do not panic |
| `src-tauri/tests/zotero/zotero_connection_test.rs::get_collection_preview_returns_keys_and_version` | `articleKeys` aligns with `previewArticles`; `libraryVersion` captured from header |

### Tier 3 - import pipeline

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_inserts_articles` | Items insert with `import_source = "zotero"` + `'import'` audit entries |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_runs_classify` | Non-dupes reach `working`, dupes flagged duplicate |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_assigns_tags` | Sanitized Zotero tags linked via `article_tags` with source `ris_keyword` |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_respects_excluded_keys` | Deselected Zotero keys are not imported; unknown keys ignored |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_skips_library_duplicates` | Skip flag drops library-DOI records before insert (`skippedDuplicates` counted; keys stay aligned) |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_aborts_on_library_version_change` | Version mismatch -> error, nothing written |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_capacity_guard_surfaces` | Over-limit batch -> `AppError::Import`, nothing written (inherited guard) |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_attaches_pdf` | PDF child copies into fulltext dir with `{clean_doi}.pdf` naming + `has_full_text = 1` |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_attachment_failure_non_fatal` | Broken attachment path -> article still imported, audit error written |
| `src-tauri/tests/zotero/zotero_import_test.rs::import_zotero_collection_duplicate_skips_attachment` | Duplicate DOI item does not re-attach full text |

### Tier 4 - frontend

| Test identifier | Assertion |
|---|---|
| `src/__tests__/zotero-import-flow.test.ts::zotero_step_transitions_on_connection_ok` | ok status moves wizard to the collection step |
| `src/__tests__/zotero-import-flow.test.ts::zotero_step_shows_not_running_message` | not_running shows "Start Zotero" message + Retry |
| `src/__tests__/zotero-import-flow.test.ts::zotero_step_shows_api_disabled_message` | api_disabled shows preferences hint + Retry |
| `src/__tests__/zotero-import-flow.test.ts::zotero_step_shows_error_status_message` | error status shows backend message + Retry |
| `src/__tests__/zotero-import-flow.test.ts::confirm_import_maps_removed_indices_to_excluded_keys` | Confirm calls `import_zotero_collection` with key-based exclusions + preview version |
| `src/__tests__/zotero-import-flow.test.ts::zero_valid_records_renders_empty_state` | 0-importable preview shows distinct empty state, Back enabled |
| `src/__tests__/zotero-collection-picker.test.ts::renders_collection_tree` | Flat collections render as nested tree |
| `src/__tests__/zotero-collection-picker.test.ts::select_collection_fetches_preview` | Clicking a collection calls preview command and advances |
| `src/__tests__/zotero-collection-picker.test.ts::shows_error_state_on_fetch_failure` | Fetch failure renders error card with Retry |
| `src/__tests__/import-drop-zone.test.ts::zotero_button_emits_zotero_selected` | New button emits `zotero-selected` |
| `src/__tests__/zotero-import-flow.test.ts::zotero_confirm_passes_skip_duplicates` | Zotero confirm passes the review-step Skip flag (default on, resets per preview) |

### Tier 5 - backend export

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::reverse_type_table_maps_ris_to_item_types` | Every RIS TY from the import table maps back to a Zotero itemType; unknown/None -> `journalArticle` |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::map_article_to_journal_item_json` | Full article -> item JSON with the `collections` array + all journal fields |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::map_creators_split_lastname_firstname` | "Doe, Jane" -> `{firstName:"Jane", lastName:"Doe", creatorType:"author"}` |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::map_creators_single_token_uses_name` | Institutional single-token author -> `{name}` creator |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::map_creators_drop_malformed_entries` | Empty parts ("", ", ") produce no creator |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::map_pages_join_start_end` | start 1 / end 10 -> `pages: "1-10"` |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::map_non_journal_types_drop_invalid_fields` | `book` drops `publicationTitle`/`ISSN` |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::map_tags_and_keywords_merge_deduped` | tags + keywords -> one case-insensitively deduped Zotero tags array |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::map_notes_to_extra_user_notes_excluded` | `notes` -> `extra`; `user_notes` absent |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::build_attachment_title_lastname_and_word_boundary_truncation` | First author's last name + title cut at the last word boundary within 30 chars + extension |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::build_attachment_title_single_token_author_uses_whole_name` | Institutional single-token author used verbatim |
| `src-tauri/tests/zotero/zotero_export_mapping_test.rs::build_attachment_title_no_author_or_title_fallbacks` | No author -> title only; blank title -> Untitled; >30-char single word hard-cuts |
| `src-tauri/tests/zotero/zotero_export_test.rs::diff_by_canonical_doi_classifies_articles` | missing / already-present / no-DOI incl. prefix + case differences |
| `src-tauri/tests/zotero/zotero_export_test.rs::diff_treats_placeholder_dois_as_no_doi` | `NA`/`-` DOIs land in the no-DOI bucket |
| `src-tauri/tests/zotero/zotero_export_test.rs::preview_counts_match_diff` | Preview numbers equal the pure diff on fixture JSON |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::parse_write_envelope` | `successful`/`success`/`unchanged`/`failed` envelope parses; item keys extracted |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::write_auth_error_classification` | 401 -> KeyRequired(+authorize hint), 403 denied, 429 rate-limited, 428 server-id, 501 needs-10 |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::authorize_response_parses` | `{"key","remember"}` body parse; remember flag surfaced |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::build_attachment_item_json` | `imported_file` child with parentItem + title + contentType by extension |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::build_upload_params` | md5/filename/filesize/mtime + `If-None-Match: *` |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::build_upload_auth_body_percent_encodes_spaces` | Phase-1 body sends spaces as %20 (never `+`); %, &, =, + escaped |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::attachment_item_creation_surfaces_envelope_failures` | No-key envelope error carries the `failed` map reasons |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::ordered_attachment_body_puts_link_mode_before_path_fields` | Body serializes linkMode before filename/contentType; round-trips the field set |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::upload_authorization_response_branches` | `{url, uploadKey}` vs `{"exists":1}` branches |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::batches_chunked_at_50_with_fresh_tokens` | Batch splitting + a unique `Zotero-Write-Token` per batch |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::decide_write_auth_reuses_stored_key` | Stored key + matching server id -> UseStored; no authorize call |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::decide_write_auth_requires_authorize_when` | Missing key or server-id mismatch -> Authorize |
| `src-tauri/tests/zotero/zotero_write_client_test.rs::key_expired_mid_run_aborts_with_guidance` | Second write 401 -> typed `KeyExpired` error, run aborted, stored key cleared |

### Tier 6 - frontend export

| Test identifier | Assertion |
|---|---|
| `src/__tests__/components/export-dialog-zotero.test.ts::zotero_button_visible_in_tab_context` | Button renders beside the tab RIS export |
| `src/__tests__/components/export-dialog-zotero.test.ts::zotero_button_visible_in_prisma_context` | Button renders in PRISMA (included) mode |
| `src/__tests__/components/export-dialog-zotero.test.ts::zotero_button_hidden_when_tab_empty` | 0-article tab hides the button like RIS |
| `src/__tests__/components/zotero-export-panel.test.ts::opens_panel_loads_collections` | Mounting fetches collections + the selected default |
| `src/__tests__/components/zotero-export-panel.test.ts::dropdown_defaults_to_zotero_selection` | Connector name match preselects the collection |
| `src/__tests__/components/zotero-export-panel.test.ts::dropdown_falls_back_to_last_used` | No/ambiguous match -> `zotero_last_collection_key` |
| `src/__tests__/components/zotero-export-panel.test.ts::api_disabled_shows_enable_instructions` | 403 state renders the preference-path card |
| `src/__tests__/components/zotero-export-panel.test.ts::communication_error_shows_enable_hint` | Any error message carries the enable-API hint |
| `src/__tests__/components/zotero-export-panel.test.ts::older_zotero_shows_version_gate` | < 10 renders "requires Zotero 10 or newer", Export disabled |
| `src/__tests__/components/zotero-export-panel.test.ts::shows_sync_summary_counts` | Preview counts render before export |
| `src/__tests__/components/zotero-export-panel.test.ts::export_invokes_command_with_scope` | Confirm calls `export_zotero_collection` with status + includeFiles |
| `src/__tests__/components/zotero-export-panel.test.ts::authorize_state_prompts_remember` | No stored key -> authorize phase + "tick Remember" copy |
| `src/__tests__/components/zotero-export-panel.test.ts::progress_events_update_bar` | `zotero-export:progress` phases drive the bar |
| `src/__tests__/components/zotero-export-panel.test.ts::result_summary_rendered` | Exported/already/no-DOI/file counts render |
| `src/__tests__/components/zotero-export-panel.test.ts::button_becomes_close_after_completion` | Result state renames the primary button to Close; click emits close |

