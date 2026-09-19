<script setup lang="ts">
/**
 * Article selection modal: teleported overlay for choosing which library
 * articles feed the article-mode chat context. Owns its search filter;
 * the parent owns the article list + selection set.
 */
import { computed, ref } from 'vue';
import type { Article } from '@/types';

const props = defineProps<{
  /** Whether the modal is open. */
  open: boolean;
  /** Candidate articles (the parent already filters duplicates out). */
  articles: Article[];
  /** Currently selected article ids (chat context set). */
  selectedIds: string[];
}>();

const emit = defineEmits<{
  /** Toggle one article in/out of the selection. */
  toggle: [articleId: string];
  /** Close without finishing (overlay click / header close). */
  close: [];
  /** Finish (Done button). */
  done: [];
}>();

/** Author list for one row's meta line. */
function formatAuthorsList(authors: string[]): string {
  if (!authors || authors.length === 0) return 'Unknown';
  if (authors.length <= 2) return authors.join('; ');
  return `${authors[0]}; ${authors[1]} et al.`;
}

const searchQuery = ref('');

/** Articles filtered by the search box (title, author, journal, year). */
const filteredArticles = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return props.articles;
  return props.articles.filter(
    (a) =>
      a.title.toLowerCase().includes(q) ||
      a.authors.some((author) => author.toLowerCase().includes(q)) ||
      (a.journal && a.journal.toLowerCase().includes(q)) ||
      (a.publicationYear && String(a.publicationYear).includes(q))
  );
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4"
      @click.self="emit('close')"
    >
      <div
        class="bg-white rounded-2xl shadow-xl w-full max-w-2xl max-h-[80vh] flex flex-col border border-slate-100 overflow-hidden animate-zoom-in"
      >
        <!-- Modal Header -->
        <div class="px-6 py-4 border-b border-slate-150 flex items-center justify-between">
          <div>
            <h3 class="text-base font-bold text-slate-900">Include Articles in Context</h3>
            <p class="text-xs text-slate-500">
              Search and toggle articles to provide as background knowledge
            </p>
          </div>
          <button
            class="w-8 h-8 rounded-full hover:bg-slate-100 flex items-center justify-center text-slate-500 transition-colors"
            @click="emit('close')"
          >
            <span class="material-symbols-outlined text-[20px]">close</span>
          </button>
        </div>

        <!-- Search Bar -->
        <div class="px-6 py-3 border-b border-slate-100 bg-slate-50/50">
          <div class="relative">
            <span
              class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 text-[18px]"
              >search</span
            >
            <input
              v-model="searchQuery"
              type="text"
              placeholder="Search by title, authors, or journal..."
              class="w-full pl-9 pr-4 py-2 rounded-xl border border-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 text-sm bg-white"
            />
          </div>
        </div>

        <!-- Articles list -->
        <div class="flex-1 overflow-y-auto p-4 divide-y divide-slate-100">
          <div
            v-for="art in filteredArticles"
            :key="art.id"
            class="flex items-center gap-4 py-3 px-2.5 hover:bg-slate-50 rounded-xl cursor-pointer transition-colors"
            @click="emit('toggle', art.id)"
          >
            <!-- Checkbox -->
            <input
              type="checkbox"
              class="accent-indigo-600 rounded cursor-pointer w-4 h-4 flex-shrink-0"
              :checked="selectedIds.includes(art.id)"
              @click.stop="emit('toggle', art.id)"
            />

            <!-- Info -->
            <div class="flex-1 min-w-0">
              <p class="text-sm font-semibold text-slate-900 truncate mb-0.5">
                {{ art.title }}
              </p>
              <div class="flex items-center gap-2 text-xs text-slate-500">
                <span class="font-medium text-slate-600">{{ formatAuthorsList(art.authors) }}</span>
                <span class="text-slate-300">&bull;</span>
                <span>{{ art.publicationYear ?? 'N/A' }}</span>
                <span v-if="art.journal" class="text-slate-300">&bull;</span>
                <span v-if="art.journal" class="italic truncate max-w-[150px]">{{
                  art.journal
                }}</span>
              </div>
            </div>

            <!-- Status badge -->
            <div
              class="px-2 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider"
              :class="{
                'bg-emerald-50 text-emerald-700': art.status === 'included',
                'bg-indigo-50 text-indigo-700': art.status === 'working',
                'bg-red-50 text-red-700': art.status === 'rejected',
              }"
            >
              {{ art.status }}
            </div>
          </div>

          <!-- Empty Selector State -->
          <div
            v-if="filteredArticles.length === 0"
            class="text-center py-12 text-slate-400 text-sm"
          >
            No matching articles found in your library.
          </div>
        </div>

        <!-- Footer -->
        <div
          class="px-6 py-4 border-t border-slate-100 bg-slate-50/50 flex justify-between items-center text-xs"
        >
          <span class="text-slate-500"> {{ selectedIds.length }} article(s) selected </span>
          <button
            class="px-4 py-2 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg font-semibold shadow-sm transition-colors text-xs"
            @click="emit('done')"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.animate-zoom-in {
  animation: chat-selector-zoom-in 0.2s cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

@keyframes chat-selector-zoom-in {
  from {
    transform: scale(0.95);
    opacity: 0;
  }
  to {
    transform: scale(1);
    opacity: 1;
  }
}
</style>
