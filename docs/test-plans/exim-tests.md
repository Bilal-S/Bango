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