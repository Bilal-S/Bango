# Bundled Resources

Place system-distributed resource files here. They will be bundled with the
Tauri application and available at runtime via `app.path().resource_dir()`.

## Journal Index Portal DB

- **File:** `journal_index.db` — a SQLite database containing the
  `journal_index` table populated from CSV files by the `import_journals` script.
- **Generation:** Run `cd scripts/import_journals && cargo run -- /path/to/csv/dir`
  to create/update the portal DB before building the application.
