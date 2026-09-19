//! Integration tests for the Citation Finder run-slot guard
//! (`commands::citation_finder::begin_citation_run`).
//!
//! Regression context: `find_citations` used to reset the cancel token BEFORE
//! the `is_running` guard, so a second Find click un-cancelled the active
//! search. The guard must only touch the token when a fresh run starts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use bango_lib::citation_finder::CitationFinderProgress;
use bango_lib::commands::citation_finder::begin_citation_run;

fn progress_with(message: &str, is_running: bool) -> CitationFinderProgress {
    CitationFinderProgress { message: message.to_string(), is_running, ..Default::default() }
}

#[test]
fn guard_starts_and_clears_cancel() {
    let progress = Mutex::new(progress_with("idle", false));
    let cancel = AtomicBool::new(true);

    let active = begin_citation_run(&progress, &cancel).expect("guard");

    assert!(active.is_none(), "a fresh start returns None");
    assert!(!cancel.load(Ordering::Relaxed), "the token resets for the new run");
    assert!(progress.lock().expect("progress mutex").is_running, "the run is marked running");
}

#[test]
fn guard_returns_existing_snapshot_without_clearing_cancel() {
    let progress = Mutex::new(progress_with("Classifying…", true));
    let cancel = AtomicBool::new(true);

    let active = begin_citation_run(&progress, &cancel).expect("guard");

    let snapshot = active.expect("a running search returns its snapshot");
    assert_eq!(snapshot.message, "Classifying…");
    assert!(cancel.load(Ordering::Relaxed), "a second Find must not un-cancel the active run");
    assert!(progress.lock().expect("progress mutex").is_running);
}
