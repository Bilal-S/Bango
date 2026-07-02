//! Shared text tokenization used by both the Wiki FTS5 BM25 index and the
//! Tier 3 screening chunk-retrieval scorer.
//!
//! Centralizing the tokenizer here keeps the in-memory TF scorer
//! (`screening::chunk_retrieval`) consistent with the FTS5 `unicode61` index
//! (`wiki::fts::build_match_query`): both split on non-alphanumeric characters,
//! lowercase, and drop the same English stop-word list. A divergence between
//! the two would silently change which passages the scorer ranks highest vs.
//! which the index considers matches.
//!
//! Pure functions only: no I/O, no DB. All `#[must_use]`.

/// A small set of English stop words dropped from queries and scoring to avoid
/// surfacing/boosting only passages that happen to contain common particles.
/// Tokens are matched case-insensitively against lowercased input.
///
/// Mirrors the list previously inlined in `wiki/fts.rs`; kept here so the
/// FTS5 MATCH builder and the screening TF scorer share one source of truth.
pub const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is", "it",
    "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there", "these",
    "they", "this", "to", "was", "will", "with", "who", "what", "when", "where", "why", "how", "i",
    "you", "we", "he", "she", "me", "him", "her", "us", "do", "does", "did", "can", "could",
    "would", "should", "my", "your", "our",
];

/// Split text into raw (pre-stop-word-filtering) tokens.
///
/// Splits on any run of non-alphanumeric characters (so punctuation and
/// whitespace both separate tokens), lowercases each token. Empty tokens are
/// dropped. This matches FTS5's `unicode61` tokenizer behavior closely enough
/// that the in-memory scorer and the index agree on token boundaries.
///
/// Pure: no allocations beyond the returned `Vec`.
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// Tokenize and drop stop words.
///
/// Falls back to all tokens if stop-word stripping would remove everything
/// (so "the and is" still yields three tokens rather than silently becoming
/// empty). This mirrors `build_match_query`'s fallback rule so a criteria
/// string made entirely of stop words still contributes to scoring.
#[must_use]
pub fn tokenize_for_match(text: &str) -> Vec<String> {
    let raw = tokenize(text);
    if raw.is_empty() {
        return raw;
    }
    let meaningful: Vec<String> = raw.iter().filter(|t| !is_stop_word(t)).cloned().collect();
    if meaningful.is_empty() {
        raw
    } else {
        meaningful
    }
}

/// Whether a (lowercase) token is a stop word. Public so callers can filter
/// without re-tokenizing (e.g. when building a token-frequency map).
#[must_use]
pub fn is_stop_word(token: &str) -> bool {
    STOP_WORDS.contains(&token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_splits_on_non_alphanumeric() {
        let tokens = tokenize("Hello, world! RCT-123 45.6%");
        assert_eq!(tokens, vec!["hello", "world", "rct", "123", "45", "6"]);
    }

    #[test]
    fn tokenize_empty_input_returns_empty() {
        assert!(tokenize("").is_empty());
        assert!(tokenize("??? !!! ...").is_empty());
    }

    #[test]
    fn tokenize_lowercases() {
        let tokens = tokenize("MixedCase UPPER lower");
        assert_eq!(tokens, vec!["mixedcase", "upper", "lower"]);
    }

    #[test]
    fn tokenize_for_match_drops_stop_words() {
        // "the RCT and children" -> {rct, children} only.
        let tokens = tokenize_for_match("the RCT and children");
        assert_eq!(tokens, vec!["rct", "children"]);
    }

    #[test]
    fn tokenize_for_match_falls_back_to_all_when_only_stop_words() {
        // Every token is a stop word -> fall back to OR-joining all.
        let tokens = tokenize_for_match("the and is");
        assert_eq!(tokens, vec!["the", "and", "is"]);
    }

    #[test]
    fn tokenize_for_match_keeps_purely_meaningful() {
        let tokens = tokenize_for_match("sugar tax childhood obesity");
        assert_eq!(tokens, vec!["sugar", "tax", "childhood", "obesity"]);
    }

    #[test]
    fn is_stop_word_case_sensitive_on_lowercase() {
        assert!(is_stop_word("the"));
        assert!(!is_stop_word("The")); // caller lowercases first
        assert!(!is_stop_word("rct"));
    }
}
