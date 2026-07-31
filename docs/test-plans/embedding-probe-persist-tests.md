# Test Connection Embedding Probe Persistence - Test Inventory

Consumed by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).
Rows use the machine-parseable `` `path::fn` `` format the script's regex
expects. Pure-helper + DB-backed integration tests live in external
`src-tauri/tests/` files per `docs/CLAUDE.md` §Testing.

These tests pin two coupled contracts:

1. **`persist_embedding_probe_to_conn`** (`commands::llm_config`): the
   dimension-forwarding contract - Test Connection's probe forwards the real
   `dimensions` to `app_settings`, not a hardcoded 0.
2. **`embedding_relevant_changed` + `save_llm_config` conditional-reset**
   (`commands::llm_config`): the contract that a parameters-only LLM config
   save (concurrency / delay / context / temperature) does NOT reset
   `embedding_status`, while a provider / endpoint / model / api-key change
   DOES. The parameters-only reset was the root cause of the "probe fires on
   first Citation Finder call" bug - the Settings auto-save watcher fired
   `save_llm_config` after Test Connection, wiping the `enabled` status the
   probe had just set and forcing Phase B to re-probe redundantly.

## Rust

| Test identifier | Assertion |
|-----------------|-----------|
| `src-tauri/tests/embedding_probe_persist_test.rs::persist_probe_forwards_real_dimensions_when_enabled` | real `dimensions` forwarded (not hardcoded 0) so `recall` works immediately after Test Connection |
| `src-tauri/tests/embedding_probe_persist_test.rs::persist_probe_stores_zero_dimensions_when_disabled` | disabled probe → dimensions 0 (correct: no vectors returned) |
| `src-tauri/tests/embedding_probe_persist_test.rs::persist_probe_overwrites_stale_dimensions_on_re_probe` | re-probe overwrites the previous dimensions (provider switch) |
| `src-tauri/tests/embedding_probe_persist_test.rs::persist_probe_disabled_then_enabled_round_trip` | disabled → enabled sequence round-trips cleanly |
| `src-tauri/tests/embedding_probe_persist_test.rs::embedding_relevant_changed_provider_change_detected` | provider change → reset |
| `src-tauri/tests/embedding_probe_persist_test.rs::embedding_relevant_changed_endpoint_change_detected` | endpoint change → reset |
| `src-tauri/tests/embedding_probe_persist_test.rs::embedding_relevant_changed_model_change_detected` | model change → reset |
| `src-tauri/tests/embedding_probe_persist_test.rs::embedding_relevant_changed_api_key_change_detected` | api-key change → reset |
| `src-tauri/tests/embedding_probe_persist_test.rs::embedding_relevant_changed_parameters_only_not_detected` | parameters-only edit (concurrency/delay/context/temperature/skip_temperature) → NO reset (the bug-fix regression pin) |
| `src-tauri/tests/embedding_probe_persist_test.rs::embedding_relevant_changed_identical_configs_not_detected` | identical configs → no reset |
| `src-tauri/tests/embedding_probe_persist_test.rs::save_llm_config_parameters_only_preserves_enabled_status` | parameters-only save preserves a known-good `enabled` status + model + dimensions (end-to-end bug-fix pin) |
| `src-tauri/tests/embedding_probe_persist_test.rs::save_llm_config_provider_change_resets_status` | provider/endpoint/model/api-key change resets status to `unknown` (re-probe on next call) |
| `src-tauri/tests/embedding_probe_persist_test.rs::save_llm_config_first_save_resets_status` | first save with no prior config resets (cold-start path: no prev to compare) |