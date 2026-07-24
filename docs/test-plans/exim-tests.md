# Export/Import Orphan Audit Entry Cleanup - Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function/`it(` name. Any missing test blocks
the PR.

Covers the v006 heal + export filter + import normalization for the
empty-string `article_id` audit-entry data-hygiene bug. See the critique +
revised plan in `.worktrees/exim1.md` and the v006 migration doc comment in
`src-tauri/src/db/migrations/v006_audit_metadata_edit.rs` for background.

## Export/import round-trip (`export::project`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/project_backup_test.rs::export_drops_genuine_orphan_audit_entry` | Export filter drops rows whose `article_id` references a non-existent article (defense-in-depth for orphans created while FK was off) |
| `src-tauri/tests/project_backup_test.rs::export_preserves_null_and_empty_string_system_entries` | Export filter preserves system-level rows in BOTH shapes: `article_id IS NULL` (modern `log_error`) and `article_id = ''` (historical; normalized to NULL by v006 on next migration) |
| `src-tauri/tests/project_backup_test.rs::import_normalizes_empty_string_article_id_to_null` | Import path coerces `"articleId": ""` -> SQL NULL so the restored row doesn't violate the FK constraint; row is preserved, not dropped |

## Migration heal (`db::migrations::v006`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/migration_recovery_test.rs::v006_heals_empty_string_article_id_to_null` | v006 rebuild heals historical `article_id = ''` rows to NULL before the orphan DELETE + INSERT...SELECT (which would otherwise crash with FOREIGN KEY constraint failed) |