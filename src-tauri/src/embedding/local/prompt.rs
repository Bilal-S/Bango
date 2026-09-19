//! EmbeddingGemma prompt profile: asymmetric query/document prefixes.
//!
//! EmbeddingGemma requires task prefixes on every input; embedding identical
//! raw strings for both roles degrades retrieval. fastembed does not add
//! these itself, so Bango owns the formatting. The prefix strategy is part of
//! the profile identity (`LOCAL_PROFILE_ID` in [`super::profile`]): changing
//! it bumps the profile revision so the director's model-mismatch staleness
//! regenerates affected rows.

/// Prefix for retrieval queries (user claims, search text).
pub const QUERY_PREFIX: &str = "task: search result | query: ";

/// Prefix for indexed documents (title+abstract rows, chunks).
pub const DOCUMENT_PREFIX: &str = "title: none | text: ";

/// Which side of the retrieval task a text belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingRole {
    /// A retrieval query: a user claim or search text.
    Query,
    /// An indexed document row: title+abstract or a full-text chunk.
    Document,
}

/// Apply the profile's role prefix to one text. Pure.
#[must_use]
pub fn apply_role_prefix(text: &str, role: EmbeddingRole) -> String {
    let prefix = match role {
        EmbeddingRole::Query => QUERY_PREFIX,
        EmbeddingRole::Document => DOCUMENT_PREFIX,
    };
    format!("{prefix}{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_prefix_precedes_text() {
        let formatted = apply_role_prefix("sugar tax reduces obesity", EmbeddingRole::Query);
        assert_eq!(formatted, "task: search result | query: sugar tax reduces obesity");
    }

    #[test]
    fn document_prefix_precedes_text() {
        let formatted =
            apply_role_prefix("Sugar Tax\n\nWe studied obesity.", EmbeddingRole::Document);
        assert_eq!(formatted, "title: none | text: Sugar Tax\n\nWe studied obesity.");
    }

    #[test]
    fn query_and_document_prefixes_differ() {
        assert_ne!(QUERY_PREFIX, DOCUMENT_PREFIX);
    }
}
