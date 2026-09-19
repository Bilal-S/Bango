import { computed, ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { ReferencePaperQuery } from '@/types';

/** Total-use threshold (citation + reference count) for the References-tab halo. */
export const REFERENCES_HALO_MIN_USES = 4;

/**
 * References-tab halo state: lights up the tab when at least one paper in the
 * Articles-of-Interest set (top unmatched, most-used reference papers; see
 * `reference_repo::get_articles_of_interest`) reaches the minimum total-use
 * count. Owned by `article-list.vue` - not `ReferencesView` - so the flag is
 * available at the tab bar even while the References tab content (which
 * fetches its own copy of the same list) is not mounted. Errors are
 * non-fatal: the halo simply stays off.
 */
export function useReferencesHalo() {
  const papers = ref<ReferencePaperQuery[]>([]);

  /** True when any top unmatched reference paper has enough total uses. */
  const hasHighUseReferences = computed(() =>
    papers.value.some((p) => p.citationCount + p.referenceCount >= REFERENCES_HALO_MIN_USES)
  );

  /** Re-fetch the articles-of-interest list behind the flag. */
  async function refresh(): Promise<void> {
    try {
      papers.value = await tauriCommand<ReferencePaperQuery[]>(
        'get_reference_articles_of_interest',
        {}
      );
    } catch (e: unknown) {
      console.error('[references-halo] refresh failed:', e);
      papers.value = [];
    }
  }

  return { hasHighUseReferences, refresh };
}
