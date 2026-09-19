# Bango AI - Binding Test Inventory

Plan: `.worktrees/bangoai-plan.md` (T2-T8 scopes; architecture and locked decisions live there).
Enforced by `scripts/check-test-inventory.sh` via `npm run check:all`.

Convention: rows tagged `#[ignore = "slow"]` (T4 live spike + T8 acceptance) run
via `npm run test:rust:full` or explicitly with `-- --ignored`.
Every other row ships as a compiling `#[ignore = "tierN stub"]` prep-PR stub
(Rust) or an `it.skip` stub (TS) until its tier lands; the `// TODO: tierN`
marker in the test file identifies un-implemented rows.

## T2 shared extraction

| Test | Assertion |
|---|---|
| `src-tauri/src/local_ai/manifest.rs::generic_manifest_validation_rejects_bad_pins` | Bad hashes, sizes, schemes, and archive member paths are rejected by the shared validator |
| `src-tauri/src/local_ai/state.rs::assessment_reports_repair_for_stranded_replaced_park` | Generic assessment returns RepairRequired for a stranded `.staging/replaced-*` tree |
## T3 foundations

| Test | Assertion |
|---|---|
| `src-tauri/src/llm/backend.rs::parse_exact_rejects_unknown_and_parse_falls_back` | Strict and forgiving backend parsing behave like the embedding enum |
| `src-tauri/src/llm/local/hardware.rs::verdict_warns_below_ram_floor_and_without_avx2` | Warning reasons are reported without blocking |
| `src-tauri/src/llm/local/hardware.rs::verdict_unsupported_without_disk_headroom_or_target` | Unsupported reasons are fatal only for disk and target |
| `src-tauri/src/llm/local/policy.rs::context_default_follows_total_ram` | 16k below 24 GB and 32k at or above it, clamps to allowed values |
| `src-tauri/src/llm/local/policy.rs::thread_budget_uses_cores_minus_two_capped_at_eight` | Floor 1, ceiling 8, independent of the embedding cap |
| `src-tauri/src/llm/local/manifest.rs::bango_ai_manifest_pins_runtime_and_model_files` | Profile id, URLs, sizes, and hashes parse and validate |
| `src-tauri/tests/db/app_settings_llm_backend_test.rs::llm_backend_round_trips_and_defaults` | Missing/garbage values read as configured_provider; stored values round-trip |
| `src-tauri/tests/db/app_settings_llm_backend_test.rs::llm_backend_travels_with_project_backup` | Portable export/import carries the selection; engine settings stay machine-local |
## T4 spike

| Test | Assertion |
|---|---|
| `src-tauri/tests/llm/bango_ai_live_test.rs::bango_ai_runtime_install_and_version` | Live (slow): pinned archive downloads, hash-verifies, extracts the pinned member set, and `llama-server --version` runs |
## T5 engine manager

| Test | Assertion |
|---|---|
| `src-tauri/src/llm/local/engine.rs::engine_starts_lazily_and_publishes_effective_endpoint` | First effective-config request starts the server and returns the loopback config |
| `src-tauri/src/llm/local/engine.rs::engine_stop_releases_the_process` | Hard stop kills the child and returns to Stopped (native sleep stays the idle strategy) |
| `src-tauri/src/llm/local/engine.rs::engine_restarts_once_then_reports_failed` | Start attempts are bounded by the attempt budget; the crash budget only counts a Ready-then-dead server; exhaustion fails actionably |
| `src-tauri/src/llm/local/engine.rs::engine_reset_is_off_thread_and_waits_for_in_flight` | Reset never blocks a caller and observes in-flight requests |
| `src-tauri/src/llm/local/engine.rs::port_selection_retries_a_lost_race` | Bind-0 race retries and eventually fails actionably |
| `src-tauri/src/llm/local/engine.rs::reserved_port_is_stable_before_start_and_rereleased_on_retry` | The held listener keeps the reserved port bound until spawn; a retry re-reserves and republishes a fresh port |
| `src-tauri/tests/commands/bango_ai_test.rs::status_reports_derived_state_hardware_and_paths` | Status helper returns derived state, verdict, sizes, and resolved paths |
| `src-tauri/tests/commands/bango_ai_test.rs::install_preflight_gates_target_and_disk` | Preflight rejects unsupported targets and short disk before any download |
| `src-tauri/tests/commands/bango_ai_test.rs::verify_and_remove_are_idempotent` | Repeat verify/remove leave a healthy or clean state without errors |
| `src-tauri/tests/commands/bango_ai_test.rs::set_llm_backend_persists_and_stops_engine` | Switch persists first, then resets the engine off-thread |
| `src-tauri/tests/commands/bango_ai_test.rs::install_progress_is_monotonic_across_phases` | Runtime-to-model handoff never regresses overall bytes |
| `src-tauri/tests/commands/bango_ai_test.rs::runtime_smoke_fails_before_model_download` | A failed executable smoke stops the transaction before any model bytes download |
| `src-tauri/tests/commands/bango_ai_test.rs::cancel_during_install_returns_before_success` | Cancellation at any phase boundary surfaces a resume hint and never promotes or succeeds |
| `src-tauri/tests/commands/bango_ai_test.rs::repair_install_restarts_engine_before_self_test` | Any install stops a running engine before touching artifacts; the self-test restarts it |
| `src-tauri/tests/commands/bango_ai_test.rs::activation_persists_backend_only_after_self_test` | llm_backend stays configured_provider through cancel/failure and flips only on self-test success |
| `src-tauri/tests/commands/bango_ai_test.rs::engine_status_and_persisted_settings_agree_after_restart` | A freshly seeded engine reports the persisted context/threads in the status payload |
| `src-tauri/src/llm/local/engine.rs::busy_engine_reports_in_flight_and_queued_state` | In-flight accounting exposes a busy/queued state for the status event |
## T6 routing and readiness

| Test | Assertion |
|---|---|
| `src-tauri/tests/llm/llm_backend_routing_test.rs::orchestrator_uses_bango_ai_config_when_selected` | Effective local config is used and the stored cloud config is untouched |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::orchestrator_uses_stored_config_by_default` | Default backend behavior is byte-identical to today |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::local_concurrency_and_timeout_overrides_apply` | Concurrency 1 and the 1800 s timeout apply only to the local backend |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::local_timeout_override_covers_every_request_type` | Every generation `LlmRequestType` variant (today 120/120/60/600 s) resolves the 1800 s local override |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::unsupported_target_blocks_generation_without_cloud_fallback` | Persisted bango_ai on an unsupported machine returns an actionable error and never sends to the stored cloud endpoint |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::save_llm_config_rejects_the_bango_ai_runtime_provider` | The runtime-only provider can never be persisted |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::effective_context_window_prevents_oversized_local_prompts` | A stored 50k cloud row cannot budget prompts against the 16k local window; call sites use effective_config::resolve and the window helper agrees |
| `src-tauri/src/llm/effective_config.rs::resolver_returns_a_complete_local_config_without_starting_the_engine` | The resolver yields the reserved endpoint, profile model, context, and concurrency 1 without spawning the server |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::local_screening_json_intent_reaches_the_client` | A screening-shaped call sends response_format json_object on the bango_ai path only |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::wiki_output_budget_uses_effective_config` | estimated_output_budget_tokens reads the effective config and carries a BangoAi arm |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::local_temperature_rejection_does_not_persist_skip_temperature` | A local recovery never writes the cloud skip_temperature flag |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::local_send_json_sends_response_format_json_object` | send_json sends response_format json_object on the bango_ai path only; cloud paths never receive the field |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::structured_types_force_thinking_off_prose_honors_toggle` | Screening/citation/extraction always disable thinking; the toggle affects only prose request types |
| `src-tauri/tests/llm/llm_backend_routing_test.rs::resolver_uses_persisted_context_and_threads` | Persisted context/threads win over RAM recommendations; resolver and settings repo agree |
| `src-tauri/tests/llm/llm_backend_readiness_test.rs::has_usable_llm_is_backend_aware` | Local readiness requires installed components; cloud readiness keeps the existing rule |
| `src-tauri/tests/llm/llm_backend_readiness_test.rs::embedding_director_gate_accepts_bango_ai` | Local-only users (bango_local embeddings) generate embeddings without a cloud row |
| `src-tauri/tests/llm/llm_backend_readiness_test.rs::embedding_director_cloud_branch_still_requires_cloud_config` | configured_provider embeddings with bango_ai chat and no cloud row still skips with LlmNotConfigured |
| `src-tauri/tests/llm/llm_backend_readiness_test.rs::wiki_ingest_and_batch_phases_accept_bango_ai` | Wiki-ingest gates and batch summary/translation phases proceed under bango_ai with no cloud row |
| `src-tauri/tests/llm/llm_backend_readiness_test.rs::translation_worker_uses_effective_config_and_context` | The worker accepts bango_ai without a cloud row and packs batches against the effective context |
| `src-tauri/tests/export/project_backup_test.rs::cloud_row_restore_precedence_ignores_llm_backend` | Backup cloud-triple restore precedence stays keyed on has_config regardless of llm_backend |
| `src/__tests__/stores/llm-config-store.test.ts::is_configured_is_backend_aware` | Ready Bango AI yields configured true without cloud config; not-ready yields false |
| `src/__tests__/stores/llm-config-store.test.ts::refresh_backend_state_unlocks_after_local_ready` | After install activation, refreshBackendState unlocks isConfigured without a re-init or restart |
## T7 UI

| Test | Assertion |
|---|---|
| `src/__tests__/composables/use-bango-ai.test.ts::loads_status_and_subscribes_to_component_events` | One listener per scope, released on dispose |
| `src/__tests__/composables/use-bango-ai.test.ts::install_success_refreshes_status_and_clears_installing` | Terminal done event refreshes and clears progress state |
| `src/__tests__/composables/use-bango-ai.test.ts::install_failure_sets_error_and_rethrows` | Failures surface to the card and rethrow |
| `src/__tests__/composables/use-bango-ai.test.ts::shared_backend_ref_keeps_panels_in_sync` | Panel switching and selection stay consistent through the shared ref |
| `src/__tests__/composables/use-bango-ai.test.ts::test_connection_reports_throughput_as_info` | Model-load, first-response, and tokens/s timings are exposed as information without gating |
| `src/__tests__/composables/use-bango-ai.test.ts::terminal_install_event_refreshes_llm_config_gate` | Terminal install events and backend switches refresh the canonical llm-config gate |
| `src/__tests__/components/settings-bango-ai-card.test.ts::renders_hardware_warning_below_ram_floor` | Warning shows the measured RAM and the install-anyway path |
| `src/__tests__/components/settings-bango-ai-card.test.ts::selecting_bango_ai_opens_consent_when_not_ready` | Selection without components opens the consent dialog |
| `src/__tests__/components/settings-bango-ai-card.test.ts::renders_component_details_and_onedrive_note` | Details show paths, runtime version, sizes, and fallback note |
| `src/__tests__/components/settings-bango-ai-card.test.ts::shows_monotonic_progress_and_cancel` | Progress never regresses and Cancel invokes the command |
| `src/__tests__/components/settings-backend-selection.test.ts::selection_header_switches_provider_and_bango_ai` | Radio selection persists the backend and expands the right panel without unmounting the provider card |
| `src/__tests__/components/settings-backend-selection.test.ts::provider_selection_persists_configured_provider` | Choosing Configured Provider persists llm_backend = configured_provider |
| `src/__tests__/components/settings-backend-selection.test.ts::consent_install_shows_progress_card_before_activation` | A consent-triggered install mounts the progress card and its error without the backend being selected |
| `src/__tests__/components/settings-bango-ai-card.test.ts::restored_selection_without_components_offers_setup_or_switch` | The not-set-up state offers Set Up / Use Configured Provider and triggers nothing automatic |
| `src/__tests__/components/bango-ai-contextual-activation.test.ts::not_ready_backend_offers_in_place_setup_or_switch` | A gated feature hitting not-ready Bango AI offers Set Up / Use Configured Provider / Cancel without a Settings trip |
| `src/__tests__/components/bango-ai-consent-dialog.test.ts::shows_model_size_and_license_and_emits_confirm` | Consent names the model, size, MIT link, and emits confirm/cancel |
| `src/__tests__/components/help-tab-reference.test.ts::renders_bango_ai_section_below_embeddings` | Help section exists in order with non-technical content |
| `src/__tests__/components/settings-bango-ai-card.test.ts::unsupported_target_renders_blocked_state_without_cloud_fallback` | The panel shows the actionable unavailable state and never promises cloud fallback |
## T8 acceptance

| Test | Assertion |
|---|---|
| `src-tauri/tests/llm/bango_ai_live_test.rs::bango_ai_ornith_acceptance_smoke` | Live (slow, network): installs pinned runtime + Ornith Q4_K_M, starts the engine, runs a real screening JSON prompt and a summary prompt, asserts parseable JSON, thinking-off behavior, and prints first-token/tokens-per-second/RAM observations |
| `src-tauri/tests/llm/bango_ai_live_test.rs::bango_ai_engine_idle_stop_with_override` | Live (slow): the debug idle override puts the server to sleep (native sleep primary, kill fallback per T4); resident memory (RSS) is printed as an observation, not hard-asserted |
