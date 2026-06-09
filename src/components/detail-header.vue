<script setup lang="ts">
import { ref } from 'vue';
import type { Article } from '@/types';
import StatusBadge from './status-badge.vue';
import { useToast } from '@/composables/use-toast';

const props = defineProps<{
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

const toast = useToast();

// Metadata expand/collapse state (persisted)
const metadataExpanded = ref(localStorage.getItem('bango-metadata-expanded') !== 'false');
function toggleMetadata(): void {
  metadataExpanded.value = !metadataExpanded.value;
  localStorage.setItem('bango-metadata-expanded', String(metadataExpanded.value));
}

function copyDoi(): void {
  if (!props.article.doi) return;
  navigator.clipboard.writeText(props.article.doi).then(() => {
    toast.show('DOI copied to clipboard', 'success', 2000);
  });
}
</script>

<template>
  <div class="p-6 border-b border-slate-100 sticky top-0 bg-white z-10">
    <div class="flex items-center justify-between mb-4">
      <div class="flex items-center gap-2">
        <span
          class="text-xs font-label-caps text-primary uppercase bg-primary/5 px-2 py-0.5 rounded"
        >
          Current Selection
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
    <h2 class="font-h1 text-h1 text-on-surface leading-tight mb-4">
      {{ article.title }}
    </h2>
    <!-- Collapsible Metadata -->
    <div class="border border-slate-200 rounded overflow-hidden">
      <button
        class="w-full flex items-center justify-between px-3 py-2 text-xs font-label-caps text-slate-500 uppercase tracking-wider hover:bg-slate-50 cursor-pointer transition-colors"
        @click="toggleMetadata"
      >
        <span class="flex items-center gap-1 min-w-0 overflow-hidden">
          <span class="shrink-0">Metadata</span>
          <span
            v-if="!metadataExpanded && article.authors.length > 0"
            class="text-[11px] text-slate-400 font-body-sm normal-case tracking-normal truncate"
          >
            – {{ article.authors.join(', ') }}
          </span>
        </span>
        <span
          class="material-symbols-outlined text-[16px] transition-transform duration-200 shrink-0"
          :class="{ 'rotate-180': metadataExpanded }"
        >
          expand_more
        </span>
      </button>
      <div v-show="metadataExpanded" class="px-3 pb-3 space-y-3">
        <div
          v-if="article.authors.length > 0"
          class="flex flex-col gap-1 text-body-sm font-body-sm"
        >
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
            >Authors</span
          >
          <span class="text-on-surface">{{ article.authors.join(', ') }}</span>
        </div>
        <div v-if="article.affiliation" class="flex flex-col gap-1 text-body-sm font-body-sm">
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
            >Affiliation</span
          >
          <span class="text-on-surface">{{ article.affiliation }}</span>
        </div>
        <div class="grid grid-cols-2 gap-4 text-body-sm font-body-sm">
          <div class="flex flex-col gap-1">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >Journal</span
            >
            <span class="text-on-surface truncate">{{ article.journal ?? '---' }}</span>
          </div>
          <div class="flex flex-col gap-1">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >Year</span
            >
            <span class="text-on-surface">{{ article.publicationYear ?? '---' }}</span>
          </div>
          <div v-if="article.doi" class="flex flex-col gap-1 col-span-2">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >DOI</span
            >
            <div class="flex items-center gap-1">
              <a
                class="text-primary hover:underline"
                :href="'https://doi.org/' + article.doi"
                target="_blank"
                rel="noopener noreferrer"
              >
                {{ article.doi }}
              </a>
              <button
                class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer transition-colors"
                title="Copy DOI"
                @click="copyDoi"
              >
                content_copy
              </button>
            </div>
          </div>
          <div v-if="article.keywords.length > 0" class="flex flex-col gap-1 col-span-2">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >Keywords</span
            >
            <span class="text-on-surface">{{ article.keywords.join(', ') }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
