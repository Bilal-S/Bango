# Export/Import - Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function/`it(` name. Any missing test blocks
the PR.

Covers the v006 heal + export filter + import normalization for the
empty-string `article_id` audit-entry data-hygiene bug, plus the
`full_text_ai_summary` JSON-blob round-trip fix. See the critique + revised
plan in `.worktrees/exim1.md` and the v006 migration doc comment in
`src-tauri/src/db/migrations/v006_audit_metadata_edit.rs` for the audit bug
background.

## Export/import round-trip (`export::project`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/export/project_backup_test.rs::export_drops_genuine_orphan_audit_entry` | Export filter drops rows whose `article_id` references a non-existent article (defense-in-depth for orphans created while FK was off) |
| `src-tauri/tests/export/project_backup_test.rs::export_preserves_null_and_empty_string_system_entries` | Export filter preserves system-level rows in BOTH shapes: `article_id IS NULL` (modern `log_error`) and `article_id = ''` (historical; normalized to NULL by v006 on next migration) |
| `src-tauri/tests/export/project_backup_test.rs::import_normalizes_empty_string_article_id_to_null` | Import path coerces `"articleId": ""` -> SQL NULL so the restored row doesn't violate the FK constraint; row is preserved, not dropped |

## LLM config preservation (`export::project` + `commands::export_cmd`)

Import Backup and Start New Project keep a locally defined LLM connection
(the backup carries no secret; overwriting would strand a keyless config).

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/export/project_backup_test.rs::import_keeps_locally_defined_llm_config` | A usable local row (provider + key blob + custom tuning) survives the import verbatim; the backup triple does NOT overwrite it |
| `src-tauri/tests/export/project_backup_test.rs::import_restores_backup_llm_config_when_local_not_usable` | With no usable local connection (`has_config` false), the backup's provider/endpoint/model triple restores with default tuning |
| `src-tauri/tests/export/reset_project_test.rs::reset_preserve_llm_config_keeps_row_verbatim` | `reset_project_inner(conn, true)` (Start New Project) round-trips the whole `llm_config` row - provider, encrypted key blob, tuning - across the schema rebuild |
| `src-tauri/tests/export/reset_project_test.rs::reset_preserve_llm_config_without_row_is_noop` | Preserving with no existing row never fabricates one |
| `src-tauri/tests/export/reset_project_test.rs::reset_wipes_llm_config_when_not_preserved` | `reset_project_inner(conn, false)` (Delete All Data) still wipes `llm_config` |

## `full_text_ai_summary` round-trip (`export::project`)

`serialize_table` parses TEXT as JSON first, so the always-JSON AI summary
blob exports as a nested JSON object. The import path re-serializes that
object back to text instead of reading the field as a string.

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/export/project_backup_test.rs::export_import_preserves_full_text_ai_summary_json_blob` | A realistic schema_version: 2 blob seeded via `article_repo::set_ai_summary` survives export -> import semantically intact (was silently dropped to NULL because `get_str_field`'s `.as_str()` yields None for JSON objects) |
| `src-tauri/tests/export/project_backup_test.rs::import_full_text_ai_summary_string_shape_passthrough` | Backups that carry the column as a plain JSON string (old/hand-edited) pass through byte-identically |

## Migration heal (`db::migrations::v006`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/db/migration_recovery_test.rs::v006_heals_empty_string_article_id_to_null` | v006 rebuild heals historical `article_id = ''` rows to NULL before the orphan DELETE + INSERT...SELECT (which would otherwise crash with FOREIGN KEY constraint failed) |