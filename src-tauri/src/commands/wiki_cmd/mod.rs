//! Wiki Tauri commands: status, init, raw files, page CRUD, search/lint,
//! chat, ingest pipeline, and static-site export.
//!
//! Directory module split (refactor v6):
//! - `mod.rs` (this file) - shared progress helpers (`pub(super)`),
//!   module declarations, and `pub use` re-exports so every historical
//!   `crate::commands::wiki_cmd::*` import path keeps resolving (including
//!   the `lib.rs` invoke-handler list + the `generate_export_inner` /
//!   `SiteExportBundle` test entry points).
//! - `status.rs` - drift check, status/root/init commands + result structs.
//! - `raw_files.rs` - raw-file add/list/export commands.
//! - `pages.rs` - page CRUD + page/source listings.
//! - `search_lint.rs` - search, lint, graph.
//! - `chat.rs` - wiki_chat delegate.
//! - `ingest.rs` - ingest / rebuild / export-and-ingest + batch builder.
//! - `site_export.rs` - static-site generate / zip / file helpers.
//!
//! Public API unchanged: `bango_lib::commands::wiki_cmd::*` import paths work
//! identically to the pre-split single-file module.

mod chat;
mod ingest;
mod pages;
mod raw_files;
mod search_lint;
mod site_export;
mod status;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// Re-export every public symbol so callers + the lib.rs invoke-handler list
// keep using `commands::wiki_cmd::<name>` without caring about the split.
//
// Glob re-exports are required (not explicit `pub use` lists) because the
// `#[tauri::command]` proc macro generates `__tauri_command_name_*` consts
// alongside each command fn; the `invoke_handler!` macro in `lib.rs`
// references them via `commands::wiki_cmd::__tauri_command_name_*`. Glob
// re-exports surface these macro-generated items automatically. Only `pub`
// items are re-exported; `pub(super)` helpers (emit_wiki_progress,
// log_wiki_ingest_warnings) stay module-private.
pub use chat::*;
pub use ingest::*;
pub use pages::*;
pub use raw_files::*;
pub use search_lint::*;
pub use site_export::*;
pub use status::*;

use std::sync::Mutex;

use serde::Serialize;
use tauri::Emitter;

/// Total steps in the wiki rebuild pipeline (for the progress bar).
///
/// Marked `pub` because `wiki::ingest::batching` and `wiki::ingest::mod`
/// reference it when emitting their own `wiki:progress` events during the
/// chunked ingest. The submodules of `wiki_cmd` access it via `super::`.
pub const WIKI_PIPELINE_TOTAL_STEPS: usize = 100;

/// Progress payload emitted via the `wiki:progress` event.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WikiProgress {
    pub step: usize,
    pub total_steps: usize,
    pub message: String,
}

/// Emit a `wiki:progress` event. Shared by the ingest/rebuild/export-and-ingest
/// commands in `ingest.rs` and `site_export.rs`.
pub(super) fn emit_wiki_progress(app_handle: &tauri::AppHandle, step: usize, message: &str) {
    let _ = app_handle.emit(
        "wiki:progress",
        WikiProgress { step, total_steps: WIKI_PIPELINE_TOTAL_STEPS, message: message.to_string() },
    );
}

/// Log non-fatal ingest warnings (e.g. ungrounded pages, batch failures) to the
/// audit table so they surface in Settings > Diagnostics and Notification
/// History. Called after a successful ingest when `report.errors` is non-empty.
/// Without this, the user sees only the toast count ("1 errors") with no way to
/// find out what the error was.
///
/// Uses the canonical `audit_repo::log_error` (action = 'error', article_id =
/// NULL, source = 'system') which is in the `audit_entries.action` CHECK
/// allowlist. The previous `log_wiki_error` helper used action =
/// 'wiki_ingest_error' which is NOT in the CHECK constraint, so SQLite silently
/// rejected every insert and wiki errors never reached Diagnostics.
pub(super) fn log_wiki_ingest_warnings(
    conn: &rusqlite::Connection,
    report: &crate::wiki::ingest::IngestReport,
) {
    if report.errors.is_empty() {
        return;
    }
    let summary = format!(
        "Wiki ingest completed with {} warning(s): {}",
        report.errors.len(),
        report.errors.join("; ")
    );
    if let Err(e) = crate::db::audit_repo::log_error(conn, &summary) {
        eprintln!("[wiki] failed to log ingest warnings to audit table: {e}");
    }
}

/// Managed state holding the currently-active wiki ingest's cancel token, if
/// any.
///
/// `Some` while an ingest is in flight; the frontend's `cancel_wiki_ingest`
/// command calls `.cancel()` on it. The slot is cleared when the ingest
/// returns (success, error, or cancel). Mirrors `ScrapingState` in
/// `commands/scraping.rs`.
///
/// The cancel token is an `Arc<AtomicBool>` (not a `tokio::sync::Notify`)
/// because the wiki ingest pipeline checks it between synchronous pre-seed
/// steps (which run on the tokio runtime but do not `await`), not just inside
/// `tokio::select!` branches. The `run_chunked_ingest` LLM-batch loop polls
/// the same `AtomicBool` between `join_next().await` completions and calls
/// `join_set.abort_all()` on cancel.
#[derive(Default)]
pub struct WikiIngestState {
    active: Mutex<Option<Arc<AtomicBool>>>,
}

impl WikiIngestState {
    /// Lock the active-token slot, recovering from poison by taking the inner
    /// guard. A poisoned mutex here means a panic occurred while a prior
    /// `set_active`/`clear_active`/`cancel_active` held the lock; the slot is
    /// still readable/writable, and the cancel contract is best-effort anyway
    /// (the frontend's Stop button is the authoritative signal), so we recover
    /// rather than propagate. Mirrors `ScrapingState::lock_active`.
    fn lock_active(&self) -> std::sync::MutexGuard<'_, Option<Arc<AtomicBool>>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Install `token` as the active ingest token. Returns the previously
    /// active token (if any).
    pub fn set_active(&self, token: Arc<AtomicBool>) -> Option<Arc<AtomicBool>> {
        self.lock_active().replace(token)
    }

    /// Clear the active token slot. Called when the ingest returns.
    pub fn clear_active(&self) {
        *self.lock_active() = None;
    }

    /// Signal cancellation to the active token, if one is present. Safe to
    /// call when no ingest is running (no-op).
    pub fn cancel_active(&self) {
        if let Some(token) = self.lock_active().as_ref() {
            token.store(true, Ordering::SeqCst);
        }
    }
}

/// Check whether a cancel token (if present) has been signalled.
///
/// Pure helper - no Tauri state dependency. The wiki ingest pipeline calls
/// this between each pre-seed step and between LLM batch completions to decide
/// whether to abort early. `None` means no cancel token was threaded in (e.g.
/// tests), so the pipeline runs to completion.
#[must_use]
pub fn is_cancelled(token: Option<&Arc<AtomicBool>>) -> bool {
    token.is_some_and(|t| t.load(Ordering::SeqCst))
}
