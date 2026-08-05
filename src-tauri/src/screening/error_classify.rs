use crate::error::AppError;

/// Consecutive transient-failure threshold for the auto-stop guard.
pub const TRANSIENT_FAILURE_THRESHOLD: u32 = 3;

/// Total timeout threshold: auto-stop after this many non-consecutive timeouts.
pub const TOTAL_TIMEOUT_THRESHOLD: u32 = 3;

/// Reason the run should stop (drives `fatal_error` message).
#[derive(Debug, Clone)]
#[must_use]
pub struct FatalReason {
    pub message: String,
}

/// Outcome of classifying an LLM error. Caller acts on the variant.
#[derive(Debug, Clone)]
#[must_use]
pub enum LlmErrorOutcome {
    /// Transient error: articles left unscreened; defer `batch_len` and
    /// continue (or stop if `should_stop` is `Some`).
    Defer {
        batch_len: usize,
        is_timeout: bool,
        should_stop: Option<FatalReason>,
        warn_slow_llm: bool,
    },
    /// Non-transient: mark the batch as errors and continue.
    HardError,
}

/// Classify LLM error → outcome for engine loop. Pure: no progress/DB state.
/// Updates `consecutive_transient_failures` and `total_timeouts` by `&mut`.
pub fn classify_llm_error(
    e: &AppError,
    batch_len: usize,
    consecutive_transient_failures: &mut u32,
    total_timeouts: &mut u32,
) -> LlmErrorOutcome {
    let transient = is_transient_llm_error(e);
    let auth_failure = is_auth_failure(e);

    if !transient {
        return LlmErrorOutcome::HardError;
    }

    // Transient path: bump counters and check thresholds.
    *consecutive_transient_failures += 1;
    let is_timeout = e.to_string().to_lowercase().contains("timed out");
    if is_timeout {
        *total_timeouts += 1;
    }

    let should_stop = if auth_failure {
        Some(FatalReason {
            message: format!(
                "Authentication failed (401/403). Please check your API key in Settings. Last error: {e}"
            ),
        })
    } else if is_timeout && *total_timeouts >= TOTAL_TIMEOUT_THRESHOLD {
        Some(FatalReason {
            message: format!(
                "Screening stopped: the LLM timed out {total_timeouts} times (each at the 120-second cap).                                      It cannot process batch_size within the time limit.                                      Reduce batch_size to 1-2 and restart. Already-screened articles are saved."
            ),
        })
    } else if *consecutive_transient_failures >= TRANSIENT_FAILURE_THRESHOLD {
        Some(FatalReason {
            message: format!(
                "LLM unavailable after {consecutive_transient_failures} consecutive failures. Last error: {e}"
            ),
        })
    } else {
        None
    };

    let warn_slow_llm = is_timeout && *total_timeouts == 1;

    LlmErrorOutcome::Defer { batch_len, is_timeout, should_stop, warn_slow_llm }
}

/// Permanent auth failure (wrong/revoked API key). 401/403 WITHOUT the
/// Windows-transient `"insufficient permissions for this operation"` body.
/// Stop immediately (threshold=1) - every subsequent batch will fail identically.
#[must_use]
pub fn is_auth_failure(e: &AppError) -> bool {
    let msg = e.to_string().to_lowercase();
    // Must contain 401 or 403...
    let has_auth_status = msg.contains("401") || msg.contains("403");
    if !has_auth_status {
        return false;
    }
    // ...but NOT the Windows transient body (that one succeeds on resubmit).
    !msg.contains("insufficient permissions for this operation")
}

/// Classify LLM error as transient (network/rate-limit/timeout) vs non-transient
/// (malformed JSON, count mismatch). Transient leaves articles unscreened for next run.
/// Matches error strings from `client::send_with_retry` / `orchestrator::send`.
#[must_use]
pub fn is_transient_llm_error(e: &AppError) -> bool {
    let msg = e.to_string().to_lowercase();
    // Rate limit / server errors (status codes appear in the error string).
    if msg.contains("429") || msg.contains("rate limit") {
        return true;
    }
    // Auth transients (the Windows OpenAI/Cloudflare "insufficient permissions"
    // 401/403 that succeeds on resubmit). Real auth failures also contain 401/
    // 403 in the string - we treat ALL 401/403 as transient so articles are not
    // mass-marked as errors; the next run re-attempts and either succeeds
    // (transient) or fails again (user must fix their key, but no data loss).
    if msg.contains("401") || msg.contains("403") {
        return true;
    }
    // Server errors (500/502/503/504) and request timeout (408).
    if msg.contains("500") || msg.contains("502") || msg.contains("503") || msg.contains("504") {
        return true;
    }
    if msg.contains("408") || msg.contains("request timeout") {
        return true;
    }
    // Orchestrator timeout.
    if msg.contains("timed out") {
        return true;
    }
    // Transport errors (connection reset, TLS failure, network unreachable).
    if msg.contains("transport error")
        || msg.contains("connection")
        || msg.contains("network")
        || msg.contains("tls")
    {
        return true;
    }
    false
}
