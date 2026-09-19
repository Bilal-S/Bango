# Local Embeddings - Binding Test Inventory

Plan: `.worktrees/embedplanNew.md` (T2 + T3 + review-fix + T4 scopes; later
tiers append rows as their tests land).
Enforced by `scripts/check-test-inventory.sh` via `npm run check:all`.

Note: rows tagged `#[ignore = "slow"]` (the T4 live spike) count as present
under the repo's slow-test convention - they run via `npm run test:rust:full`
or explicitly with `-- --ignored` plus their documented env-var artifacts.

## T2 - Pure foundations (`embedding/local/`), backend setting, service router

| Test | Assertion |
|---|---|
| `src-tauri/src/embedding/local/paths.rs::onedrive_personal_segment_detected` | Personal `OneDrive` path segment is flagged |
| `src-tauri/src/embedding/local/paths.rs::onedrive_business_segment_detected` | `OneDrive - Contoso` business segment is flagged |
| `src-tauri/src/embedding/local/paths.rs::onedrive_detection_is_case_insensitive` | Detection ignores case |
| `src-tauri/src/embedding/local/paths.rs::plain_documents_path_not_flagged` | Plain Documents paths are not flagged |
| `src-tauri/src/embedding/local/paths.rs::resolve_normal_root_uses_storage_model_dir` | Model root is `{storage_root}/model` without OneDrive |
| `src-tauri/src/embedding/local/paths.rs::resolve_onedrive_root_falls_back_to_app_data` | Model root falls back to app data under OneDrive |
| `src-tauri/src/embedding/local/paths.rs::resolve_runtime_always_app_data` | Runtime root always derives from app data |
| `src-tauri/src/embedding/local/paths.rs::resolve_missing_app_data_base_keeps_storage_root` | Missing app-data base degrades to the storage root, no fallback flag |
| `src-tauri/src/embedding/local/thread_budget.rs::budget_caps_at_four_threads` | Budget never exceeds 4 intra-op threads |
| `src-tauri/src/embedding/local/thread_budget.rs::budget_reserves_two_cores` | Two cores stay reserved for UI/runtime/DB on large machines |
| `src-tauri/src/embedding/local/thread_budget.rs::budget_small_machine_floor_one` | 1-3 core machines still receive at least 1 thread |
| `src-tauri/src/embedding/local/prompt.rs::query_prefix_precedes_text` | Query role applies the `task: search result` prefix before the text |
| `src-tauri/src/embedding/local/prompt.rs::document_prefix_precedes_text` | Document role applies the `title: none` prefix before the text |
| `src-tauri/src/embedding/local/prompt.rs::query_and_document_prefixes_differ` | The two role prefixes are distinct strings |
| `src-tauri/src/embedding/local/state.rs::state_not_installed_when_model_root_missing` | Absent model root reports `NotInstalled` |
| `src-tauri/src/embedding/local/state.rs::state_not_installed_without_manifest` | Model dir without a profile manifest reports `NotInstalled` |
| `src-tauri/src/embedding/local/state.rs::state_ready_when_profile_manifest_present` | Profile dir with `manifest.json` reports `Ready` |
| `src-tauri/src/embedding/local/state.rs::state_ignores_unrelated_and_staging_dirs` | Foreign profile dirs and `.staging` never report `Ready` |
| `src-tauri/src/embedding/local/state.rs::assess_ready_for_complete_install` | Manifest + all files at pinned sizes report `Ready` |
| `src-tauri/src/embedding/local/state.rs::assess_reports_repair_required_for_missing_file` | A missing installed file reports `RepairRequired` |
| `src-tauri/src/embedding/local/state.rs::assess_reports_repair_required_for_short_file` | A truncated installed file reports `RepairRequired` |
| `src-tauri/src/embedding/local/state.rs::assess_reports_repair_required_for_profile_mismatch` | An installed manifest recording a foreign profile reports `RepairRequired` |
| `src-tauri/src/embedding/local/state.rs::assess_reports_repair_required_for_stranded_replaced_install` | A crashed promote's parked `.staging/replaced-*` reports `RepairRequired` |
| `src-tauri/src/embedding/local/state.rs::assess_not_installed_when_empty` | An empty model root reports `NotInstalled` |
| `src-tauri/src/embedding/local/state.rs::not_ready_phrase_is_human_text_for_every_state` | Not-ready states carry a human phrase, never a Debug enum name |
| `src-tauri/tests/embedding/embedding_backend_setting_test.rs::backend_defaults_to_configured_provider` | Fresh DB reads the cloud backend |
| `src-tauri/tests/embedding/embedding_backend_setting_test.rs::backend_round_trips_bango_local` | Set/get round-trips the local backend |
| `src-tauri/tests/embedding/embedding_backend_setting_test.rs::backend_unknown_value_falls_back_to_default` | Garbage stored values fall back to the cloud backend |
| `src-tauri/tests/embedding/embedding_backend_setting_test.rs::backend_key_is_machine_local` | The key is excluded from `PROJECT_PORTABLE_SETTINGS` |
| `src-tauri/tests/embedding/embedding_service_test.rs::service_cloud_query_routes_through_orchestrator` | Cloud backend + Query role reaches the provider endpoint and parses vectors |
| `src-tauri/tests/embedding/embedding_service_test.rs::service_cloud_documents_route_through_orchestrator` | Cloud backend + Document role reaches the same endpoint |
| `src-tauri/tests/embedding/embedding_service_test.rs::service_local_not_installed_returns_actionable_error` | Local backend without installation errors naming Settings |
| `src-tauri/tests/embedding/embedding_service_test.rs::service_local_installed_reports_pending_engine` | An incomplete install errors via the engine's health gate (points at Settings) |
| `src-tauri/tests/embedding/embedding_service_test.rs::service_local_missing_storage_root_reports_not_installed` | Empty storage root (side-effect-free read found nothing) reads as `NotInstalled` |

## T3 - Component manager (pinned manifest + atomic downloader + verify/remove)

| Test | Assertion |
|---|---|
| `src-tauri/src/embedding/local/manifest.rs::local_manifest_parses_and_validates` | Embedded pinned manifest parses, matches the profile id, and hashes the dominant payloads |
| `src-tauri/src/embedding/local/manifest.rs::parse_manifest_rejects_profile_drift` | A manifest whose profile differs from `LOCAL_PROFILE_ID` is rejected |
| `src-tauri/src/embedding/local/manifest.rs::manifest_rejects_malformed_sha256` | Short, uppercase, and 63-char hashes are rejected |
| `src-tauri/src/embedding/local/manifest.rs::manifest_rejects_malformed_source_revision` | Non-hex and empty source revisions are rejected |
| `src-tauri/src/embedding/local/manifest.rs::manifest_rejects_non_https_external_url` | Plain-HTTP external URLs rejected; loopback HTTP allowed only at a host boundary |
| `src-tauri/src/embedding/local/manifest.rs::manifest_required_bytes_cover_download_plus_margin` | Required disk bytes exceed the download total within a bounded margin |
| `src-tauri/src/embedding/local/manifest.rs::manifest_required_bytes_double_for_repair` | Repair budgets a full second staged copy |
| `src-tauri/src/embedding/local/download.rs::target_supported_matrix` | Only win-x64, osx-arm64, linux-x64 pass the gate (osx-x86_64 dropped: no runtime builds) |
| `src-tauri/src/embedding/local/download.rs::part_extension_appends_to_file_name` | Part files carry the `.part` suffix |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_install_downloads_and_verifies` | Install lands pinned files + manifest and reports Ready |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_install_rejects_bad_hash` | Hash mismatch aborts with no partial install |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_install_is_idempotent_without_refetch` | Repair skips pinned files; each file fetched exactly once |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_install_cancel_aborts` | Cancellation leaves NotInstalled |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_verify_detects_corruption` | Tampered files are reported by full verification |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_verify_flags_profile_mismatch` | An installed manifest with a foreign profile revision is reported |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_install_resumes_partial_download` | A partial `.part` continues via `Range`/206 and completes the pinned file |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_promote_restores_parked_install_on_failure` | A failed promote restores the parked working install |
| `src-tauri/tests/embedding/embedding_component_test.rs::component_remove_deletes_artifacts` | Remove deletes artifacts (all candidate roots) and is idempotent |

## T5 - Runtime component + engine

| Test | Assertion |
|---|---|
| `src-tauri/src/embedding/local/manifest.rs::manifest_target_id_matrix` | (os, arch) -> target id mapping incl. unsupported combos |
| `src-tauri/src/embedding/local/manifest.rs::manifest_runtime_validation_rejects_bad_entries` | Runtime entries reject bad hashes, duplicate targets, unsafe libPaths |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_extract_tgz_pulls_only_pinned_member` | tar.gz extraction pulls only the pinned library member |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_extract_zip_pulls_only_pinned_member` | zip extraction pulls only the pinned library member |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_extract_rejects_unsafe_member_paths` | Traversal member paths rejected; missing members error |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_install_skips_when_library_exists` | Existing healthy library short-circuits before any HTTP (integrity-aware) |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_install_missing_target_archive_errors` | No archive for the current target -> actionable unsupported error |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_install_repairs_size_mismatch` | A truncated library is re-downloaded + re-extracted; staging cleaned |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_extract_matches_dot_prefixed_members` | macOS `./`-prefixed tar members match the pinned path |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_verify_reports_missing_and_corrupt_library` | `verify_runtime` reports missing + wrong-size libraries, passes healthy |
| `src-tauri/src/embedding/local/engine.rs::engine_validate_vectors_accepts_wellformed_output` | Vector validation passes well-formed output (incl. empty batch) |
| `src-tauri/src/embedding/local/engine.rs::engine_validate_vectors_rejects_wrong_count_and_dims` | Wrong vector count + wrong dimensions rejected |
| `src-tauri/src/embedding/local/engine.rs::engine_resolve_dylib_env_override_wins` | `ORT_DYLIB_PATH` override wins; whitespace-only override is ignored |
| `src-tauri/src/embedding/local/engine.rs::engine_resolve_dylib_requires_healthy_install` | Missing/truncated runtime -> actionable error; pinned-size library resolves |
| `src-tauri/src/embedding/local/engine.rs::engine_reset_off_thread_without_a_session_is_a_noop` | Empty-session off-thread reset is a fast Ok; no session appears |
| `src-tauri/tests/embedding/embedding_runner_test.rs::sender_local_backend_routes_to_engine_gate` | Local sender refuses an uninstalled profile (no cloud fallback, no HTTP) |
| `src-tauri/tests/embedding/embedding_runner_test.rs::sender_local_probe_reports_not_installed_without_cloud_call` | Offline probe reports actionable disabled without touching the cloud |
| `src-tauri/tests/embedding/embedding_runner_test.rs::sender_provider_id_labels_backend` | Rows label `Openai` (cloud) vs `bango_local` (local) |
| `src-tauri/tests/embedding/embedding_runner_test.rs::sender_cloud_probe_without_config_reports_llm_not_configured` | Production sender's `ConfiguredProvider` probe returns the default disabled outcome (regression: the override called the trait method on `self`, self-recursing into a stack-overflow SIGSEGV) |
| `src-tauri/tests/embedding/embedding_runner_test.rs::sender_cloud_probe_delegates_to_the_http_probe` | Cloud probe reaches the shared HTTP probe body (Anthropic short-circuit outcome) instead of re-entering the override |
| `src-tauri/tests/embedding/embedding_backend_setting_test.rs::backend_parse_exact_is_strict_for_command_arguments` | Strict command-boundary parse rejects garbage (vs the forgiving DB read) + round-trips both values |

## T6 - Settings UI

| Test | Assertion |
|---|---|
| `src/__tests__/composables/use-local-embeddings.test.ts::load_populates_selection_status_and_subscribes_to_events` | `load()` fetches both commands + registers the `embedding:component` listener; scope dispose releases it |
| `src/__tests__/composables/use-local-embeddings.test.ts::progress_events_update_and_terminal_done_refreshes_status` | Events drive `progress`; terminal `done` clears `installing` + reloads status |
| `src/__tests__/composables/use-local-embeddings.test.ts::select_backend_persists_and_updates_shared_ref` | `set_embedding_backend` persists; the backend ref is shared across composable instances |
| `src/__tests__/composables/use-local-embeddings.test.ts::install_reloads_status_failures_surface_in_error` | Install reloads status; failures set `error` + rethrow for the card |
| `src/__tests__/composables/use-local-embeddings.test.ts::verify_stores_outcome_remove_reloads_status` | Verify outcome stored; remove invokes the command + reloads |
| `src/__tests__/composables/use-local-embeddings.test.ts::listener_resolving_after_scope_dispose_is_released_immediately` | A listener resolving after scope dispose is released immediately (leak regression) |
| `src/__tests__/components/embeddings-consent-dialog.test.ts::renders_model_terms_and_live_download_size` | Consent dialog shows model, Gemma terms, and the live download size |
| `src/__tests__/components/embeddings-consent-dialog.test.ts::falls_back_to_platform_range_without_status` | Null status falls back to the "about 220-300 MB" platform range |
| `src/__tests__/components/embeddings-consent-dialog.test.ts::cancel_button_receives_initial_focus` | The cancel button is focus-armed on mount (never the big download) |
| `src/__tests__/components/embeddings-consent-dialog.test.ts::escape_key_cancels_and_buttons_emit_their_actions` | Escape cancels; Cancel/Download emit their actions |
| `src/__tests__/components/citation-local-embeddings-dialog.test.ts::renders_the_three_actions_and_model` | Contextual prompt renders Download / Use Configured Provider / Cancel |
| `src/__tests__/components/citation-local-embeddings-dialog.test.ts::disables_all_actions_while_installing_and_shows_progress` | Installing disables every action + Escape, and shows the live percent |
| `src/__tests__/components/citation-local-embeddings-dialog.test.ts::emits_download_useCloud_and_cancel` | The three actions emit their events |
| `src/__tests__/components/citation-local-embeddings-dialog.test.ts::renders_an_install_error_inline` | Install errors render inline for retry |
| `src/__tests__/components/settings-embeddings.test.ts::selecting_bango_local_when_not_ready_opens_consent_without_persisting` | Not-ready + select local opens consent; nothing persisted until confirm |
| `src/__tests__/components/settings-embeddings.test.ts::consent_cancel_closes_the_dialog_and_keeps_the_cloud_selection` | Cancel keeps the cloud selection |
| `src/__tests__/components/settings-embeddings.test.ts::consent_confirm_selects_the_backend_then_installs` | Confirm persists `bango_local` then starts the install |
| `src/__tests__/components/settings-embeddings.test.ts::selecting_bango_local_when_ready_persists_without_the_dialog` | Ready install persists the selection with no consent dialog |
| `src/__tests__/components/settings-embeddings.test.ts::shows_the_repair_banner_when_the_runtime_is_missing` | Missing runtime surfaces the repair banner |
| `src/__tests__/components/settings-embeddings.test.ts::manual_download_routes_through_consent_when_cloud_selected` | L4: the card's Download opens consent (Gemma terms) when cloud is selected; nothing installs before confirm |
| `src/__tests__/components/settings-embeddings.test.ts::manual_download_installs_directly_when_local_selected` | With local selected (consent already given) the Download installs directly |
| `src/__tests__/components/settings-embeddings.test.ts::card_describes_what_embeddings_do_and_links_to_the_help_section` | The card description is one non-technical sentence + a Learn-more link routing to `/help?tab=reference#ref-embeddings`; the in-card privacy table is gone |
| `src/__tests__/components/settings-embeddings.test.ts::radios_disable_while_a_backend_switch_is_in_flight` | The provider radios disable while `selectBackend` persists (no double-fire during the off-thread session reset) |
| `src/__tests__/components/citation-local-embeddings-dialog.test.ts::hides_use_configured_provider_when_the_chat_provider_cannot_embed` | The option hides with an explanation for providers without embedding APIs; the license line shows regardless |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_bango_local_ready_passes_gate_despite_stale_disabled` | The local Phase A gate tracks actual readiness, not the persisted disabled triple |
| `src-tauri/tests/embedding/embedding_component_test.rs::runtime_extract_zip_matches_dot_prefixed_members` | Zip `./`-prefix parity with the tar member matching |

## T7 - Backend-aware readiness + contextual activation

| Test | Assertion |
|---|---|
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_bango_local_skips_anthropic_static_override` | Local backend + Anthropic chat provider stays clickable (static override does not fire); `local_ready` false until installed |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_bango_local_reports_persisted_disabled` | Persisted disabled (offline probe outcome) reports disabled + not ready |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_cloud_backend_reports_defaults` | Cloud backend reports `configured_provider` + `local_ready: false` |

## T4 - Live engine spike (ignored, artifacts via env vars)

| Test | Assertion |
|---|---|
| `src-tauri/tests/embedding/embedding_engine_live_test.rs::live_embeddinggemma_q4_end_to_end` | Live: pins verify, pinned runtime archive installs + engine embeds without `ORT_DYLIB_PATH`, direct Q4 load via ort load-dynamic, 768-dim role-prefixed output, retrieval sanity (`BANGO_EMBED_MODEL_DIR` required - the runtime archive downloads itself) |
| `src-tauri/tests/embedding/embedding_engine_live_test.rs::embeddinggemma_q4_acceptance_smoke` | T8 acceptance smoke (self-sufficient, network): installs BOTH pinned components, verifies, embeds a 200-doc corpus via `LocalEngine`, 768-dim + count + retrieval-sanity asserts, and prints install/cold-call/throughput/warm-latency/peak-RSS observations (RAM lower-bound assert on linux) |

## Operational app-install check (ignored; no network)

| Test | Assertion |
|---|---|
| `src-tauri/tests/embedding/embedding_engine_live_test.rs::live_embed_chunk_fixture_against_installed_app` | Embeds the committed `pone-0285956` chunk fixture through the components the APP downloaded (storage root from `BANGO_STORAGE_ROOT` / the app DB read-only / the platform default; no network, no `ORT_DYLIB_PATH`): asserts install Ready + offline probe enabled/768/`LOCAL_PROFILE_ID`, per-chunk 1x768 finite vectors, and prints the time each chunk embedding takes plus a min/mean/max/total summary |
| `src-tauri/tests/embedding/embedding_engine_live_test.rs::local_engine_reset_off_thread_after_probe` | Live backend-switch freeze regression: probe loads the session, `reset_off_thread` drops it without blocking the caller, the next probe lazy-reloads enabled (app install required) |
| `src-tauri/tests/embedding/embedding_engine_live_test.rs::generate_pone_chunks_fixture` | Plain-`#[ignore]` fixture generator: rewrites `tests/assets/pone-0285956-chunks.json` from the committed PLOS ONE PDF via `utils::sections::extract_sections` + `utils::chunking::chunk_sections(DEFAULT_CHUNK_WORDS)` |
