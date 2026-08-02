<script setup lang="ts">
import { ref, computed, nextTick, onMounted, watch } from 'vue';
import type { Article } from '@/types';
import StatusBadge from './status-badge.vue';
import { getPublicationTypeLabel } from '@/utils/formatters';
import { invoke } from '@tauri-apps/api/core';

const props = defineProps<{
  article: Article;
  canRequestAiSummary: boolean;
  isAiSummaryPending: boolean;
  hasReturnTarget: boolean;
  fullScreen: boolean;
  fullTextFileIcon?: string | null;
  /** Whether an LLM provider is configured. Used only to decide between the
   *  disabled-with-tooltip placeholder and hiding the action entirely. The
   *  real determination of "can the user translate right now" lives in the
   *  parent via `canRequestTranslation`, mirroring the `canRequestAiSummary`
   *  pattern: the parent owns `isLlmConfigured` centrally and each button
   *  adds only its own eligibility details. */
  isLlmConfigured?: boolean;
  /** Whether the manual translate action is actionable right now (article
   *  eligible AND LLM configured). Owned by the parent. */
  canRequestTranslation?: boolean;
  /** Whether the article is eligible for translation ignoring the LLM gate
   *  (non-English, not translated, not queued/running). Owned by the parent.
   *  When false the action is hidden entirely; when true but
   *  `canRequestTranslation` is false, the disabled-with-tooltip placeholder
   *  renders so the user gets the "configure LLM" hover hint. */
  isTranslationEligible?: boolean;
}>();

const emit = defineEmits<{
  close: [];
  toggleFullScreen: [];
  attachFullText: [id: string];
  requestAiSummary: [];
  readFullText: [];
  requestTranslate: [id: string];
  deleteArticle: [];
  /** Emitted when the user commits an inline title edit (Enter or blur).
   *  The parent routes this to the `update_article_metadata` IPC with
   *  `field = 'title'`. Empty/whitespace drafts are blocked before emit. */
  updateTitle: [title: string];
}>();

// Translation is in-flight when the queue reports queued/running. The status
// chip renders instead of the button in that case.
const isTranslationPending = computed(
  () =>
    props.article.translationStatus === 'queued' || props.article.translationStatus === 'running'
);

// ── Inline title editing (double-click) ────────────────────────────────
// Mirrors the proven pattern in `article-metadata.vue` (v6.9): at most one
// field edited at a time, `nextTick` focus + select on edit-start, Enter
// commits, Escape cancels, blur commits. Title is `TEXT NOT NULL` so empty
// drafts are blocked with a red hint (matches the Year validation gate).
const isEditingTitle = ref(false);
const titleDraft = ref('');
const titleError = ref<string | null>(null);
const titleTextareaRef = ref<HTMLTextAreaElement | null>(null);

function startEditTitle(): void {
  isEditingTitle.value = true;
  titleDraft.value = props.article.title;
  titleError.value = null;
  void nextTick(() => {
    const el = titleTextareaRef.value;
    el?.focus();
    el?.select();
  });
}

function commitTitle(): void {
  const trimmed = titleDraft.value.trim();
  if (trimmed === '') {
    titleError.value = 'Title cannot be empty';
    void nextTick(() => {
      titleTextareaRef.value?.focus();
    });
    return;
  }
  // No-op if unchanged: close the editor without an IPC round-trip.
  if (trimmed === props.article.title) {
    isEditingTitle.value = false;
    titleDraft.value = '';
    titleError.value = null;
    return;
  }
  emit('updateTitle', trimmed);
  isEditingTitle.value = false;
  titleDraft.value = '';
  titleError.value = null;
}

function cancelTitle(): void {
  isEditingTitle.value = false;
  titleDraft.value = '';
  titleError.value = null;
}

// ── Original title (pre-translation) ──────────────────────────────────
// Fetched on-demand only for translated articles so untranslated ones
// incur zero DB cost. The original title is stored in
// `article_original_content` and surfaced here in brackets alongside the
// translated English title.
const originalTitle = ref<string | null>(null);

async function fetchOriginalTitle(): Promise<void> {
  if (!props.article.isTranslated) {
    originalTitle.value = null;
    return;
  }
  try {
    originalTitle.value = await invoke<string | null>('get_original_title', {
      articleId: props.article.id,
    });
  } catch {
    originalTitle.value = null;
  }
}

onMounted(() => {
  void fetchOriginalTitle();
});

// Re-fetch when navigating to a different article in the same panel.
watch(
  () => props.article.id,
  () => {
    void fetchOriginalTitle();
  }
);
</script>

<template>
  <div class="p-6 border-b border-slate-100 sticky top-0 bg-white z-10">
    <div class="flex items-center justify-between mb-4">
      <div class="flex items-center gap-2">
        <span
          class="text-xs font-label-caps text-primary uppercase bg-primary/5 px-2 py-0.5 rounded"
        >
          {{ getPublicationTypeLabel(article.referenceType).toUpperCase() }}
        </span>
        <StatusBadge :status="article.status" />
        <!-- Translation status chips (after status badge; pill-shaped to
             match StatusBadge). Mutually exclusive by data state. -->
        <span
          v-if="isTranslationPending"
          class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-semibold uppercase tracking-tight text-amber-700 bg-amber-50"
          :title="article.translationStatus === 'queued' ? 'Translation queued' : 'Translating…'"
        >
          <span class="material-symbols-outlined text-[14px] animate-spin">progress_activity</span>
          {{ article.translationStatus === 'queued' ? 'Translation Queued' : 'Translating' }}
        </span>
        <span
          v-else-if="article.isTranslated"
          class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-semibold uppercase tracking-tight text-red-700 bg-red-50 ring-1 ring-red-200"
          title="Article translated to English"
        >
          <span class="material-symbols-outlined text-[14px]">check_circle</span>
          Translated
        </span>
        <span
          v-else-if="article.translationStatus === 'failed'"
          class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-semibold uppercase tracking-tight text-red-700 bg-red-50"
          :title="article.translationError ?? 'Translation failed'"
        >
          <span class="material-symbols-outlined text-[14px]">error</span>
          Translation Failed
        </span>
        <!-- Action icons (after status badge) -->
        <!-- Translate icon (language-plan-v2 Phase 5) -->
        <button
          v-if="canRequestTranslation"
          class="material-symbols-outlined text-[18px] text-amber-600 hover:text-amber-700 hover:bg-amber-50 cursor-pointer rounded px-1 transition-colors"
          title="Translate to English"
          @click="emit('requestTranslate', article.id)"
        >
          translate
        </button>
        <button
          v-else-if="isTranslationEligible && !isLlmConfigured"
          type="button"
          disabled
          class="material-symbols-outlined text-[18px] text-slate-300 cursor-not-allowed rounded px-1"
          title="Configure an LLM provider in Settings to enable translations"
        >
          translate
        </button>
        <!-- Full-text attachment icon -->
        <button
          v-if="article.hasFullText && fullTextFileIcon"
          class="material-symbols-outlined text-[18px] cursor-pointer rounded px-1 transition-colors"
          :class="
            article.fullTextFileName?.toLowerCase().endsWith('.pdf')
              ? 'text-red-500 hover:bg-red-50'
              : 'text-blue-500 hover:bg-blue-50'
          "
          :title="'Open full text: ' + (article.fullTextFileName ?? '')"
          @click="emit('readFullText')"
        >
          {{ fullTextFileIcon }}
        </button>
        <button
          v-else
          class="material-symbols-outlined text-[18px] text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer rounded px-1 transition-colors"
          title="Attach full text (PDF or TXT)"
          @click="emit('attachFullText', article.id)"
        >
          attach_file
        </button>
        <!-- AI Summary icon -->
        <button
          v-if="canRequestAiSummary"
          class="material-symbols-outlined text-[18px] text-violet-500 hover:text-violet-700 hover:bg-violet-50 cursor-pointer rounded px-1 transition-colors animate-pulse"
          title="Generate AI summary from full text"
          @click="emit('requestAiSummary')"
        >
          auto_awesome
        </button>
        <span
          v-else-if="isAiSummaryPending"
          class="material-symbols-outlined text-[18px] text-violet-400 animate-spin px-1"
          title="AI summary in progress..."
        >
          progress_activity
        </span>
      </div>
      <div class="flex items-center gap-1">
        <!-- Delete article icon: grey at rest, turns red on hover to signal
             the destructive action. Placed left of the fullscreen toggle so
             the cluster reads: delete -> fullscreen -> close. The actual
             deletion is gated by a confirmation dialog in article-detail-panel. -->
        <button
          class="material-symbols-outlined text-slate-400 hover:text-rose-600 hover:bg-rose-50 transition-colors cursor-pointer rounded px-1"
          title="Delete this article and all related records."
          @click="emit('deleteArticle')"
        >
          delete
        </button>
        <button
          class="material-symbols-outlined text-slate-400 hover:text-slate-900 transition-colors cursor-pointer"
          title="Toggle full screen"
          @click="emit('toggleFullScreen')"
        >
          {{ fullScreen ? 'close_fullscreen' : 'open_in_full' }}
        </button>
        <button
          class="material-symbols-outlined text-slate-400 hover:text-slate-900 transition-colors cursor-pointer"
          :title="hasReturnTarget ? 'Return to previous article' : 'Close detail panel'"
          @click="emit('close')"
        >
          {{ hasReturnTarget ? 'arrow_back' : 'close' }}
        </button>
      </div>
    </div>
    <!-- Title: double-click to edit in place (no edit icon; the hover affordance
         + tooltip communicate editability, matching the Metadata card spans).
         Title is `TEXT NOT NULL` so empty drafts are blocked with a red hint. -->
    <div v-if="isEditingTitle" class="flex flex-col gap-1">
      <textarea
        ref="titleTextareaRef"
        v-model="titleDraft"
        rows="2"
        class="font-h1 text-h1 text-on-surface leading-tight font-semibold w-full px-2 py-1 bg-white border rounded transition-all focus:ring-1 resize-none"
        :class="
          titleError
            ? 'border-rose-400 focus:border-rose-500 focus:ring-rose-500'
            : 'border-primary focus:border-primary focus:ring-primary'
        "
        placeholder="Article title"
        @keyup.enter="commitTitle"
        @keyup.escape="cancelTitle"
        @blur="commitTitle"
      ></textarea>
      <span v-if="titleError" class="text-[11px] text-rose-600 leading-tight">{{
        titleError
      }}</span>
    </div>
    <h2
      v-else
      class="font-h1 text-h1 text-on-surface leading-tight font-semibold cursor-text hover:bg-slate-50 rounded px-1 -mx-1 transition-colors"
      title="Double-click to edit"
      @dblclick="startEditTitle"
    >
      {{ article.title
      }}<span
        v-if="originalTitle && originalTitle !== article.title"
        class="text-slate-400 text-base font-normal"
      >
        ({{ originalTitle }})
      </span>
    </h2>
  </div>
</template>
