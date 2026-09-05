# Tags & Labels Merge - Test Inventory

Binding inventory for the "Replace with..." merge feature (plan:
`.worktrees/tags2.md`). Enforced mechanically by
`scripts/check-test-inventory.sh` (wired into `npm run check:all`).

## Rust - `src-tauri/tests/db/merge_tags_labels_test.rs`

| Test file::function | Asserts |
|---------------------|---------|
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_reassigns_and_deletes` | source deleted, target survives, junction rows moved, `reassigned_count` correct |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_overcount_fix_overlap_subtracts_from_reassigned` | co-occurrence overlap subtracts from `reassigned_count` |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_no_dangling_overlap_rows` | CASCADE removes overlap rows; no dangling junction rows |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_same_id_rejected` | `from_id == into_id` rejected with `AppError::Validation` |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_missing_from_rejected` | bad `from_id` rejected with `AppError::NotFound` |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_missing_into_rejected` | bad `into_id` rejected with `AppError::NotFound` |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_writes_per_article_audit` | each reassigned article has a `tag_remove` audit row mentioning the merge |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_bumps_changed_at` | reassigned articles' `changed_at` advances |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_sets_staleness_flags` | `biblio_needs_refresh` + `wiki_needs_refresh` set after merge |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_tag_chain_safe` | chained A->B then B->C merges: no dangling rows, correct counts |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_label_reassigns_and_deletes` | label source deleted, target survives |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_label_overcount_fix` | label co-occurrence overlap subtracts from `reassigned_count` |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_label_same_id_rejected` | label `from_id == into_id` rejected |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_label_sets_staleness_flags` | both staleness flags set after label merge |
| `src-tauri/tests/db/merge_tags_labels_test.rs::merge_label_no_dangling_overlap_rows` | no dangling `article_labels` rows after label merge |

## Rust - `src-tauri/tests/db/tags_labels_test.rs` (bugfix regression guards)

| Test file::function | Asserts |
|---------------------|---------|
| `src-tauri/tests/db/tags_labels_test.rs::test_delete_tag_sets_staleness_flags` | `delete_tag` path sets both staleness flags (PR 1 bugfix) |
| `src-tauri/tests/db/tags_labels_test.rs::test_delete_label_sets_staleness_flags` | `delete_label` path sets both staleness flags (PR 1 bugfix) |

## TypeScript - stores

| Test file::function | Asserts |
|---------------------|---------|
| `src/__tests__/tags.test.ts::mergeTag (demo) removes from-tag and folds its count into the survivor` | demo branch removes `from`, folds count into survivor, returns synthesized `MergeResult` |
| `src/__tests__/tags.test.ts::mergeTag (demo) throws when from-id is unknown` | demo branch throws on unknown `from-id` |
| `src/__tests__/labels.test.ts::mergeLabel (demo) removes from-label and folds its count into the survivor` | demo branch mirrors the tag path |
| `src/__tests__/labels.test.ts::mergeLabel (demo) throws when into-id is unknown` | demo branch throws on unknown `into-id` |