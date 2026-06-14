<script setup lang="ts">
import type { Article } from '@/types';
import StatusBadge from './status-badge.vue';
import { getPublicationTypeLabel } from '@/utils/formatters';

defineProps<{
  article: Article;
  canRequestAiSummary: boolean;
  isAiSummaryPending: boolean;
  hasReturnTarget: boolean;
  fullScreen: boolean;
  fullTextFileIcon?: string | null;
}>();

const emit = defineEmits<{
  close: [];
  toggleFullScreen: [];
  attachFullText: [id: string];
  requestAiSummary: [];
  readFullText: [];
}>();
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
