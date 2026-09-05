# slow-plan Test Inventory

Binding test inventory for `.worktrees/slow-plan.md` (Bango performance +
translation-worker plan). Each row is a `file::function` identifier that MUST
exist (un-ignored, passing) before the plan's implementation is complete.

Per `docs/CLAUDE.md` §Testing (Test-First Protocol), this file is intended to
be wired into `scripts/check-test-inventory.sh`'s `PLAN_DOCS` array in a
follow-up PR that adds the `#[ignore]` / `it.skip` stubs. It is **not yet
machine-enforced** because the stubs do not exist; wiring it in prematurely
would break `npm run check:all`.

| `src-tauri/tests/translation/auto_translate_test.rs::auto_translate_defaults_to_false_when_absent` | Decision (a): absent key returns `false` (opt-in default). |
| `src-tauri/tests/translation/auto_translate_test.rs::auto_translate_garbage_value_falls_back_to_default_false` | Decision (a): garbage value falls back to the opt-in `false` default. |
| `src-tauri/src/db/article_repo.rs::get_translatable_import_ids_filters_status_and_language` | Tier 1b: filtered SELECT returns only `is_translated=0 AND translation_status IN ('none','failed')` candidates. |
| `src-tauri/src/db/article_repo.rs::mark_translation_queued_batch_idempotent` | Tier 1b: bulk UPDATE only touches rows still in `('none','failed')`. |
| `src-tauri/src/db/connection.rs::busy_timeout_set_on_open` | Tier 0: `create_connection_at` + `create_connection` both set `PRAGMA busy_timeout=5000`. |
| `src-tauri/src/translation/worker.rs::reenqueue_stranded_on_startup_caps_at_0` | Tier 1c: `STARTUP_STRANDED_CAP = 0` (no auto-recovery on restart). Every stranded article is reset to `failed` with an audit note; the user retries manually via the article detail panel. Covered by `translation_queue_test.rs::startup_fails_queued_and_running_articles`. |
| `src-tauri/src/translation/wait.rs::wait_for_article_translation_resolves_on_terminal_status` | Tier 1e: the waiter returns once `translation_status` leaves `queued`/`running`. |
| `src-tauri/src/commands/screening.rs::run_pre_screening_translation_skips_when_auto_translate_off` | Decision (b): the screening pre-step is a no-op when `auto_translate = false`. |
| `src-tauri/src/commands/screening.rs::run_pre_screening_translation_enqueues_when_auto_translate_on` | Decision (b): the screening pre-step enqueues `MetadataOnly` jobs for unscreened non-English working articles when `auto_translate = true`. |

## Coverage status (current implementation)

- `auto_translate_defaults_to_false_when_absent` - **exists + passing**
  (`tests/translation/auto_translate_test.rs`).
- `auto_translate_garbage_value_falls_back_to_default_false` - **exists +
  passing** (`tests/translation/auto_translate_test.rs`).
- The remaining rows are exercised indirectly by the existing
  `manual_translate_test.rs`, `auto_translate_full_text_test.rs`, and
  `batch_import_test.rs` suites (all green). Dedicated unit tests for the new
  repo helpers + the screening pre-step are deferred to the follow-up test
  PR per the Test-First Protocol.