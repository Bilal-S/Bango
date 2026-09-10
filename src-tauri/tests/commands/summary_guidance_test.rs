//! Tests for `commands::summary::normalize_guidance`, the premium-gated
//! normalization of the report-guidance params shared by `generate_summary`
//! and `analyze_research_gaps`.
//!
//! Non-premium callers always get the fixed `DEFAULT_TARGET_WORDS` (1000)
//! target length: their own instructions and word counts are dropped, so a
//! non-premium direct IPC call can never pick an arbitrary output length or
//! inject instructions. Premium callers keep the pass-through semantics
//! (instructions trimmed with blanks dropped, word counts of 0 dropped).

use bango_lib::commands::summary::normalize_guidance;
use bango_lib::AppFlags;

/// Non-premium callers that somehow supplied guidance values (direct IPC,
/// stale session state) still get the fixed default: both values overridden.
#[test]
fn non_premium_gets_default_target_words_even_when_values_supplied() {
    let flags = AppFlags { premium: false };
    let (instructions, words) =
        normalize_guidance(&flags, Some("Focus on UK policy studies.".to_string()), Some(2500));
    assert_eq!(instructions, None, "non-premium instructions must be dropped even when supplied");
    assert_eq!(words, Some(1000), "non-premium word count must be forced to the fixed default");
}

/// The real frontend path: non-premium callers send no guidance at all, and
/// the default target length is still applied.
#[test]
fn non_premium_gets_default_target_words_with_none_inputs() {
    let flags = AppFlags { premium: false };
    let (instructions, words) = normalize_guidance(&flags, None, None);
    assert_eq!(instructions, None);
    assert_eq!(words, Some(1000));
}

#[test]
fn premium_trims_instructions_and_drops_blank() {
    let flags = AppFlags { premium: true };
    let (instructions, _) =
        normalize_guidance(&flags, Some("  Keep methods terse.  ".to_string()), None);
    assert_eq!(instructions.as_deref(), Some("Keep methods terse."));

    let (blank, _) = normalize_guidance(&flags, Some("   ".to_string()), None);
    assert_eq!(blank, None, "blank instructions must be dropped");
}

#[test]
fn premium_drops_zero_word_count_and_passes_positive_through() {
    let flags = AppFlags { premium: true };
    let (_, zero) = normalize_guidance(&flags, None, Some(0));
    assert_eq!(zero, None, "zero word count must be dropped");

    let (_, positive) = normalize_guidance(&flags, None, Some(750));
    assert_eq!(positive, Some(750));
}
