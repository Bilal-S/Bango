use serde::Serialize;

/// Result of a tag/label merge. Returned by both `merge_tag` and `merge_label`.
///
/// The counts are computed BEFORE the destructive `UPDATE OR IGNORE` so the
/// co-occurrence overlap signal is preserved. The pre-confirm dialog can only
/// show an honest upper bound (`from.articleCount`); these precise counts
/// surface in the success toast after the merge commits.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    /// Name of the deleted source tag/label.
    pub from_name: String,
    /// Name of the surviving target tag/label.
    pub into_name: String,
    /// Articles whose tag/label link genuinely moved (excludes co-occurrence
    /// overlaps where the article already had the survivor and was simply
    /// de-linked by the cascade).
    pub reassigned_count: usize,
    /// Articles that already had the survivor tag/label and so were silently
    /// de-linked (no reassignment, just lost the duplicate link via CASCADE).
    /// Reported in the toast so the user understands why `reassigned_count`
    /// may be less than the from-tag's original article count.
    pub already_had_survivor_count: usize,
}
