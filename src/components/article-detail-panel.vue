<script setup lang="ts">
import { ref, computed, watch } from 'vue';
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
import { useLlmConfigured } from '@/composables/use-llm-configured';
import { useScreeningStore } from '@/stores/screening';
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
  screenArticle: [id: string];
  moveArticle: [id: string, newStatus: string];
  updateNotes: [id: string, notes: string];
  updateTags: [id: string, tagIds: string[]];
  updateLabels: [id: string, labelIds: string[]];
  updateCriteria: [id: string, inclusionIds: string[], exclusionIds: string[]];
  updateMetadata: [id: string, field: string, value: string | string[]];
  navigateToArticle: [id: string];
  toggleFullScreen: [];
  attachFullText: [id: string];
  deleteFullText: [id: string];
  readFullText: [id: string];
  refreshArticle: [id: string];
  articlePromoted: [articleId: string];
  readerOpened: [];
  referencesUpdated: [];
  /** Parent invokes `delete_article` + closes panel after user confirms. Dialog owned here. */
  deleteArticle: [id: string];
  /** Parent invokes `clear_ai_reasoning` + refreshes article after user confirms. Dialog owned here. */
  clearAiReasoning: [id: string];
}>();

const screeningStore = useScreeningStore();

/* Canonical LLM-configured gate: mirrors the backend `has_config` contract.
   Local providers (LM Studio / Ollama / llama.cpp) do not need an API key.
   Pre-warms the store via `fetchIfNeeded()`. */
const isLlmConfigured = useLlmConfigured();

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

/* Translation eligibility: parent owns eligibility + LLM-configured determination.
   DetailHeader receives two props so it can render the enabled button, the
   disabled-with-tooltip placeholder (eligible but no LLM), or hide the
   action entirely (English / already translated / in-flight). */
const ENGLISH_LANGUAGE_VALUES = new Set(['english', 'en', 'eng', 'engl']);

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

/* Article translation eligibility ignoring LLM gate. Used by child to decide
   "hide entirely" vs "disabled with configure-LLM tooltip". */
const isTranslationEligible = computed(() => {
  const a = props.article;
  if (a.isTranslated) return false;
  if (isEnglishLanguage(a.language)) return false;
  if (a.translationStatus === 'queued' || a.translationStatus === 'running') return false;
  return true;
});

/* Article is being screened per the global store. We match the article title
   against `currentArticleTitles` in the progress event payload. */
const isArticleBeingScreened = computed(
  () =>
    screeningStore.progress?.isRunning === true &&
    screeningStore.progress.currentArticleTitles.includes(props.article.title)
);

/* Local in-flight flag set synchronously on click (before progress events arrive)
   and cleared once the article prop reflects post-screening state. Closes two
   gaps: (1) click-time optimistic progress has empty `currentArticleTitles` so
   the button would flash enabled; (2) after run completes but before
   `refreshArticle` resolves, `props.article` still shows the pre-screening
   state so the button would briefly re-enable. */
const isScreening = ref(false);

// The combined disabled/spinner state: true from the click moment until the
// article prop is confirmed updated post-screening.
const isScreeningInProgress = computed(() => isScreening.value || isArticleBeingScreened.value);

/* Screen button shown when: article is working, unscreened, no error, LLM configured.
   Auto-hides when screening completes (status changes or `screenedAt` is set). */
const canScreenArticle = computed(
  () =>
    props.article.status === 'working' &&
    !props.article.screeningError &&
    !props.article.screenedAt &&
    isLlmConfigured.value
);

/* Reset local flag when article prop reflects post-screening state. This is
   the primary completion trigger, fired after `refreshArticle` resolves. */
watch(
  () => [props.article.status, props.article.screenedAt, props.article.screeningError] as const,
  ([status, screenedAt, screeningError]) => {
    if (isScreening.value && (status !== 'working' || screenedAt || screeningError)) {
      isScreening.value = false;
    }
  }
);

/* Backup trigger: if the global screening run ends but the article prop hasn't
   updated yet (e.g. refresh IPC is slow), re-emit `refreshArticle`. The watcher
   above clears `isScreening` once the prop eventually updates. */
watch(
  () => screeningStore.progress?.isRunning ?? false,
  (isRunning, wasRunning) => {
    if (isScreening.value && wasRunning === true && isRunning === false) {
      emit('refreshArticle', props.article.id);
    }
  }
);

// Determine the file type icon based on filename
const fullTextFileIcon = computed(() => getFullTextFileIcon(props.article.fullTextFileName));

/* Refresh the article when its screening run completes so status, AI decision,
   reasoning, and audit trail update live. Targets only the article that was
   screened, not every screening completion, so navigating between articles
   during an unrelated batch run does not cause unnecessary refreshes. */
watch(isArticleBeingScreened, (beingScreened, wasBeingScreened) => {
  if (wasBeingScreened === true && beingScreened === false) {
    emit('refreshArticle', props.article.id);
  }
});

/** Trigger AI summary generation */
function handleRequestAiSummary(): void {
  requestArticleAiSummary(props.article.id, props.article.title, async (articleId: string) => {
    emit('refreshArticle', articleId);
  });
}

/** Set the local in-flight flag synchronously so the spinner appears immediately,
 *  then emit `screenArticle` for the parent to invoke the backend command. */
function handleScreenClick(): void {
  isScreening.value = true;
  emit('screenArticle', props.article.id);
}

// Audit trail expand/collapse state
const auditExpanded = ref(false);

/* Panel resizing: the drag-shield overlay sits above the FullTextReader's PDF
   <iframe> during an active resize. Without it, an iframe swallows mouse events,
   `mouseup` never fires, and the resize gets permanently stuck. Two guards:
   1. `doResize` ends the drag on `mousemove` with `buttons === 0` (lost mouseup).
   2. `stopResize` is idempotent (stopped flag prevents double-invocation). */
const panelWidth = ref(parseInt(localStorage.getItem('bango-detail-panel-width') || '480'));
const isResizing = ref(false);

function startResize(e: MouseEvent): void {
  e.preventDefault();
  isResizing.value = true;
  const startX = e.clientX;
  const startWidth = panelWidth.value;
  let stopped = false;

  function stopResize(): void {
    if (stopped) return;
    stopped = true;
    isResizing.value = false;
    window.removeEventListener('mousemove', doResize);
    window.removeEventListener('mouseup', stopResize);
    document.body.style.cursor = '';
  }

  function doResize(moveEvent: MouseEvent): void {
    /* Safety net: if mouse button is no longer pressed but we never saw mouseup
       (e.g. cursor left window mid-drag), end resize so listener doesn't stay active. */
    if (moveEvent.buttons === 0) {
      stopResize();
      return;
    }
    const delta = startX - moveEvent.clientX;
    const newWidth = Math.max(320, Math.min(900, startWidth + delta));
    panelWidth.value = newWidth;
    localStorage.setItem('bango-detail-panel-width', newWidth.toString());
  }

  window.addEventListener('mousemove', doResize);
  window.addEventListener('mouseup', stopResize);
  document.body.style.cursor = 'col-resize';
}

// Full-text reader ref for programmatic open
const fullTextReaderRef = ref<InstanceType<typeof FullTextReader> | null>(null);

/* Translation UI orchestration + article-delete confirmation dialog.
   Both dialogs owned here so the parent only handles the final emit. */
const showDeleteDialog = ref(false);

/* Clear-AI-reasoning confirmation dialog. AiDecisionCard emits `clearReasoning`,
   this component shows confirmation, on confirm emits `clearAiReasoning`. */
const showClearReasoningDialog = ref(false);

const {
  showTranslateDialog,
  translateArticleTitle,
  requestTranslation,
  confirmTranslation,
  cancelTranslation,
} = useTranslation({
  onTranslationQueued: (articleId) => {
    /* Refresh immediately after enqueue so badge flips to "Translation Queued"
       without waiting for `translation:complete` (can take minutes for FT jobs). */
    emit('refreshArticle', articleId);
  },
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
    <!-- Resize Handle (desktop only, hidden in fullscreen).
         Sits ABOVE the FullTextReader overlay (z-50) so the panel stays
         resizable while the PDF/text reader is open - without this the
         overlay swallows the drag events and the panel cannot be resized
         until the reader is closed. The persisted width applies to the
         whole detail panel and survives closing the reader. -->
    <div
      v-if="!fullScreen"
      class="resizer hidden lg:block absolute left-0 top-0 bottom-0 w-1.5 cursor-col-resize z-[60] hover:bg-indigo-400/50 active:bg-indigo-600 transition-colors"
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
      @delete-article="showDeleteDialog = true"
      @update-title="(title) => emit('updateMetadata', article.id, 'title', title)"
    />

    <!-- Scrollable Content.
         In full-screen the panel chrome fills the viewport width; the article
         body is centered in a readable column so wide viewports stay legible. -->
    <div
      class="flex-1 overflow-y-auto p-6 space-y-8"
      :class="fullScreen ? 'max-w-[1100px] mx-auto' : ''"
    >
      <!-- AI Decision Card. Wrapped in a <Transition> so clearing the AI
           decision (trashcan -> confirm -> backend nulls `ai_decision`)
           animates the card shrinking + fading out instead of vanishing
           instantly. The enter transition (re-screening produces a new
           decision) mirrors the leave for visual symmetry. -->
      <Transition name="ai-card">
        <AiDecisionCard
          v-if="article.aiDecision"
          :article="article"
          @clear-reasoning="showClearReasoningDialog = true"
        />
      </Transition>

      <!-- Metadata (Authors, Journal, Year, DOI, Keywords) -->
      <ArticleMetadata
        :article="article"
        @update-field="(field, value) => emit('updateMetadata', article.id, field, value)"
      />

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

    <!-- Delete-article confirmation dialog. Mirrors the Translation dialog
         shape (Teleported to body, `.dialog--danger` + `.dialog__danger-box`).
         The body enumerates every related record class that will be removed so
         the user understands the blast radius before confirming. The parent
         closes the detail panel after the backend delete returns, so this
         component just emits `deleteArticle` and lets the parent drive the
         list + panel teardown. -->
    <Teleport to="body">
      <div v-if="showDeleteDialog" class="dialog-overlay" @click.self="showDeleteDialog = false">
        <div class="dialog dialog--danger">
          <h2>Delete Article</h2>
          <div class="dialog__danger-box">
            <span class="material-symbols-outlined">warning</span>
            <p>
              This will <strong>permanently delete</strong> the article and
              <strong>all related records</strong>. The audit history, user notes, AI summary, full
              text + extracted chunks, translation archive, and dedup links will be removed.
              Reference links to papers that no other article uses will also be deleted. This action
              <strong>cannot be undone</strong>.
            </p>
          </div>
          <div class="dialog__desc">
            <p>
              Article: <code>{{ article.title }}</code>
            </p>
          </div>
          <div class="dialog__actions">
            <button class="btn btn--outline" @click="showDeleteDialog = false">Cancel</button>
            <button
              class="btn btn--danger"
              @click="
                showDeleteDialog = false;
                emit('deleteArticle', article.id);
              "
            >
              Delete Article
            </button>
          </div>
        </div>
      </div>
    </Teleport>

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

    <!-- Clear-AI-reasoning confirmation dialog. Mirrors the delete-article +
         translation dialog shape (Teleported to body) but uses an info style
         rather than danger: the action is destructive only for the reasoning
         text + confidence; the decision, status, and screening history stay
         intact. Restoring the reasoning requires re-screening (LLM token
         cost), which is the warning the body emphasizes. -->
    <Teleport to="body">
      <div
        v-if="showClearReasoningDialog"
        class="dialog-overlay"
        @click.self="showClearReasoningDialog = false"
      >
        <div class="dialog">
          <h2>Delete AI Reasoning</h2>
          <div class="dialog__desc">
            <p>
              This will remove the AI <strong>decision</strong>, <strong>reasoning text</strong>,
              and <strong>confidence score</strong>
              from this article. The AI Decision card will be hidden.
            </p>
            <p class="mt-2">
              Your own Include / Exclude choice (the article's status) is
              <strong>not affected</strong>. Restoring the AI assessment requires
              <strong>re-screening</strong> the article, which has an LLM token cost. This action
              <strong>cannot be undone</strong>.
            </p>
            <p class="mt-3">
              Article: <code>{{ article.title }}</code>
            </p>
          </div>
          <div class="dialog__actions">
            <button class="btn btn--outline" @click="showClearReasoningDialog = false">
              Cancel
            </button>
            <button
              class="btn btn--danger"
              @click="
                showClearReasoningDialog = false;
                emit('clearAiReasoning', article.id);
              "
            >
              Delete AI Decision
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Drag shield: a transparent full-viewport overlay rendered only during
         an active resize. Without this, the FullTextReader's PDF <iframe>
         (a separate document) swallows mousemove/mouseup events that land on
         it, so a drag that crosses the iframe would never see its mouseup and
         the resize would get permanently stuck (the panel would then track
         every mouse movement, including clicks in the article table). The
         shield forces those events to the parent document. Teleported to
         <body> + position: fixed so it also covers the article table and any
         other sibling outside this <aside>. -->
    <Teleport to="body">
      <div
        v-if="isResizing"
        class="fixed inset-0 z-[9999] cursor-col-resize"
        data-testid="drag-shield"
      />
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
        <!-- Screen button: visible only for working + unscreened articles when
             LLM is configured. Shows a spinner when screening is in progress for
             this article. Auto-hides when screening completes (status changes to
             included/rejected/error or screenedAt gets set). -->
        <button
          v-if="canScreenArticle"
          :disabled="isScreeningInProgress"
          class="inline-flex items-center gap-1.5 bg-indigo-600 text-white px-4 py-2 rounded-lg font-semibold text-sm hover:bg-indigo-700 active:scale-95 transition-all shadow-sm cursor-pointer disabled:opacity-60 disabled:cursor-wait"
          :title="
            isScreeningInProgress
              ? 'Screening in progress...'
              : 'Submit this article to the AI screening pipeline'
          "
          @click="handleScreenClick"
        >
          <span
            v-if="isScreeningInProgress"
            class="inline-block w-3.5 h-3.5 border-2 border-white/40 border-t-white rounded-full animate-spin"
          />
          <span v-else class="material-symbols-outlined text-[16px]">psychology</span>
          Screen
        </button>
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

/* AI Decision card enter/leave transition.
   - Leave (the primary path you asked for): the card shrinks (max-height +
     margin + padding to 0) and fades out over 250ms, producing the
     "shrink and vanish" effect when `ai_decision` is cleared. Using
     `max-height` (with a generous cap) avoids measuring the element's
     natural height. `overflow: hidden` during the transition prevents the
     collapsing content from overflowing the parent's `space-y-8` gap.
   - Enter (re-screening produces a new decision): the reverse - fade +
     expand from 0 so a freshly-screened decision does not pop in. Mirrors
     the leave for visual symmetry. */
.ai-card-enter-active {
  transition:
    max-height 0.25s ease-out,
    opacity 0.25s ease-out,
    margin 0.25s ease-out;
  overflow: hidden;
}
.ai-card-leave-active {
  transition:
    max-height 0.25s ease-in,
    opacity 0.25s ease-in,
    margin 0.25s ease-in,
    padding 0.25s ease-in;
  overflow: hidden;
}
.ai-card-enter-from {
  max-height: 0;
  opacity: 0;
  margin-top: 0;
  margin-bottom: 0;
}
.ai-card-leave-to {
  max-height: 0;
  opacity: 0;
  margin-top: 0;
  margin-bottom: 0;
  padding-top: 0;
  padding-bottom: 0;
}
/* Generous max-height cap for the expanded state so the collapse animation
   runs smoothly without needing the exact rendered height. The AI Decision
   card is well under this cap in practice. */
.ai-card-enter-to,
.ai-card-leave-from {
  max-height: 800px;
  opacity: 1;
}
</style>
