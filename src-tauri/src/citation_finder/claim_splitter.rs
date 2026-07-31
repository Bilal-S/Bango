//! LLM-based claim splitting for per-statement mode (`citation_finder/AGENTS.md`).
//!
//! The splitter asks the LLM to break the pasted prose into ≤5 distinct
//! self-contained factual claims. Each claim is then embedded + matched
//! independently.
//!
//! The actual LLM call lives in `search.rs` (it owns the orchestrator). This
//! module owns the prompt template + the pure post-processing (max-5
//! enforcement, trimming).

/// The system prompt for the claim-splitting call.
pub const CLAIM_SPLITTER_SYSTEM_PROMPT: &str = "\
You are a claim extraction assistant. Your job is to split a block of academic \
prose into distinct, self-contained factual claims that a researcher might want \
to cite individually.";

/// Build the user prompt for the claim-splitting call.
///
/// Pure `#[must_use]`. The caller passes the pasted text; the LLM returns a
/// JSON array of strings.
#[must_use]
pub fn build_claim_splitter_prompt(text: &str) -> String {
    format!(
        "Split the following text into distinct factual claims. Each claim should be a single,\n\
         self-contained statement. Return at most 5 claims. If the text contains fewer than 5\n\
         distinct claims, return only the ones that exist.\n\n\
         Text: \"{text}\"\n\n\
         Return a JSON array of strings: [\"claim 1\", \"claim 2\", ...]"
    )
}

/// Maximum number of claims the splitter may return. The LLM is instructed to
/// respect this, but `enforce_max_claims` is the authoritative guard.
pub const MAX_CLAIMS: usize = 5;

/// Enforce the max-claims cap + trim each claim. Pure `#[must_use]`.
///
/// - Truncates the slice to `MAX_CLAIMS` (5) if the LLM over-returned.
/// - Trims leading/trailing whitespace from each claim.
/// - Drops empty claims (post-trim).
///
/// The order is preserved (input order = claim order).
#[must_use]
pub fn enforce_max_claims(claims: Vec<String>) -> Vec<String> {
    claims
        .into_iter()
        .take(MAX_CLAIMS)
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}
// Unit tests live in `src-tauri/tests/citation_finder_claim_split_test.rs`
// (extracted per `docs/CLAUDE.md` §Testing).
