//! CPU thread budget for local embedding inference.
//!
//! ONNX Runtime's default session uses every physical core as intra-op
//! threads, which saturates the machine and janks the UI during bulk
//! indexing. Desktop usability beats absolute throughput: cap the pool and
//! leave headroom for the UI thread, tokio workers, and SQLite.

/// Hard ceiling for intra-op threads regardless of core count.
pub const MAX_EMBEDDING_THREADS: usize = 4;

/// Cores reserved for the OS, UI thread, async runtime, and database work.
pub const RESERVED_CORES: usize = 2;

/// Intra-op thread budget for the local embedding session.
///
/// `min(MAX_EMBEDDING_THREADS, max(1, cores - RESERVED_CORES))`.
/// An unknown core count (0) still yields 1 so session creation never
/// receives a zero thread count.
#[must_use]
pub fn embedding_thread_budget(available_cores: usize) -> usize {
    available_cores.saturating_sub(RESERVED_CORES).clamp(1, MAX_EMBEDDING_THREADS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_caps_at_four_threads() {
        assert_eq!(embedding_thread_budget(6), 4);
        assert_eq!(embedding_thread_budget(8), 4);
        assert_eq!(embedding_thread_budget(16), 4);
        assert_eq!(embedding_thread_budget(128), 4);
    }

    #[test]
    fn budget_reserves_two_cores() {
        // Below the cap the reservation is visible: 5 cores -> 3 threads.
        assert_eq!(embedding_thread_budget(5), 3);
        assert_eq!(embedding_thread_budget(4), 2);
    }

    #[test]
    fn budget_small_machine_floor_one() {
        assert_eq!(embedding_thread_budget(0), 1);
        assert_eq!(embedding_thread_budget(1), 1);
        assert_eq!(embedding_thread_budget(2), 1);
        assert_eq!(embedding_thread_budget(3), 1);
    }
}
