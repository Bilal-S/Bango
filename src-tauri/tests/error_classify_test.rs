// Direct unit tests for `screening::error_classify::classify_llm_error` (Gap 5).
//
// The leaf classifiers (`is_transient_llm_error`, `is_auth_failure`) are
// exercised transitively via `screening_engine_test.rs` (14 call sites), but
// `classify_llm_error` - the counter-mutating decision tree that drives the
// auto-stop + slow-LLM-warning behavior - had zero direct tests before this
// file. A regression in its branching could silently change when the run
// auto-stops.

use bango_lib::error::AppError;
use bango_lib::screening::error_classify::{
    classify_llm_error, FatalReason, LlmErrorOutcome, TOTAL_TIMEOUT_THRESHOLD,
    TRANSIENT_FAILURE_THRESHOLD,
};

/// Construct an `AppError::Import(String)` carrying the given message body.
/// All LLM errors are routed through `AppError::Import`, so this mirrors the
/// production shape the classifier inspects.
fn import_err(message: &str) -> AppError {
    AppError::Import(message.to_string())
}

#[test]
fn classify_llm_error_non_transient_returns_hard_error() {
    // Malformed JSON / parse-count-mismatch are NOT transient: the batch is
    // marked as errors and the run continues.
    let mut consecutive = 0u32;
    let mut total_timeouts = 0u32;
    let outcome = classify_llm_error(
        &import_err("Malformed LLM response: unexpected token at line 1"),
        3,
        &mut consecutive,
        &mut total_timeouts,
    );

    assert!(
        matches!(outcome, LlmErrorOutcome::HardError),
        "non-transient error must return HardError, got {outcome:?}"
    );
    // Counters must NOT move on the HardError path.
    assert_eq!(consecutive, 0, "HardError must not bump consecutive counter");
    assert_eq!(total_timeouts, 0, "HardError must not bump total_timeouts");
}

#[test]
fn classify_llm_error_auth_failure_stops_immediately() {
    // A plain 401 (no Windows-transient body) is an auth failure: threshold
    // is 1, so the FIRST occurrence must set `should_stop`.
    let mut consecutive = 0u32;
    let mut total_timeouts = 0u32;
    let outcome = classify_llm_error(
        &import_err("LLM request failed: 401 Unauthorized - invalid api key"),
        5,
        &mut consecutive,
        &mut total_timeouts,
    );

    match outcome {
        LlmErrorOutcome::Defer { batch_len, should_stop, .. } => {
            assert_eq!(batch_len, 5, "batch_len must echo the passed batch size");
            let reason = should_stop
                .expect("auth failure (401) must set should_stop on the first occurrence");
            assert!(
                reason.message.contains("Authentication failed"),
                "fatal message must name auth failure: {}",
                reason.message
            );
            assert!(
                reason.message.contains("401"),
                "fatal message must include the status: {}",
                reason.message
            );
        }
        other => panic!("expected Defer for auth failure, got {other:?}"),
    }
}

#[test]
fn classify_llm_error_consecutive_transient_threshold_stops() {
    // TRANSIENT_FAILURE_THRESHOLD (3) consecutive transient errors stop the
    // run. The 3rd call must set `should_stop`.
    let mut consecutive = 0u32;
    let mut total_timeouts = 0u32;

    // Call 1: transient server error, below threshold -> no stop.
    let outcome = classify_llm_error(
        &import_err("LLM request failed: 503 Service Unavailable"),
        2,
        &mut consecutive,
        &mut total_timeouts,
    );
    assert!(
        matches!(&outcome, LlmErrorOutcome::Defer { should_stop: None, .. }),
        "1st transient must not stop, got {outcome:?}"
    );
    assert_eq!(consecutive, 1);

    // Call 2: still below threshold.
    let outcome = classify_llm_error(
        &import_err("LLM request failed: 502 Bad Gateway"),
        2,
        &mut consecutive,
        &mut total_timeouts,
    );
    assert!(
        matches!(&outcome, LlmErrorOutcome::Defer { should_stop: None, .. }),
        "2nd transient must not stop, got {outcome:?}"
    );
    assert_eq!(consecutive, 2);

    // Call 3: hits the threshold -> should_stop set.
    let outcome = classify_llm_error(
        &import_err("LLM request failed: 500 Internal Server Error"),
        2,
        &mut consecutive,
        &mut total_timeouts,
    );
    match outcome {
        LlmErrorOutcome::Defer { should_stop: Some(reason), .. } => {
            assert!(
                reason.message.contains("consecutive failures"),
                "fatal message must mention consecutive failures: {}",
                reason.message
            );
        }
        other => panic!("3rd consecutive transient must set should_stop, got {other:?}"),
    }
    assert_eq!(consecutive, TRANSIENT_FAILURE_THRESHOLD);
}

#[test]
fn classify_llm_error_total_timeout_threshold_stops() {
    // TOTAL_TIMEOUT_THRESHOLD (3) total (non-consecutive) timeouts stop the
    // run. This catches the pattern where batches succeed between timeouts,
    // resetting the consecutive counter, but throughput is too slow.
    let mut consecutive = 0u32;
    let mut total_timeouts = 0u32;

    // Timeout 1.
    let _ = classify_llm_error(
        &import_err("LLM request timed out after 120s"),
        1,
        &mut consecutive,
        &mut total_timeouts,
    );
    assert_eq!(total_timeouts, 1);

    // Simulate a successful batch between timeouts: the caller resets the
    // consecutive counter (the engine does this on success). total_timeouts
    // is NOT reset (it accumulates across the whole run).
    consecutive = 0;

    // Timeout 2.
    let _ = classify_llm_error(
        &import_err("LLM request timed out after 120s"),
        1,
        &mut consecutive,
        &mut total_timeouts,
    );
    assert_eq!(total_timeouts, 2);
    consecutive = 0;

    // Timeout 3 -> hits TOTAL_TIMEOUT_THRESHOLD -> should_stop set.
    let outcome = classify_llm_error(
        &import_err("LLM request timed out after 120s"),
        1,
        &mut consecutive,
        &mut total_timeouts,
    );
    match outcome {
        LlmErrorOutcome::Defer { should_stop: Some(FatalReason { message }), .. } => {
            assert!(
                message.contains("timed out 3 times"),
                "fatal message must mention 3 timeouts: {message}"
            );
            assert!(
                message.contains("Reduce batch_size"),
                "fatal message must be actionable: {message}"
            );
        }
        other => panic!("3rd total timeout must set should_stop, got {other:?}"),
    }
    assert_eq!(total_timeouts, TOTAL_TIMEOUT_THRESHOLD);
}

#[test]
fn classify_llm_error_first_timeout_warns_slow_llm() {
    // The 1st timeout sets `warn_slow_llm = true` (non-fatal yellow banner)
    // but does NOT stop the run.
    let mut consecutive = 0u32;
    let mut total_timeouts = 0u32;
    let outcome = classify_llm_error(
        &import_err("LLM request timed out after 120s"),
        4,
        &mut consecutive,
        &mut total_timeouts,
    );

    match outcome {
        LlmErrorOutcome::Defer { warn_slow_llm: true, should_stop: None, .. } => {}
        other => {
            panic!("1st timeout must set warn_slow_llm=true + no stop, got {other:?}")
        }
    }
}

#[test]
fn classify_llm_error_transient_below_threshold_defers_only() {
    // A single transient that is neither a timeout nor an auth failure, below
    // both thresholds, must defer without stopping or warning.
    let mut consecutive = 0u32;
    let mut total_timeouts = 0u32;
    let outcome = classify_llm_error(
        &import_err("LLM request failed: 429 Too Many Requests"),
        2,
        &mut consecutive,
        &mut total_timeouts,
    );

    match outcome {
        LlmErrorOutcome::Defer {
            should_stop: None,
            warn_slow_llm: false,
            is_timeout: false,
            ..
        } => {}
        other => panic!("plain transient must defer without stop/warn, got {other:?}"),
    }
    assert_eq!(consecutive, 1);
    assert_eq!(total_timeouts, 0, "429 is not a timeout; total_timeouts must not bump");
}

#[test]
fn classify_llm_error_increments_consecutive_counter() {
    // Each transient call increments the consecutive counter by exactly 1,
    // regardless of subtype (timeout, 429, server error, auth-transient).
    let mut consecutive = 0u32;
    let mut total_timeouts = 0u32;

    let _ =
        classify_llm_error(&import_err("429 rate limit"), 1, &mut consecutive, &mut total_timeouts);
    assert_eq!(consecutive, 1);

    let _ = classify_llm_error(
        &import_err("503 service unavailable"),
        1,
        &mut consecutive,
        &mut total_timeouts,
    );
    assert_eq!(consecutive, 2);

    let _ = classify_llm_error(
        &import_err("transport error: connection reset"),
        1,
        &mut consecutive,
        &mut total_timeouts,
    );
    assert_eq!(consecutive, 3);
}

#[test]
fn classify_llm_error_increments_total_timeouts_for_timeout_only() {
    // Only errors whose message contains "timed out" bump total_timeouts.
    // A 429 (transient but not a timeout) must not.
    let mut consecutive = 0u32;
    let mut total_timeouts = 0u32;

    let _ = classify_llm_error(
        &import_err("429 Too Many Requests"),
        1,
        &mut consecutive,
        &mut total_timeouts,
    );
    assert_eq!(total_timeouts, 0, "429 must not bump total_timeouts");

    let _ = classify_llm_error(
        &import_err("LLM request timed out after 120s"),
        1,
        &mut consecutive,
        &mut total_timeouts,
    );
    assert_eq!(total_timeouts, 1, "timeout must bump total_timeouts");
}
