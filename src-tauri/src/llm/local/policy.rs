//! Pure engine policy for Bango AI: the RAM-aware context default, context
//! clamping, and the generation thread budget (separate from the embedding
//! cap of 4, because a generation call wants more cores while a background
//! embedding batch must stay polite).

/// Selectable context sizes (tokens).
pub const ALLOWED_CONTEXTS: [i32; 4] = [8_192, 16_384, 32_768, 65_536];

/// At or above this much total RAM the default context is 32k.
pub const LARGE_RAM_THRESHOLD_MB: u64 = 24 * 1024;

/// At or above this much total RAM the default context is 64k (the model
/// trains to 256k; the 64k KV cache is roughly 10 GB for this 9B).
pub const XLARGE_RAM_THRESHOLD_MB: u64 = 48 * 1024;

/// Maximum generation threads. The small pinned model is memory-bandwidth
/// bound, so more threads keep helping up to 12; the UI/SQLite reserve is
/// already subtracted by `llm_thread_budget`.
pub const MAX_LLM_THREADS: usize = 12;

/// Cores reserved for the UI, async runtime, and SQLite.
pub const RESERVED_CORES: usize = 2;

/// User-tunable engine settings (machine-local app settings). The type lives
/// here (pure) so the config resolver and the engine share it without a
/// module cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSettings {
    pub context: i32,
    pub threads: usize,
    pub reasoning: bool,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self { context: 16_384, threads: 4, reasoning: false }
    }
}

/// Recommended settings for a machine (editable in the UI from T7).
#[must_use]
pub fn recommend_settings(total_ram_mb: u64, cores: usize) -> EngineSettings {
    EngineSettings {
        context: context_default_for_ram(total_ram_mb),
        threads: llm_thread_budget(cores),
        reasoning: false,
    }
}

/// Default context for a machine: 32k on 24 GB+ machines, 16k below.
#[must_use]
pub fn context_default_for_ram(total_ram_mb: u64) -> i32 {
    if total_ram_mb >= XLARGE_RAM_THRESHOLD_MB {
        65_536
    } else if total_ram_mb >= LARGE_RAM_THRESHOLD_MB {
        32_768
    } else {
        16_384
    }
}

/// Clamp an arbitrary stored context to the nearest selectable size.
#[must_use]
pub fn clamp_context(value: i32) -> i32 {
    ALLOWED_CONTEXTS
        .iter()
        .copied()
        .min_by_key(|candidate| (i64::from(*candidate) - i64::from(value)).abs())
        .unwrap_or(16_384)
}

/// Generation thread budget: `cores - 2`, floor 1, ceiling 8.
#[must_use]
pub fn llm_thread_budget(cores: usize) -> usize {
    cores.saturating_sub(RESERVED_CORES).clamp(1, MAX_LLM_THREADS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_default_follows_total_ram() {
        assert_eq!(context_default_for_ram(8 * 1024), 16_384);
        assert_eq!(context_default_for_ram(16 * 1024), 16_384);
        assert_eq!(context_default_for_ram(24 * 1024), 32_768);
        assert_eq!(context_default_for_ram(48 * 1024), 65_536);
        assert_eq!(context_default_for_ram(64 * 1024), 65_536);
        // Stored values clamp to the nearest selectable size.
        assert_eq!(clamp_context(0), 8_192);
        assert_eq!(clamp_context(9_000), 8_192);
        assert_eq!(clamp_context(20_000), 16_384);
        assert_eq!(clamp_context(100_000), 65_536);
    }

    #[test]
    fn thread_budget_uses_cores_minus_two_capped_at_twelve() {
        assert_eq!(llm_thread_budget(1), 1);
        assert_eq!(llm_thread_budget(2), 1);
        assert_eq!(llm_thread_budget(4), 2);
        assert_eq!(llm_thread_budget(10), 8);
        assert_eq!(llm_thread_budget(14), 12);
        assert_eq!(llm_thread_budget(32), 12);
    }
}
