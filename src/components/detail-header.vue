<script setup lang="ts">
import { computed } from 'vue';
import type { Article } from '@/types';
import StatusBadge from './status-badge.vue';
import { getPublicationTypeLabel } from '@/utils/formatters';

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
  requestTranslation: [id: string];
}>();

// Translation is in-flight when the queue reports queued/running. The status
// chip renders instead of the button in that case.
const isTranslationPending = computed(
  () =>
    props.article.translationStatus === 'queued' || props.article.translationStatus === 'running'
);
</script>

<template>
  <div class="p-6 border-b border-slate-100 sticky top-0 bg-white z-10">
    <div class="flex items-center justify-between mb-4">
      <div class="flex items-center gap-2">
        <span
          class="text-xs font-label-caps text-primary uppercase bg-primary/5 px-2 py-0.5 rounded"
        >
          Current {{ getPublicationTypeLabel(article.referenceType) }} Selection
        </span>
        <StatusBadge :status="article.status" />
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
        <!-- Translate icon (language-plan-v2 Phase 5) -->
        <button
          v-if="canRequestTranslation"
          class="material-symbols-outlined text-[18px] text-amber-600 hover:text-amber-700 hover:bg-amber-50 cursor-pointer rounded px-1 transition-colors"
          title="Translate to English"
          @click="emit('requestTranslation', article.id)"
        >
          translate
        </button>
        <!-- Translate disabled: eligible (non-English, not translated, not
             in-flight) but no LLM configured. Rendered as a disabled
             placeholder with a tooltip guiding the user to Settings. -->
        <button
          v-else-if="isTranslationEligible && !isLlmConfigured"
          type="button"
          disabled
          class="material-symbols-outlined text-[18px] text-slate-300 cursor-not-allowed rounded px-1"
          title="Configure an LLM provider in Settings to enable translations"
        >
          translate
        </button>
        <!-- Translation status chips (replace the button once translation starts/completes) -->
        <span
          v-else-if="isTranslationPending"
          class="inline-flex items-center gap-1 text-[11px] font-label-caps uppercase text-amber-700 bg-amber-50 px-1.5 py-0.5 rounded"
          :title="article.translationStatus === 'queued' ? 'Translation queued' : 'Translating…'"
        >
          <span class="material-symbols-outlined text-[14px] animate-spin">progress_activity</span>
          {{ article.translationStatus === 'queued' ? 'Translation Queued' : 'Translating' }}
        </span>
        <span
          v-else-if="article.isTranslated"
          class="inline-flex items-center gap-1 text-[11px] font-label-caps uppercase text-red-700 bg-red-50 px-1.5 py-0.5 rounded ring-1 ring-red-200"
          title="Article translated to English"
        >
          <span class="material-symbols-outlined text-[14px]">check_circle</span>
          Translated
        </span>
        <span
          v-else-if="article.translationStatus === 'failed'"
          class="inline-flex items-center gap-1 text-[11px] font-label-caps uppercase text-red-700 bg-red-50 px-1.5 py-0.5 rounded"
          :title="article.translationError ?? 'Translation failed'"
        >
          <span class="material-symbols-outlined text-[14px]">error</span>
          Translation Failed
        </span>
      </div>
      <div class="flex items-center gap-1">
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
    <h2 class="font-h1 text-h1 text-on-surface leading-tight">
      {{ article.title }}
    </h2>
  </div>
</template>
