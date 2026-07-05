//! Integration test for the mutex-poison error path.
//!
//! `lock_conn` is the shared helper that every Tauri command handler and engine
//! routes `DbState.conn` locks through. It maps a `PoisonError` to
//! `AppError::LockPoisoned` (not `AppError::Database`), so a panic-while-locked
//! surfaces as an application-state error rather than a misleading SQL error.
//!
//! This test poisons a real `Mutex<Connection>` (by panicking while holding the
//! lock in a `catch_unwind` scope) and asserts the helper returns the correct
//! variant + message.

use std::sync::Mutex;

use bango_lib::db::connection::create_connection;
use bango_lib::db::connection::lock_conn;
use bango_lib::error::AppError;

#[test]
fn lock_conn_maps_poison_to_lock_poisoned() {
    let conn = create_connection().expect("in-memory connection");
    let m = Mutex::new(conn);
    // Poison the mutex by panicking while holding the lock.
    let _ = std::panic::catch_unwind(|| {
        let _guard = m.lock().expect("lock for poison setup");
        panic!("deliberate poison");
    });
    let err = lock_conn(&m).expect_err("should be poisoned");
    assert!(matches!(err, AppError::LockPoisoned(_)), "got: {err:?}");
    let s = err.to_string();
    assert!(s.contains("Internal state error"), "got: {s}");
    assert!(s.contains("lock poisoned"), "got: {s}");
}
