import type { ComputedRef, Ref } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { Article, AuditEntry } from '@/types';

export interface ArticleMutationsDeps {
  articles: Ref<Article[]>;
  selectedArticle: Ref<Article | null>;
  auditTrail: Ref<AuditEntry[]>;
  hasNext: ComputedRef<boolean>;
  navigateNext: () => Promise<void>;
  selectArticle: (id: string) => Promise<void>;
  syncArticleToList: (id: string) => void;
  /** Hard-close the detail panel + clear return targets (post-delete). */
  resetDetailView: () => void;
  fetchCounts: () => Promise<void>;
  search: () => Promise<void>;
  fetchTags: () => Promise<void>;
  fetchLabels: () => Promise<void>;
}

/**
 * Single-article IPC mutations (status move, delete, notes, metadata, tags,
 * labels, criteria, AI-reasoning clear) plus the shared post-mutation
 * refresh (re-select + patch the table row). Extracted from
 * `useArticleSearch` (refactor1 T4.1); the parent re-exposes everything
 * unchanged.
 */
export function useArticleMutations(deps: ArticleMutationsDeps) {
  const {
    articles,
    selectedArticle,
    auditTrail,
    hasNext,
    navigateNext,
    selectArticle,
    syncArticleToList,
    resetDetailView,
    fetchCounts,
    search,
    fetchTags,
    fetchLabels,
  } = deps;

  async function moveArticle(
    id: string,
    newStatus: string
  ): Promise<{ isLast: boolean; didNavigate: boolean }> {
    await tauriCommand('update_article_status', { id, newStatus });
    // Re-fetch the article so we get the updated changedAt from the backend
    const fresh = await tauriCommand<Article>('get_article', { id });
    // Patch the article in-place to reflect new status + changedAt without a full redraw
    const idx = articles.value.findIndex((a) => a.id === id);
    if (idx >= 0) {
      articles.value.splice(idx, 1, fresh);
    }
    const isLast = !hasNext.value;
    const autoNavigate = localStorage.getItem('bango-auto-navigate-after-decision') !== 'false';
    let didNavigate = false;
    if (!isLast && autoNavigate) {
      await navigateNext();
      didNavigate = true;
    } else {
      selectedArticle.value = fresh;
      auditTrail.value = await tauriCommand<AuditEntry[]>('get_audit_trail', { articleId: id });
    }
    // Refresh counts in the background (e.g. tab badges)
    void fetchCounts();
    return { isLast, didNavigate };
  }

  /**
   * Permanently delete an article. The confirmation dialog is owned by
   * `article-detail-panel.vue`. Re-throws the backend error so the caller can
   * surface a toast.
   */
  async function deleteArticle(id: string): Promise<void> {
    await tauriCommand('delete_article', { id });
    // Remove the deleted article from the cached list so the table redraws
    // immediately without waiting for the search() round-trip.
    const idx = articles.value.findIndex((a) => a.id === id);
    if (idx >= 0) {
      articles.value.splice(idx, 1);
    }
    // Close the detail panel (the selectedArticle no longer exists) and clear
    // the return-target back-stack.
    resetDetailView();
    // Refresh counts in the background (tab badges + biblio/wiki flags).
    void fetchCounts();
    // Re-run the query so the page is consistent (e.g. a new article slides
    // in to fill the vacated slot when paginating).
    void search();
  }

  async function updateNotes(id: string, notes: string): Promise<void> {
    await tauriCommand('update_article_notes', { id, notes });
    await selectArticle(id);
    syncArticleToList(id);
  }

  /**
   * Update a single metadata field on an article via the in-place editor.
   * `field` is the snake_case DB column name (e.g. `"publication_year"`).
   */
  async function updateMetadata(
    id: string,
    field: string,
    value: string | string[]
  ): Promise<void> {
    await tauriCommand('update_article_metadata', { id, field, value });
    await selectArticle(id);
    syncArticleToList(id);
  }

  async function updateTags(id: string, tagIds: string[]): Promise<void> {
    await tauriCommand('update_article_tags', { id, tagIds });
    await selectArticle(id);
    syncArticleToList(id);
    await fetchTags();
  }

  async function updateLabels(id: string, labelIds: string[]): Promise<void> {
    await tauriCommand('update_article_labels', { id, labelIds });
    await selectArticle(id);
    syncArticleToList(id);
    await fetchLabels();
  }

  async function updateCriteria(
    id: string,
    inclusionIds: string[],
    exclusionIds: string[]
  ): Promise<void> {
    await tauriCommand('update_article_criteria', { id, inclusionIds, exclusionIds });
    await selectArticle(id);
    syncArticleToList(id);
  }

  /**
   * Clear AI reasoning text + confidence. Nulls `ai_decision`, `ai_reasoning`,
   * `ai_confidence`. `status`, `screened_at`, and `manual_override` are preserved.
   */
  async function clearAiReasoning(id: string): Promise<void> {
    await tauriCommand('clear_ai_reasoning', { id });
    await selectArticle(id);
    syncArticleToList(id);
  }

  return {
    moveArticle,
    deleteArticle,
    updateNotes,
    updateMetadata,
    updateTags,
    updateLabels,
    updateCriteria,
    clearAiReasoning,
  };
}
