import { ref } from 'vue';
import type ArticleDetailSlideOver from '@/components/article-detail-slide-over.vue';

/**
 * Overlay guards for the shared `ArticleDetailSlideOver` inside biblio views.
 *
 * The slide-over component owns the `useArticleSearch` wiring + panel
 * lifecycle; the view keeps only these guards so `article:` links open the
 * full article detail without leaving the view (closing returns to the exact
 * network state: graph, cluster selection, cached analysis).
 */
export function useArticleDetailOverlay() {
  const articleDetailRef = ref<InstanceType<typeof ArticleDetailSlideOver> | null>(null);
  const showArticleDetail = ref(false);
  const isArticleDetailFullScreen = ref(false);

  function onArticleDetailOpened(): void {
    showArticleDetail.value = true;
  }

  function onArticleDetailClosed(): void {
    showArticleDetail.value = false;
    isArticleDetailFullScreen.value = false;
  }

  function onArticleDetailToggleFullScreen(): void {
    isArticleDetailFullScreen.value = !isArticleDetailFullScreen.value;
  }

  return {
    articleDetailRef,
    showArticleDetail,
    isArticleDetailFullScreen,
    onArticleDetailOpened,
    onArticleDetailClosed,
    onArticleDetailToggleFullScreen,
  };
}
