<template>
  <Transition name="slide">
    <div
      v-if="keyword"
      class="absolute top-0 right-0 h-full w-80 bg-white border-l border-slate-200 shadow-xl z-40 flex flex-col overflow-hidden"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3 border-b border-slate-100">
        <h3 class="text-sm font-semibold text-slate-800 truncate" :title="keyword.label">
          {{ keyword.label }}
        </h3>
        <button
          class="p-1 rounded hover:bg-slate-100 cursor-pointer transition-colors"
          @click="$emit('close')"
        >
          <span class="material-symbols-outlined text-base text-slate-400">close</span>
        </button>
      </div>

      <!-- Metrics -->
      <div class="grid grid-cols-2 gap-3 p-4 pb-2">
        <div class="bg-slate-50 rounded-lg p-3 text-center flex flex-col justify-center h-24">
          <p class="text-xl font-bold text-indigo-600 leading-tight">{{ keyword.weight }}</p>
          <p class="text-[10px] text-slate-500 font-medium mt-1">Occurrences</p>
        </div>
        <div class="bg-slate-50 rounded-lg p-2.5 flex flex-col justify-between h-24">
          <div
            v-if="pubsByYear.length === 0"
            class="text-[10px] text-slate-400 italic text-center my-auto"
          >
            No year data
          </div>
          <div v-else class="flex flex-col justify-between h-full">
            <div class="flex items-end gap-[1.5px] h-10 mt-1" style="min-width: 0">
              <div
                v-for="(yc, i) in pubsByYear"
                :key="i"
                class="relative flex-1 group rounded-t-sm transition-colors duration-150 cursor-default"
                :style="{
                  height: barHeight(yc.count) + '%',
                  backgroundColor: '#818cf8',
                  minWidth: '0',
                }"
              >
                <!-- Hover tooltip -->
                <div
                  class="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 px-1.5 py-0.5 rounded text-[9px] font-medium bg-slate-800 text-white whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-10"
                >
                  {{ yc.year }}: {{ yc.count }}
                </div>
              </div>
            </div>
            <!-- Year labels (first / last) -->
            <div
              v-if="pubsByYear.length > 1"
              class="flex justify-between text-[8px] text-slate-400 leading-none mt-1"
            >
              <span>{{ pubsByYear[0]?.year }}</span>
              <span>{{ pubsByYear[pubsByYear.length - 1]?.year }}</span>
            </div>
          </div>
          <p class="text-[10px] text-slate-500 font-medium text-center mt-1">Pubs / Year</p>
        </div>
      </div>

      <!-- Google Trends Compare CTA -->
      <div class="px-4 py-1.5 shrink-0">
        <button
          class="w-full py-1.5 px-3 rounded border text-xs font-semibold flex items-center justify-center gap-1.5 transition-all select-none cursor-pointer"
          :disabled="isQueueFull && !isQueued"
          :class="
            isQueued
              ? 'bg-rose-50 border-rose-200 text-rose-700 hover:bg-rose-100 hover:border-rose-300'
              : isQueueFull
                ? 'bg-slate-50 border-slate-200 text-slate-400 cursor-not-allowed'
                : 'bg-indigo-50 border-indigo-200 text-indigo-700 hover:bg-indigo-100 hover:border-indigo-300'
          "
          :title="
            isQueued
              ? 'Remove this keyword from Google Trends comparison'
              : isQueueFull
                ? 'Comparison queue is full (max 5 keywords). Remove one to add this.'
                : 'Add this keyword to Google Trends comparison'
          "
          @click="toggleQueue"
        >
          <span class="material-symbols-outlined text-[15px]">
            {{ isQueued ? 'remove_from_queue' : 'add_to_queue' }}
          </span>
          {{ isQueued ? 'Remove from Google Trends' : 'Compare in Google Trends' }}
        </button>
      </div>

      <!-- View Articles CTA (Gap 1a) -->
      <!-- Gated to tags/labels-sourced keyword nodes: the existing
           ArticleQuery.tags / ArticleQuery.labels filters match the node label
           (most-frequent raw term = the tag/label name). Metadata/ai/user-
           sourced nodes need the deferred backend ArticleQuery.keywords
           field (Gap 1b) so the button is hidden for them rather than
           rendering as dead. -->
      <div v-if="canViewArticles" class="px-4 py-1.5 shrink-0">
        <button
          class="w-full py-1.5 px-3 rounded border text-xs font-semibold flex items-center justify-center gap-1.5 transition-all select-none cursor-pointer bg-emerald-50 border-emerald-200 text-emerald-700 hover:bg-emerald-100 hover:border-emerald-300"
          title="View articles tagged/labelled with this keyword"
          @click="$emit('view-articles')"
        >
          <span class="material-symbols-outlined text-[15px]">article</span>
          View articles
        </button>
      </div>

      <!-- Detail Info -->
      <div class="px-4 py-2 space-y-2">
        <div class="flex justify-between items-center text-xs">
          <span class="text-slate-500 font-medium">Source:</span>
          <span
            class="font-semibold text-slate-700 capitalize bg-slate-100 px-2 py-0.5 rounded-full text-[10px]"
            >{{ keyword.source }}</span
          >
        </div>
        <div v-if="keyword.cluster !== null" class="flex justify-between items-center text-xs">
          <span class="text-slate-500 font-medium">Community:</span>
          <span
            class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[10px] font-semibold text-white"
            :style="{ backgroundColor: keywordColor }"
          >
            Cluster {{ (keyword.cluster ?? 0) + 1 }}
          </span>
        </div>
      </div>

      <!-- Scroll Area -->
      <div class="flex-1 overflow-y-auto px-4 pb-4 space-y-4 pt-2">
        <!-- Raw Mapped Terms -->
        <div v-if="keyword.rawTerms && keyword.rawTerms.length > 0">
          <p class="text-xs font-semibold text-slate-500 mb-1.5">Mapped Raw Terms</p>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="term in keyword.rawTerms"
              :key="term"
              class="px-2 py-1 bg-slate-50 border border-slate-200/80 rounded-md text-[10px] text-slate-600"
            >
              {{ term }}
            </span>
          </div>
        </div>

        <!-- Co-occurring Terms -->
        <div>
          <p class="text-xs font-semibold text-slate-500 mb-2">
            Related Keywords ({{ relatedKeywords.length }})
          </p>
          <div v-if="relatedKeywords.length === 0" class="text-xs text-slate-400 italic">
            No related keywords visible under current filters.
          </div>
          <ul v-else class="space-y-1">
            <li
              v-for="rk in relatedKeywords"
              :key="rk.id"
              class="flex items-center justify-between text-xs text-slate-700 py-1.5 px-2 rounded hover:bg-slate-50 cursor-pointer border border-transparent hover:border-slate-100 transition-all"
              @click="$emit('navigate', rk.id)"
            >
              <span class="truncate font-medium text-slate-800" :title="rk.label">{{
                rk.label
              }}</span>
              <div class="flex items-center gap-2 text-slate-400 shrink-0">
                <span
                  class="text-[10px] bg-slate-100 px-1.5 py-0.5 rounded text-slate-500"
                  title="Co-occurrences"
                >
                  w: {{ rk.edgeWeight }}
                </span>
                <span class="text-[10px]" title="Total frequency"> {{ rk.weight }}f </span>
              </div>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type Graph from 'graphology';
import type { KeywordNode } from '../types/biblio-keyword';
import { clusterColor } from '../types/biblio-network';
import { useTrendsQueueStore } from '../stores/trends-queue';

const props = defineProps<{
  keyword: KeywordNode | null;
  graph: Graph | null;
}>();

defineEmits<{
  (e: 'close'): void;
  (e: 'navigate', nodeId: string): void;
  /**
   * Emitted when the user clicks the "View articles" button. Only rendered
   * for `tags`/`labels`-sourced keyword nodes because the existing
   * `ArticleQuery.tags` / `ArticleQuery.labels` filters can match those node
   * labels. `metadata` / `ai_extracted` / `user_added`-sourced nodes are
   * deferred (Gap 1b) until a backend `ArticleQuery.keywords` field exists.
   */
  (e: 'view-articles'): void;
}>();

const trendsQueue = useTrendsQueueStore();

const isQueued = computed(() => {
  if (!props.keyword) return false;
  return trendsQueue.keywords.some((k) => k.toLowerCase() === props.keyword!.label.toLowerCase());
});

const isQueueFull = computed(() => {
  return trendsQueue.keywords.length >= 5;
});

/**
 * Whether the "View articles" deep-link is available for this keyword node.
 * Gated to `tags`/`labels`-sourced nodes because the existing
 * `ArticleQuery.tags` / `ArticleQuery.labels` filters can match those node
 * labels (the label is the most-frequent raw term — i.e. the tag/label name).
 * `metadata` / `ai_extracted` / `user_added`-sourced nodes are sourced from
 * `biblio_article_terms` / `articles.keywords`, which no existing filter
 * matches; those are deferred to Gap 1b (backend `ArticleQuery.keywords`).
 */
const canViewArticles = computed(
  () => props.keyword?.source === 'tags' || props.keyword?.source === 'labels'
);

function toggleQueue() {
  if (!props.keyword) return;
  const term = props.keyword.label;
  if (isQueued.value) {
    trendsQueue.removeKeyword(term);
  } else {
    trendsQueue.addKeyword(term);
  }
}

const keywordColor = computed(() =>
  props.keyword?.cluster !== null ? clusterColor(props.keyword?.cluster ?? 0) : '#94a3b8'
);

const pubsByYear = computed(() => {
  return props.keyword?.yearCounts ?? [];
});

const pubsMax = computed(() =>
  pubsByYear.value.length > 0 ? Math.max(...pubsByYear.value.map((yc) => yc.count)) : 0
);

function barHeight(count: number): number {
  if (pubsMax.value === 0) return 0;
  return Math.max(8, (count / pubsMax.value) * 100);
}

const relatedKeywords = computed(() => {
  if (!props.keyword || !props.graph) return [];
  const g = props.graph;
  const nodeId = props.keyword.id;
  if (!g.hasNode(nodeId)) return [];

  return g
    .neighbors(nodeId)
    .filter((n: string) => g.getNodeAttribute(n, 'hidden') !== true)
    .map((n: string) => {
      const attrs = g.getNodeAttributes(n);
      // Retrieve the undirected edge weight
      const edgeWeight = (g.getEdgeAttribute(nodeId, n, 'weight') as number) ?? 0;
      return {
        id: n,
        label: attrs.label ?? n,
        weight: attrs.weight ?? 0,
        edgeWeight,
      };
    })
    .sort((a, b) => b.edgeWeight - a.edgeWeight || b.weight - a.weight);
});
</script>

<style scoped>
.slide-enter-active,
.slide-leave-active {
  transition: transform 0.25s ease;
}
.slide-enter-from,
.slide-leave-to {
  transform: translateX(100%);
}
</style>
