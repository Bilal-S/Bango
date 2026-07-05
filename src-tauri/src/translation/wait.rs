//! Event-driven translation-completion waiters.
//!
//! Tier 1e + Tier 3 share these helpers so batch-import Phase 3 and the
//! screening translation pre-step do not poll the DB every 2s per article.
//!
//! Two layers:
//! - `TranslationDoneBus`: a `tokio::sync::broadcast` channel the worker emits
//!   on after each job finishes (success or failure). One bus is created at
//!   app startup and managed as Tauri state.
//! - `wait_for_article_translation(app, db, article_id)`: resolves once the
//!   article's `translation_status` leaves `'running'`/`'queued'`. Listens to
//!   the bus AND falls back to a 60s sanity poll so a missed event (e.g. the
//!   bus buffer dropping under burst load) never deadlocks the caller.

use std::time::Duration;

use tauri::Manager;
use tokio::sync::broadcast;

use crate::db::article_repo;
use crate::db::connection::lock_conn;

/// The managed-state broadcast bus the worker emits on after each job. The
/// channel capacity is deliberately small (16): subscribers only need to
/// observe the *latest* completion, and a full buffer degrades gracefully to
/// the 60s fallback poll (the bus never blocks the worker).
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

    /// Emit a completion notification for `article_id`. Called by the worker
    /// after each job finishes. Non-fatal: if there are no subscribers the
    /// send is a no-op (the broadcast channel returns `Err` which we ignore).
    pub fn emit_done(&self, article_id: &str) {
        let _ = self.tx.send(article_id.to_string());
    }
}

impl Default for TranslationDoneBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-article wait timeout for the sanity-poll fallback. Bounded so a stuck
/// job does not block the caller indefinitely; the caller proceeds with
/// whatever text exists (matches the batch-import Phase 3 contract).
const WAIT_FALLBACK_TIMEOUT_SECS: u64 = 5 * 60;

/// Sanity-poll interval when no event arrives (e.g. the bus buffer dropped).
/// 60s matches the user-requested "overall polling every 60 seconds as backup
/// as sanity check" cadence.
const SANITY_POLL_INTERVAL_SECS: u64 = 60;

/// Resolve once `translation_status` for `article_id` leaves `'running'`/
/// `'queued'`. Returns:
/// - `Ok(())` if the status becomes `'succeeded'`/`'failed'`/`'none'` or the
///   article is `is_translated`.
/// - `Err(msg)` if the article is still queued/running after
///   [`WAIT_FALLBACK_TIMEOUT_SECS`].
///
/// Listens to [`TranslationDoneBus`] for prompt notification AND falls back to
/// a periodic DB poll every `SANITY_POLL_INTERVAL_SECS` seconds so a missed
/// event never deadlocks the caller.
pub async fn wait_for_article_translation(
    app: &tauri::AppHandle,
    db: &std::sync::Mutex<rusqlite::Connection>,
    article_id: &str,
) -> Result<(), String> {
    let mut rx = app.try_state::<TranslationDoneBus>().map(|s| s.subscribe());

    let deadline = std::time::Instant::now() + Duration::from_secs(WAIT_FALLBACK_TIMEOUT_SECS);

    loop {
        // Fast path: check the live status first.
        if status_is_terminal(db, article_id) {
            return Ok(());
        }

        let now = std::time::Instant::now();
        if now >= deadline {
            return Err(format!(
                "translation timed out for {article_id} after {WAIT_FALLBACK_TIMEOUT_SECS}s"
            ));
        }

        // Wait for either: a bus event, a sanity-poll tick, or the deadline.
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
                                // The event matches; loop and verify the status
                                // is actually terminal (the bus fires on both
                                // success and failure).
                            }
                            Ok(_) => {
                                // Different article; keep waiting.
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                // Missed some events; the sanity poll will catch up.
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                // Worker exited; drop the receiver and fall
                                // back to pure polling for the remainder.
                                tokio::time::sleep(poll_delay).await;
                            }
                        }
                    }
                }
            }
            None => {
                // No bus available (e.g. test harness without managed state).
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
