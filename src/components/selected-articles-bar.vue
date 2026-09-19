<script setup lang="ts">
/**
 * Selected-articles context bar (article chat mode): the pill list of
 * articles feeding RAG context, with a hover tooltip carrying the full
 * title + authors, an open-details action, and a per-pill remove action.
 */
import { ref } from 'vue';
import type { Article } from '@/types';

defineProps<{
  /** The selected (context) articles, in selection order. */
  articles: Article[];
}>();

const emit = defineEmits<{
  /** Open the article detail slide-over. */
  openDetail: [articleId: string];
  /** Remove one article from the context set. */
  remove: [articleId: string];
  /** Clear the whole context set. */
  clear: [];
}>();

/** Truncate a display string at a word-safe-ish boundary. */
function truncateString(str: string, maxLen = 20): string {
  if (!str) return '';
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 3) + '...';
}

/** First author, truncated, for the pill label. */
function getAuthorText(article: Article): string {
  const author = article.authors?.[0] ?? 'Unknown';
  return truncateString(author, 20);
}

/** Title, truncated, for the pill label. */
function getTitleText(article: Article): string {
  return truncateString(article.title, 20);
}

/** Full author list for the hover tooltip. */
function formatAuthorsList(authors: string[]): string {
  if (!authors || authors.length === 0) return 'Unknown';
  if (authors.length <= 2) return authors.join('; ');
  return `${authors[0]}; ${authors[1]} et al.`;
}

// Hover-tooltip state (floating, teleported tooltip below).
const hoveredArticle = ref<Article | null>(null);
const tooltipX = ref(0);
const tooltipY = ref(0);

function handleMouseEnter(event: MouseEvent, article: Article) {
  hoveredArticle.value = article;
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  tooltipX.value = rect.left + rect.width / 2;
  tooltipY.value = rect.top;
}

function handleMouseLeave() {
  hoveredArticle.value = null;
}

function onRemove(articleId: string) {
  emit('remove', articleId);
  handleMouseLeave();
}
</script>

<template>
  <div class="mb-3">
    <div class="flex items-center justify-between mb-2">
      <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">
        Selected Context ({{ articles.length }})
      </span>
      <button
        v-if="articles.length > 0"
        class="text-[11px] text-indigo-600 hover:text-indigo-800 font-semibold"
        @click="emit('clear')"
      >
        Clear Context
      </button>
    </div>

    <!-- Horizontal scrolling pills -->
    <div class="flex flex-wrap gap-2 max-h-32 overflow-y-auto py-1">
      <div
        v-for="art in articles"
        :key="art.id"
        class="relative flex items-center rounded-full bg-slate-100 border border-slate-200 text-xs text-slate-700 hover:bg-slate-200 transition-colors"
      >
        <!-- Info text area with help cursor and hover tooltip trigger -->
        <div
          class="flex items-center gap-1.5 pl-3 py-1.5 pr-2 cursor-help rounded-l-full"
          @mouseenter="handleMouseEnter($event, art)"
          @mouseleave="handleMouseLeave"
        >
          <span class="font-semibold text-slate-800">{{ getAuthorText(art) }}</span>
          <span class="text-slate-500">({{ art.publicationYear ?? 'N/A' }})</span>
          <span class="text-slate-400">-</span>
          <span class="truncate max-w-[120px]">{{ getTitleText(art) }}</span>
        </div>

        <!-- Control actions area (does NOT trigger hover tooltip, has pointer cursor) -->
        <div
          class="flex items-center gap-1.5 pr-3 py-1 border-l border-slate-200/60 pl-2 rounded-r-full"
        >
          <!-- Open In New details action -->
          <button
            class="flex items-center justify-center w-5 h-5 rounded-full hover:bg-slate-300 text-slate-500 hover:text-indigo-600 transition-colors cursor-pointer"
            title="Open article details"
            @click="emit('openDetail', art.id)"
          >
            <span class="material-symbols-outlined text-[14px]">open_in_new</span>
          </button>

          <!-- Close button -->
          <button
            class="flex items-center justify-center w-5 h-5 rounded-full hover:bg-slate-300 text-slate-500 hover:text-rose-600 transition-colors cursor-pointer"
            title="Remove from context"
            @click="onRemove(art.id)"
          >
            <span class="material-symbols-outlined text-[14px]">close</span>
          </button>
        </div>
      </div>

      <p v-if="articles.length === 0" class="text-xs text-slate-400 italic py-1">
        No articles added. Click (+) to select articles from your library to include in this query.
      </p>
    </div>
  </div>

  <!-- Floating Tooltip for pills -->
  <Teleport to="body">
    <Transition name="tooltip-fade">
      <div
        v-if="hoveredArticle"
        class="fixed z-50 w-80 p-3 rounded-xl bg-slate-900 text-white text-[11px] leading-normal shadow-xl border border-slate-800 flex flex-col gap-1 pointer-events-none text-left"
        :style="{
          left: tooltipX + 'px',
          top: tooltipY + 'px',
          transform: 'translate(-50%, -108%)',
        }"
      >
        <div class="font-bold text-slate-400">Title</div>
        <div class="font-medium text-white break-words">{{ hoveredArticle.title }}</div>
        <div class="font-bold text-slate-400 mt-1">Authors</div>
        <div class="text-slate-300 break-words">
          {{ formatAuthorsList(hoveredArticle.authors) }}
        </div>
        <!-- Tooltip arrow -->
        <div
          class="absolute top-full left-1/2 -translate-x-1/2 border-4 border-transparent border-t-slate-900"
        ></div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* Tooltip animation */
.tooltip-fade-enter-active,
.tooltip-fade-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}
.tooltip-fade-enter-from,
.tooltip-fade-leave-to {
  opacity: 0;
  transform: translate(-50%, -100%) scale(0.95) !important;
}
</style>
