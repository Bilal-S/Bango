import { tauriCommand } from './use-tauri-command';

export interface ArticleBulkDeps {
  /** Clears the multi-select set after a bulk mutation. */
  clearSelection: () => void;
  /** Re-runs the backend query so rows reflect the mutation. */
  search: () => Promise<void>;
  fetchTags: () => Promise<void>;
  fetchLabels: () => Promise<void>;
}

/**
 * Bulk multi-select operations over the article list. Extracted from
 * `useArticleSearch` (refactor1 T4.1); the parent re-exposes everything
 * unchanged.
 */
export function useArticleBulk(deps: ArticleBulkDeps) {
  const { clearSelection, search, fetchTags, fetchLabels } = deps;

  async function bulkUpdateStatus(ids: string[], newStatus: string): Promise<void> {
    await tauriCommand('bulk_update_article_status', { ids, newStatus });
    clearSelection();
    await search();
  }

  /** @returns number of articles that actually received the tag. */
  async function bulkAddTag(ids: string[], tagName: string): Promise<number> {
    const affected = await tauriCommand<number>('bulk_add_tag_to_articles', {
      articleIds: ids,
      tagName,
    });
    clearSelection();
    await fetchTags();
    await search();
    return affected;
  }

  /** @returns number of articles that actually received the label. */
  async function bulkAddLabel(ids: string[], labelName: string): Promise<number> {
    const affected = await tauriCommand<number>('bulk_add_label_to_articles', {
      articleIds: ids,
      labelName,
    });
    clearSelection();
    await fetchLabels();
    await search();
    return affected;
  }

  /** @returns number of articles from which the tag was removed (0 = not present). */
  async function bulkRemoveTag(ids: string[], tagName: string): Promise<number> {
    const affected = await tauriCommand<number>('bulk_remove_tag_from_articles', {
      articleIds: ids,
      tagName,
    });
    clearSelection();
    await fetchTags();
    await search();
    return affected;
  }

  /** @returns number of articles from which the label was removed. */
  async function bulkRemoveLabel(ids: string[], labelName: string): Promise<number> {
    const affected = await tauriCommand<number>('bulk_remove_label_from_articles', {
      articleIds: ids,
      labelName,
    });
    clearSelection();
    await fetchLabels();
    await search();
    return affected;
  }

  return {
    bulkUpdateStatus,
    bulkAddTag,
    bulkAddLabel,
    bulkRemoveTag,
    bulkRemoveLabel,
  };
}
