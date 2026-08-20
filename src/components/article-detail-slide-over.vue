<template>
  <Transition name="detail-slide">
    <ArticleDetailPanel
      v-if="openPanel && detailArticle"
      :article="detailArticle"
      :audit-trail="detailAuditTrail"
      :has-previous="false"
      :has-next="false"
      :has-return-target="false"
      :full-screen="fullScreen"
      :article-position="1"
      :article-total="1"
      @close="close"
      @delete-article="handleDeleteArticle"
      @clear-ai-reasoning="handleClearAiReasoning"
      @toggle-full-screen="$emit('toggle-full-screen')"
      @update-notes="updateNotes"
      @update-tags="updateTags"
      @update-labels="updateLabels"
      @update-criteria="updateCriteria"
      @update-metadata="updateMetadata"
      @screen-article="screenArticle"
      @move-article="moveArticle"
      @attach-full-text="handleAttachFullText"
      @delete-full-text="deleteFullTextAttachment"
      @refresh-article="refreshArticle"
    />
  </Transition>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import ArticleDetailPanel from './article-detail-panel.vue';
import { useArticleSearch } from '@/composables/use-article-search';
import { useScreening } from '@/composables/use-screening';
import { useToast } from '@/composables/use-toast';
import { useFullTextAttachment } from '@/composables/use-full-text-attachment';
import { useArticleDelete } from '@/composables/use-article-delete';
import { useClearAiReasoning } from '@/composables/use-clear-ai-reasoning';

/**
 * Shared full-article detail slide-over for host views that open an article
 * by id without leaving the view (the biblio network views, via their
 * controls/detail panels). Extracted from the wiring previously copy-pasted
 * verbatim across `biblio-citations.vue`, `biblio-coauthors.vue`, and
 * `biblio-keywords.vue`.
 *
 * Contract:
 * - `open(articleId)` (via template ref) fetches the article + audit trail;
 *   on success the panel mounts and `opened` is emitted; on failure an error
 *   toast is shown and nothing opens.
 * - `close()` (via template ref, or the panel's close button) unmounts the
 *   panel, clears the loaded article + audit trail, and emits `closed`.
 * - `toggle-full-screen` is forwarded so the host owns the fullscreen flag
 *   (it also drives the host's canvas visibility guard).
 * - Hosts bind `opened` / `closed` to their overlay guards (domain detail
 *   panel hiding, graph canvas hidden while fullscreen).
 */
defineProps<{
  /** Mirrors the host-owned fullscreen flag into the panel. */
  fullScreen?: boolean;
}>();

const emit = defineEmits<{
  (e: 'opened'): void;
  (e: 'closed'): void;
  (e: 'toggle-full-screen'): void;
}>();

const toast = useToast();
const {
  selectedArticle: detailArticle,
  auditTrail: detailAuditTrail,
  selectArticle,
  refreshArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  updateMetadata,
  moveArticle,
  deleteArticle,
  clearAiReasoning,
  attachFullText,
  deleteFullTextAttachment,
} = useArticleSearch();
const { screenArticle } = useScreening();

const openPanel = ref(false);

/* Article delete orchestration centralized in `useArticleDelete`. Composable
 * nulls `selectedArticle`; `onDeleted` hook closes the panel. */
const { handleDeleteArticle } = useArticleDelete({
  deleteArticle,
  onDeleted: () => close(),
});

// Full-text attach UI orchestration is centralized in `useFullTextAttachment`.
const { handleAttachFullText } = useFullTextAttachment({ attachFullText });

/* AI-reasoning clear orchestration centralized in `useClearAiReasoning`.
 * Composable owns toast; `useArticleSearch.clearAiReasoning` owns IPC + refresh. */
const { handleClearAiReasoning } = useClearAiReasoning({ clearAiReasoning });

/** Open the panel for one article id; error toast on a failed fetch.
 * `selectArticle` captures fetch failures in its internal error state and
 * never throws, so the loaded-article check is the failure signal.
 *
 * Known edge (unreachable through the UI): a FAILED `open(B)` while the
 * panel already shows article A keeps showing A (silently, no toast), because
 * `selectArticle` never nulls `selectedArticle` on error. Every entry point
 * is covered while the panel is open (the z-40 themes panel sits under this
 * z-50 overlay; the citation paper panel hides via `!showArticleDetail`), and
 * `close()` nulls the loaded article, so any reachable re-open starts from
 * null. Strictly better than the previous pattern, which always showed the
 * panel on failure with no feedback at all. */
async function open(articleId: string): Promise<void> {
  await selectArticle(articleId);
  if (!detailArticle.value) {
    toast.show('Failed to load article details', 'error');
    return;
  }
  openPanel.value = true;
  emit('opened');
}

/** Close the panel and clear the loaded article + audit trail. */
function close(): void {
  openPanel.value = false;
  detailArticle.value = null;
  detailAuditTrail.value = [];
  emit('closed');
}

defineExpose({ open, close });
</script>

<style scoped>
.detail-slide-enter-active,
.detail-slide-leave-active {
  transition:
    transform 0.25s ease,
    opacity 0.25s ease;
}

.detail-slide-enter-from,
.detail-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
</style>
