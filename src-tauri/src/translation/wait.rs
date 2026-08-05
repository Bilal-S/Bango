//! Event-driven translation-completion waiters.
//!
//! `TranslationDoneBus`: broadcast channel the worker emits on after each job.
//! `wait_for_article_translation`: resolves when status leaves `running`/`queued`,
//! listening to the bus + a 60s sanity poll. Missed events never deadlock.

use std::time::Duration;

use tauri::Manager;
use tokio::sync::broadcast;

use crate::db::article_repo;
use crate::db::connection::lock_conn;

/// Managed-state broadcast bus for completion notifications. Capacity 16:
/// subscribers only need the latest completion; full buffer degrades to 60s poll.
#[derive(Clone)]
pub struct TranslationDoneBus {
    tx: broadcast::Sender<String>,
}

impl TranslationDoneBus {
    /// Create a new bus with a fixed capacity.
    #[must_use]
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(16);
        Self { tx }
    }

    /// Subscribe to the bus. Each subscriber gets its own receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Emit a completion notification for `article_id`. Called by worker after each job.
    pub fn emit_done(&self, article_id: &str) {
        let _ = self.tx.send(article_id.to_string());
    }
}

impl Default for TranslationDoneBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-article wait timeout for sanity-poll fallback.
const WAIT_FALLBACK_TIMEOUT_SECS: u64 = 5 * 60;

/// Sanity-poll interval when no event arrives.
const SANITY_POLL_INTERVAL_SECS: u64 = 60;

/// Resolve once translation_status for `article_id` leaves `running`/`queued`.
/// Returns `Ok(())` on terminal status, `Err(msg)` on timeout.
///
/// Listens to `TranslationDoneBus` + periodic DB poll (60s) so missed events
/// never deadlock.
pub async fn wait_for_article_translation(
    app: &tauri::AppHandle,
    db: &std::sync::Mutex<rusqlite::Connection>,
    article_id: &str,
) -> Result<(), String> {
    eprintln!("[screening:diag] translation_wait: START article_id={article_id}");
    let mut rx = app.try_state::<TranslationDoneBus>().map(|s| s.subscribe());

    let deadline = std::time::Instant::now() + Duration::from_secs(WAIT_FALLBACK_TIMEOUT_SECS);

    loop {
        // Fast path: check live status first.
        if status_is_terminal(db, article_id) {
            eprintln!(
                "[screening:diag] translation_wait: DONE article_id={article_id} (terminal status)"
            );
            return Ok(());
        }

        let now = std::time::Instant::now();
        if now >= deadline {
            eprintln!(
                "[screening:diag] translation_wait: TIMEOUT article_id={article_id} after {WAIT_FALLBACK_TIMEOUT_SECS}s"
            );
            return Err(format!(
                "translation timed out for {article_id} after {WAIT_FALLBACK_TIMEOUT_SECS}s"
            ));
        }

        // Wait for bus event, sanity-poll tick, or deadline.
        let remaining = deadline - now;
        let poll_delay = Duration::from_secs(SANITY_POLL_INTERVAL_SECS).min(remaining);
        let sleep = tokio::time::sleep(poll_delay);

        match &mut rx {
            Some(rx) => {
                tokio::select! {
                    _ = sleep => {
                        // Sanity poll: loop and re-check the status.
                    }
                    recv = rx.recv() => {
                        match recv {
                            Ok(done_id) if done_id == article_id => {
                                // Event matches; loop to verify status is terminal.
                            }
                            Ok(_) => {
                                // Different article; keep waiting.
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                // Missed events; sanity poll catches up.
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                // Worker exited; fall back to pure polling.
                                tokio::time::sleep(poll_delay).await;
                            }
                        }
                    }
                }
            }
            None => {
                // No bus available (e.g. test harness).
                sleep.await;
            }
        }
    }
}

/// Read `translation_status` + `is_translated` and return `true` if the
/// article is no longer queued/running (i.e. the caller can proceed).
fn status_is_terminal(db: &std::sync::Mutex<rusqlite::Connection>, article_id: &str) -> bool {
    let Ok(conn) = lock_conn(db) else { return false };
    match article_repo::get_translation_status(&conn, article_id) {
        Ok(info) => {
            info.is_translated || !matches!(info.translation_status.as_str(), "queued" | "running")
        }
        Err(_) => false,
    }
}
