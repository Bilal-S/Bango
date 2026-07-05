#!/usr/bin/env python3
"""
One-shot mechanical refactor for the mutex-poison error variant (item8plan.md).

Replaces every inline `.lock().map_err(|e| AppError::Database(
rusqlite::Error::InvalidParameterName(e.to_string())))` lock site with a call
to the shared `crate::db::connection::lock_conn(...)` helper (which maps to
`AppError::LockPoisoned`). Also rewires the private helpers in
`translation/engine.rs` (`lock_db`) and `commands/wiki_cmd.rs` (`lock_conn`)
to the shared one.

Run from the repo root. Prints per-file change counts. Idempotent: a second run
reports 0 changes.

DOES NOT TOUCH:
  - `db/reference_repo.rs` (legit `InvalidParameterName` domain error, not a lock)
  - `batch_import/mod.rs` (non-fatal `match db.conn.lock() { ... }` arms)
  - the helper definitions themselves (handled by follow-up targeted edits)
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path("src-tauri/src")

# --- Shape 1: the dominant 4-line command-handler block ---------------------
#     let conn = db_state
#         .conn
#         .lock()
#         .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
SHAPE_1 = re.compile(
    r"(?P<indent>[ \t]*)let conn = db_state\n"
    r"[ \t]+\.conn\n"
    r"[ \t]+\.lock\(\)\n"
    r"[ \t]+\.map_err\(\|e\| AppError::Database\(rusqlite::Error::InvalidParameterName\(e\.to_string\(\)\)\)\)\?;",
    re.MULTILINE,
)
SHAPE_1_REPL = r"\g<indent>let conn = crate::db::connection::lock_conn(&db_state.conn)?;"

# --- Shape 2: wiki/chat.rs 3-line closure -----------------------------------
#     let conn = db_state.conn.lock().map_err(|e| {
#         AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
#     })?;
SHAPE_2 = re.compile(
    r"(?P<indent>[ \t]*)let conn = db_state\.conn\.lock\(\)\.map_err\(\|e\| \{\n"
    r"[ \t]+AppError::Database\(rusqlite::Error::InvalidParameterName\(e\.to_string\(\)\)\)\n"
    r"[ \t]+\}\)\?;",
    re.MULTILINE,
)
SHAPE_2_REPL = r"\g<indent>let conn = crate::db::connection::lock_conn(&db_state.conn)?;"

# --- Shape 3/4: screening/engine.rs conn_mutex closure (single or multi-line body) ---
#     let c = conn_mutex.lock().map_err(|e| {
#         AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
#     })?;
# and the |e2| split-body variant.
SHAPE_3 = re.compile(
    r"(?P<indent>[ \t]*)let c = conn_mutex\.lock\(\)\.map_err\(\|e\d*\| \{[\s\S]*?\}\)\?;",
    re.MULTILINE,
)
SHAPE_3_REPL = r"\g<indent>let c = crate::db::connection::lock_conn(conn_mutex)?;"

# --- Shape 5: translation/engine.rs lock_db calls (both forms) --------------
#   `lock_db(db)?` -> `crate::db::connection::lock_conn(db)?`
#   `let Ok(conn) = lock_db(db) else {` -> `let Ok(conn) = crate::db::connection::lock_conn(db) else {`
SHAPE_5_CALL = re.compile(r"\block_db\(db\)\b")
SHAPE_5_REPL = r"crate::db::connection::lock_conn(db)"

# --- Shape 6: wiki_cmd.rs local lock_conn calls -----------------------------
# The local helper takes `&State<DbState>`; the shared one takes `&Mutex<Connection>`.
# Rewrite every CALL (not the `fn lock_conn` definition) to use `&db_state.conn`.
# Matches: lock_conn(&db_state) / lock_conn(db_state)
WIKI_CALL = re.compile(r"\block_conn\((?P<arg>&?db_state)\)")
WIKI_CALL_REPL = r"crate::db::connection::lock_conn(&db_state.conn)"


def process(path: Path) -> int:
    text = path.read_text()
    original = text
    counts: dict[str, int] = {}

    def apply(pattern: re.Pattern[str], repl: str, label: str) -> None:
        nonlocal text
        new_text, n = pattern.subn(repl, text)
        if n:
            counts[label] = n
            text = new_text

    apply(SHAPE_1, SHAPE_1_REPL, "shape1")
    apply(SHAPE_2, SHAPE_2_REPL, "shape2")
    apply(SHAPE_3, SHAPE_3_REPL, "shape3")
    if path.as_posix() == "src-tauri/src/translation/engine.rs":
        apply(SHAPE_5_CALL, SHAPE_5_REPL, "shape5-lock_db")
    if path.as_posix() == "src-tauri/src/commands/wiki_cmd.rs":
        apply(WIKI_CALL, WIKI_CALL_REPL, "wiki_cmd-call")

    if text != original:
        path.write_text(text)
        total = sum(counts.values())
        detail = ", ".join(f"{k}={v}" for k, v in sorted(counts.items()))
        print(f"  {path}: {total} ({detail})")
        return total
    return 0


def main() -> int:
    if not ROOT.exists():
        print(f"ERROR: {ROOT} not found (run from repo root)", file=sys.stderr)
        return 1
    grand_total = 0
    files = sorted(ROOT.rglob("*.rs"))
    print("Applying lock-poison refactor...")
    for f in files:
        grand_total += process(f)
    print(f"Total replacements: {grand_total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())