#!/usr/bin/env bash
# scripts/clear-ft.sh - Clears full-text (FT) data and article reference data.
#
# Usage: ./scripts/clear-ft.sh

set -euo pipefail

DB_PATH="$HOME/.local/share/BonCode.Bango/bango.db"

if [[ ! -f "$DB_PATH" ]]; then
  echo "Error: Database not found at $DB_PATH" >&2
  exit 1
fi

echo "Clearing full-text data..."
sqlite3 "$DB_PATH" "
-- 1. Clear article_chunks table (cascade-deleted normally, but explicit for safety)
DELETE FROM article_chunks;

-- 2. Reset full-text columns on all articles
UPDATE articles SET
  full_text = NULL,
  full_text_ai_summary = NULL,
  full_text_file_name = NULL,
  has_full_text = 0;

-- 3. Delete audit entries
DELETE FROM audit_entries WHERE action LIKE '%full_text%' OR action LIKE '%ai_summary%';
"

echo "Clearing article reference data..."
sqlite3 "$DB_PATH" "
-- Delete all article-reference links (the junction table)
DELETE FROM article_reference_links;

-- Delete all reference papers
DELETE FROM reference_papers;

-- Reset the article flags
UPDATE articles SET
  has_reference_details = 0,
  has_citation_details = 0,
  num_references = NULL,
  reference_type = NULL;
"

echo "Database cleared successfully."
