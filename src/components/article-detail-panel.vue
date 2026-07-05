<script setup lang="ts">
import { ref, computed } from 'vue';
import type { Article, AuditEntry } from '@/types';
import AuditTimeline from './audit-timeline.vue';
import DetailHeader from './detail-header.vue';
import AiDecisionCard from './ai-decision-card.vue';
import ArticleMetadata from './article-metadata.vue';
import MatchedCriteria from './matched-criteria.vue';
import AbstractSummaryView from './abstract-summary-view.vue';
import TagsSection from './tags-section.vue';
import LabelsSection from './labels-section.vue';
import ArticleNotes from './article-notes.vue';
import ArticleReferences from './article-references.vue';
import FullTextReader from './full-text-reader.vue';
import { useLlmConfigStore } from '@/stores/llm-config';
import {
  requestArticleAiSummary,
  parseAiSummary,
  pendingSummaries,
} from '@/composables/use-ai-summary';
import type { AiSummaryData } from '@/composables/use-ai-summary';
import { useTranslation } from '@/composables/use-translation';
import { getFullTextFileIcon } from '@/utils/formatters';

const props = defineProps<{
  article: Article;
  auditTrail: AuditEntry[];
  hasPrevious: boolean;
  hasNext: boolean;
  hasReturnTarget: boolean;
  fullScreen?: boolean;
  articlePosition: number;
  articleTotal: number;
  decisionMessage?: string;
  decisionType?: 'success' | 'info';
  openReaderId?: string | null;
}>();

const emit = defineEmits<{
  close: [];
  navigatePrev: [];
  navigateNext: [];
  moveArticle: [id: string, newStatus: string];
  updateNotes: [id: string, notes: string];
  updateTags: [id: string, tagIds: string[]];
  updateLabels: [id: string, labelIds: string[]];
  updateCriteria: [id: string, inclusionIds: string[], exclusionIds: string[]];
  navigateToArticle: [id: string];
  toggleFullScreen: [];
  attachFullText: [id: string];
  deleteFullText: [id: string];
  readFullText: [id: string];
  refreshArticle: [id: string];
  articlePromoted: [articleId: string];
  readerOpened: [];
  referencesUpdated: [];
}>();

const llmConfigStore = useLlmConfigStore();

// Ensure store is loaded
void llmConfigStore.fetchIfNeeded();

// Whether an LLM provider is configured and ready. Delegates to the store
// getter, which mirrors the backend `llm_config_repo::has_config` contract
// (local providers like LM Studio / Ollama / llama.cpp do not need a key).
const isLlmConfigured = computed(() => llmConfigStore.isConfigured);

// Parsed AI summary data
const aiSummaryData = computed<AiSummaryData | null>(() =>
  parseAiSummary(props.article.fullTextAiSummary)
);

// Whether we can request an AI summary (has full text, LLM configured, no summary yet)
const canRequestAiSummary = computed(
  () =>
    !!props.article.hasFullText &&
    !!props.article.fullText &&
    isLlmConfigured.value &&
    !aiSummaryData.value &&
    !pendingSummaries.value.has(props.article.id)
);

// Whether an AI summary is pending for this article
const isAiSummaryPending = computed(() => pendingSummaries.value.has(props.article.id));

// ---- Translation eligibility (mirrors the canRequestAiSummary pattern) ----
// Parent owns the full determination: article eligibility + LLM configured.
// The child (DetailHeader) receives two props so it can render the enabled
// button, the disabled-with-tooltip placeholder (eligible but no LLM), or
// hide the action entirely (English / already translated / in-flight).
const ENGLISH_LANGUAGE_VALUES = new Set(['english', 'en']);

const isEnglishLanguage = (language: string | null | undefined): boolean => {
  if (!language) return true; // absent/blank treated as English (no translation)
  return ENGLISH_LANGUAGE_VALUES.has(language.trim().toLowerCase());
};

// Whether the manual translate action is actionable right now (eligible +
// LLM configured). Structurally identical to `canRequestAiSummary`.
const canRequestTranslation = computed(() => {
  const a = props.article;
  if (a.isTranslated) return false;
  if (isEnglishLanguage(a.language)) return false;
  if (a.translationStatus === 'queued' || a.translationStatus === 'running') return false;
  return isLlmConfigured.value;
});

// Whether an article is eligible for translation ignoring the LLM gate. Used
// by the child to decide between "hide entirely" (not eligible) and
// "disabled with configure-LLM tooltip" (eligible but no LLM).
const isTranslationEligible = computed(() => {
  const a = props.article;
  if (a.isTranslated) return false;
  if (isEnglishLanguage(a.language)) return false;
  if (a.translationStatus === 'queued' || a.translationStatus === 'running') return false;
  return true;
});

// Determine the file type icon based on filename
const fullTextFileIcon = computed(() => getFullTextFileIcon(props.article.fullTextFileName));

/** Trigger AI summary generation */
function handleRequestAiSummary(): void {
  requestArticleAiSummary(props.article.id, props.article.title, async (articleId: string) => {
    emit('refreshArticle', articleId);
  });
}

// Audit trail expand/collapse state
const auditExpanded = ref(false);

// Panel resizing logic
const panelWidth = ref(parseInt(localStorage.getItem('bango-detail-panel-width') || '480'));
const isResizing = ref(false);

function startResize(e: MouseEvent): void {
  e.preventDefault();
  isResizing.value = true;
  const startX = e.clientX;
  const startWidth = panelWidth.value;

  function doResize(moveEvent: MouseEvent): void {
    const delta = startX - moveEvent.clientX;
    const newWidth = Math.max(320, Math.min(900, startWidth + delta));
    panelWidth.value = newWidth;
    localStorage.setItem('bango-detail-panel-width', newWidth.toString());
  }

  function stopResize(): void {
    isResizing.value = false;
    window.removeEventListener('mousemove', doResize);
    window.removeEventListener('mouseup', stopResize);
    document.body.style.cursor = '';
  }

  window.addEventListener('mousemove', doResize);
  window.addEventListener('mouseup', stopResize);
  document.body.style.cursor = 'col-resize';
}

// Full-text reader ref for programmatic open
const fullTextReaderRef = ref<InstanceType<typeof FullTextReader> | null>(null);

// Translation UI orchestration (language-plan-v2 Phase 5): owns the
// confirmation-dialog state, the enqueue invoke, the immediate toast, and the
// global `translation:complete` listener that refreshes this article.
const {
  showTranslateDialog,
  translateArticleTitle,
  requestTranslation,
  confirmTranslation,
  cancelTranslation,
} = useTranslation({
  onTranslationComplete: (articleId) => {
    // Refresh the article so the header chip flips to "Translated".
    emit('refreshArticle', articleId);
  },
});
</script>

<template>
  <aside
    class="detail-panel h-full bg-white flex flex-col z-50 relative"
    :class="{
      'transition-none': isResizing,
      'detail-panel--fullscreen': fullScreen,
      'shadow-[0_4px_24px_rgba(0,0,0,0.15)] border-l border-slate-200': !fullScreen,
    }"
    :style="fullScreen ? {} : { '--detail-panel-width': panelWidth + 'px' }"
  >
    <!-- Resize Handle (desktop only, hidden in fullscreen) -->
    <div
      v-if="!fullScreen"
      class="resizer hidden lg:block absolute left-0 top-0 bottom-0 w-1.5 cursor-col-resize z-50 hover:bg-indigo-400/50 active:bg-indigo-600 transition-colors"
      @mousedown="startResize"
    />

    <!-- Header -->
    <DetailHeader
      :article="article"
      :has-return-target="hasReturnTarget"
      :full-screen="!!fullScreen"
      :can-request-ai-summary="canRequestAiSummary"
      :is-ai-summary-pending="isAiSummaryPending"
      :full-text-file-icon="fullTextFileIcon"
      :is-llm-configured="isLlmConfigured"
      :can-request-translation="canRequestTranslation"
      :is-translation-eligible="isTranslationEligible"
      @toggle-full-screen="emit('toggleFullScreen')"
      @close="emit('close')"
      @read-full-text="fullTextReaderRef?.openFullTextView()"
      @attach-full-text="emit('attachFullText', article.id)"
      @request-ai-summary="handleRequestAiSummary"
      @request-translate="requestTranslation(article.id, article.title)"
    />

    <!-- Scrollable Content -->
    <div class="flex-1 overflow-y-auto p-6 space-y-8">
      <!-- AI Decision Card -->
      <AiDecisionCard v-if="article.aiDecision" :article="article" />

      <!-- Metadata (Authors, Journal, Year, DOI, Keywords) -->
      <ArticleMetadata :article="article" />

      <!-- Matched Criteria -->
      <MatchedCriteria
        :article="article"
        @update-criteria="(id, inc, exc) => emit('updateCriteria', id, inc, exc)"
      />

      <!-- Abstract / AI Summary Tabbed View -->
      <AbstractSummaryView :article="article" @refresh-article="emit('refreshArticle', $event)" />

      <!-- Tags -->
      <TagsSection :article="article" @update-tags="(id, tags) => emit('updateTags', id, tags)" />

      <!-- Labels -->
      <LabelsSection
        :article="article"
        @update-labels="(id, labels) => emit('updateLabels', id, labels)"
      />

      <!-- Notes (imported + user) -->
      <ArticleNotes
        :article="article"
        @update-notes="(id, notes) => emit('updateNotes', id, notes)"
      />

      <!-- References (tabbed) -->
      <ArticleReferences
        :article="article"
        @navigate-to-article="emit('navigateToArticle', $event)"
        @article-promoted="emit('articlePromoted', $event)"
        @references-updated="emit('referencesUpdated')"
      />

      <!-- Audit Trail -->
      <section>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-xs font-label-caps text-slate-500 uppercase tracking-wider">
            Audit Trail
          </h3>
          <button
            class="material-symbols-outlined text-[18px] text-slate-400 hover:text-slate-700 cursor-pointer transition-colors"
            @click="auditExpanded = !auditExpanded"
          >
            {{ auditExpanded ? 'expand_less' : 'expand_more' }}
          </button>
        </div>
        <template v-if="auditExpanded">
          <AuditTimeline
            :entries="auditTrail"
            :show-header="false"
            @navigate-to-article="emit('navigateToArticle', $event)"
          />
        </template>
      </section>

      <div class="pb-10" />
    </div>

    <!-- Inline Decision Notification -->
    <Transition name="decision-toast">
      <div
        v-if="decisionMessage"
        class="px-4 py-2 text-center text-sm font-semibold text-white"
        :class="decisionType === 'info' ? 'bg-blue-500' : 'bg-emerald-500'"
      >
        {{ decisionMessage }}
      </div>
    </Transition>

    <!-- Full-Text Reading View Overlay -->
    <FullTextReader
      ref="fullTextReaderRef"
      :article="article"
      :full-screen="!!fullScreen"
      :can-request-ai-summary="canRequestAiSummary"
      :is-ai-summary-pending="isAiSummaryPending"
      :open-reader-id="openReaderId ?? null"
      @toggle-full-screen="emit('toggleFullScreen')"
      @delete-full-text="emit('deleteFullText', $event)"
      @refresh-article="emit('refreshArticle', $event)"
      @reader-opened="emit('readerOpened')"
    />

    <!-- Translation Confirmation Dialog (language-plan-v2 Phase 5).
         Teleported to <body> so it escapes the detail panel's transform
         (mobile slide-in animation) + z-index stacking context, mirroring the
         criteria-edit-dialog.vue precedent. Without Teleport the fixed-position
         dialog is trapped inside the transformed <aside> and never appears. -->
    <Teleport to="body">
      <div v-if="showTranslateDialog" class="dialog-overlay" @click.self="cancelTranslation">
        <div class="dialog dialog--danger">
          <h2>Translate Article</h2>
          <div class="dialog__danger-box">
            <span class="material-symbols-outlined">warning</span>
            <p>
              This will <strong>permanently rewrite</strong> the article text (title, abstract, and
              full text) to English so AI screening and summaries can process it. The original
              non-English text is preserved in the originals archive. This action has a
              <strong>high token cost</strong> and cannot be undone without re-importing the
              article.
            </p>
          </div>
          <div class="dialog__desc">
            <p>
              Article: <code>{{ translateArticleTitle }}</code>
            </p>
          </div>
          <div class="dialog__actions">
            <button class="btn btn--outline" @click="cancelTranslation">Cancel</button>
            <button class="btn btn--danger" @click="confirmTranslation">
              Translate to English
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Footer Actions -->
    <div class="p-4 border-t border-slate-100 flex gap-3 bg-slate-50/50 items-center">
      <!-- Left: Navigation -->
      <div class="flex items-center gap-1 shrink-0">
        <button
          class="material-symbols-outlined p-1 rounded-lg transition-colors"
          :class="
            hasPrevious
              ? 'text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer'
              : 'text-slate-200 cursor-not-allowed'
          "
          :title="hasPrevious ? 'Previous article' : 'No previous article'"
          @click="hasPrevious && emit('navigatePrev')"
        >
          chevron_left
        </button>
        <span class="text-xs text-slate-500 font-medium tabular-nums min-w-[4rem] text-center">
          {{ articlePosition }} of {{ articleTotal }}
        </span>
        <button
          class="material-symbols-outlined p-1 rounded-lg transition-colors"
          :class="
            hasNext
              ? 'text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer'
              : 'text-slate-200 cursor-not-allowed'
          "
          :title="hasNext ? 'Next article' : 'No next article'"
          @click="hasNext && emit('navigateNext')"
        >
          chevron_right
        </button>
      </div>
      <!-- Right: Action buttons -->
      <div class="flex gap-3 flex-1 justify-end">
        <button
          v-if="article.status !== 'included'"
          class="bg-emerald-600 text-white px-4 py-2 rounded-lg font-semibold text-sm hover:bg-emerald-700 active:scale-95 transition-all shadow-sm cursor-pointer"
          title="Include this article in your systematic review"
          @click="emit('moveArticle', article.id, 'included')"
        >
          Include
        </button>
        <button
          v-if="article.status !== 'rejected'"
          class="bg-white border border-slate-200 text-rose-700 px-4 py-2 rounded-lg font-semibold text-sm hover:bg-rose-50 transition-colors shadow-sm cursor-pointer"
          title="Reject this article from your systematic review"
          @click="emit('moveArticle', article.id, 'rejected')"
        >
          Reject
        </button>
        <button
          v-if="article.status !== 'working'"
          class="bg-white border border-slate-200 text-slate-700 px-4 py-2 rounded-lg font-semibold text-sm hover:bg-slate-50 transition-colors shadow-sm cursor-pointer"
          title="Move this article back to Working status"
          @click="emit('moveArticle', article.id, 'working')"
        >
          Move to Working
        </button>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.detail-panel {
  width: var(--detail-panel-width);
  flex-shrink: 0;
  transition: width 0.2s ease;
}

.detail-panel--fullscreen {
  width: 100%;
  flex-shrink: 1;
  max-width: 960px;
  margin: 0 auto;
  border-left: none;
  box-shadow: none;
}

@media (max-width: 1023px) {
  .detail-panel {
    position: fixed;
    top: 0;
    right: 0;
    width: 100%;
    max-width: 100%;
    height: 100vh;
    border-left: none;
    z-index: 60;
    animation: slideInRight 0.25s ease;
  }
}

@keyframes slideInRight {
  from {
    transform: translateX(100%);
  }
  to {
    transform: translateX(0);
  }
}

/* Inline decision toast animation */
.decision-toast-enter-active {
  transition: all 0.3s ease-out;
}
.decision-toast-leave-active {
  transition: all 0.25s ease-in;
}
.decision-toast-enter-from {
  transform: translateX(100%);
  opacity: 0;
}
.decision-toast-leave-to {
  opacity: 0;
}
</style>
