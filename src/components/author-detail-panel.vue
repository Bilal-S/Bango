<template>
  <Transition name="slide">
    <div
      v-if="author"
      class="absolute top-0 right-0 h-full w-80 bg-white border-l border-slate-200 shadow-xl z-40 flex flex-col overflow-hidden"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3 border-b border-slate-100">
        <h3 class="text-sm font-semibold text-slate-800 truncate">{{ author.label }}</h3>
        <button
          class="p-1 rounded hover:bg-slate-100 cursor-pointer transition-colors"
          @click="$emit('close')"
        >
          <span class="material-symbols-outlined text-base text-slate-400">close</span>
        </button>
      </div>

      <!-- Metrics -->
      <div class="grid grid-cols-3 gap-3 p-4 pb-2">
        <div class="bg-slate-50 rounded-lg p-3 text-center">
          <p class="text-lg font-bold text-indigo-600">{{ author.weight }}</p>
          <p class="text-[10px] text-slate-500 mt-0.5">Papers</p>
        </div>
        <div class="bg-slate-50 rounded-lg p-3 text-center">
          <p class="text-lg font-bold text-indigo-600">{{ author.totalCitations }}</p>
          <p class="text-[10px] text-slate-500 mt-0.5">Citations</p>
        </div>
        <div class="bg-slate-50 rounded-lg p-3 text-center">
          <p class="text-lg font-bold text-indigo-600">{{ author.estimatedHIndex ?? '—' }}</p>
          <p class="text-[10px] text-slate-500 mt-0.5">h-index</p>
        </div>
      </div>

      <!-- Pubs/Year sparkline bar graph -->
      <div class="px-4 pb-3">
        <div class="bg-slate-50 rounded-lg p-3">
          <p class="text-[10px] text-slate-500 mb-2 font-medium">Pubs / Year</p>
          <div v-if="pubsLoading" class="flex justify-center py-3">
            <span class="animate-spin rounded-full h-4 w-4 border-b-2 border-indigo-600"></span>
          </div>
          <div
            v-else-if="pubsByYear.length === 0"
            class="text-[10px] text-slate-400 italic text-center py-2"
          >
            No year data
          </div>
          <div v-else class="flex items-end gap-[2px] h-14" style="min-width: 0">
            <div
              v-for="(yc, i) in pubsByYear"
              :key="i"
              class="relative flex-1 group rounded-t-sm transition-colors duration-150 cursor-default"
              :style="{
                height: barHeight(yc.count) + '%',
                backgroundColor: barColor(i),
                minWidth: '0',
              }"
            >
              <!-- Hover tooltip -->
              <div
                class="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-slate-800 text-white whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-10"
              >
                {{ yc.year }}: {{ yc.count }}
              </div>
            </div>
          </div>
          <!-- Year labels (first / last) -->
          <div
            v-if="pubsByYear.length > 1"
            class="flex justify-between text-[9px] text-slate-400 mt-1"
          >
            <span>{{ pubsByYear[0]!.year }}</span>
            <span>{{ pubsByYear[pubsByYear.length - 1]!.year }}</span>
          </div>
        </div>
      </div>

      <!-- Cluster badge -->
      <div v-if="author.cluster !== null" class="px-4 pb-3">
        <span
          class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium text-white"
          :style="{ backgroundColor: authorColor }"
        >
          Cluster {{ (author.cluster ?? 0) + 1 }}
        </span>
      </div>

      <!-- Scroll Area (Affiliations & Co-authors) -->
      <div class="flex-1 overflow-y-auto px-4 pb-4 space-y-4">
        <!-- Affiliations -->
        <div>
          <p class="text-xs font-semibold text-slate-500 mb-2">Affiliations</p>
          <div v-if="loading" class="flex justify-center py-2">
            <span class="animate-spin rounded-full h-4 w-4 border-b-2 border-indigo-600"></span>
          </div>
          <div v-else-if="institutions.length === 0" class="text-xs text-slate-400 italic">
            No affiliations found
          </div>
          <ul v-else class="space-y-2">
            <li
              v-for="inst in institutions"
              :key="inst.id"
              class="bg-slate-50 rounded p-2 text-xs border border-slate-100 flex flex-col"
            >
              <span class="font-medium text-slate-800 capitalize">{{ inst.normalizedName }}</span>
              <span
                v-if="inst.city || inst.country"
                class="text-slate-400 text-[10px] mt-0.5 flex items-center gap-1"
              >
                <span class="material-symbols-outlined text-[10px] leading-none">location_on</span>
                {{ [inst.city, inst.country].filter(Boolean).join(', ') }}
              </span>
            </li>
          </ul>
        </div>

        <!-- Co-authors -->
        <div>
          <p class="text-xs font-semibold text-slate-500 mb-2">
            Co-Authors ({{ coAuthors.length }})
          </p>
          <ul class="space-y-1">
            <li
              v-for="ca in coAuthors"
              :key="ca.id"
              class="flex items-center justify-between text-xs text-slate-700 py-1 px-2 rounded hover:bg-slate-50 cursor-pointer"
              @click="$emit('navigate', ca.id)"
            >
              <span class="truncate">{{ ca.label }}</span>
              <span class="text-slate-400 ml-2 shrink-0">{{ ca.weight }}p</span>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type Graph from 'graphology';
import type { CoAuthorNode, BiblioInstitution, YearCount } from '../types/biblio-network';
import { clusterColor } from '../types/biblio-network';
import { tauriCommand, isTauri } from '../composables/use-tauri-command';

const props = defineProps<{
  author: CoAuthorNode | null;
  graph: Graph | null;
}>();

defineEmits<{
  (e: 'close'): void;
  (e: 'navigate', nodeId: string): void;
}>();

const authorColor = computed(() =>
  props.author?.cluster !== null ? clusterColor(props.author?.cluster ?? 0) : '#94a3b8'
);

const institutions = ref<BiblioInstitution[]>([]);
const loading = ref(false);

// ── Pubs/Year sparkline state ──
const pubsByYear = ref<YearCount[]>([]);
const pubsLoading = ref(false);
const pubsMax = computed(() =>
  pubsByYear.value.length > 0 ? Math.max(...pubsByYear.value.map((yc) => yc.count)) : 0
);

/** Bar height as percentage of the container. */
function barHeight(count: number): number {
  if (pubsMax.value === 0) return 0;
  return Math.max(8, (count / pubsMax.value) * 100);
}

/** Bar color: filled bars use indigo-400, hovered bars use indigo-600 via CSS group-hover. */
function barColor(_index: number): string {
  return '#818cf8'; // indigo-400
}

watch(
  () => props.author?.id,
  async (newId) => {
    if (!newId) {
      institutions.value = [];
      pubsByYear.value = [];
      return;
    }
    loading.value = true;
    pubsLoading.value = true;
    try {
      if (isTauri()) {
        const [instResult, pubsResult] = await Promise.all([
          tauriCommand<BiblioInstitution[]>('biblio_get_author_institutions', {
            authorId: newId,
          }),
          tauriCommand<YearCount[]>('biblio_get_author_pubs_by_year', {
            authorId: newId,
          }),
        ]);
        institutions.value = instResult;
        pubsByYear.value = pubsResult;
      } else {
        institutions.value = [
          {
            id: 'mock-1',
            normalizedName: 'mock university of bango',
            city: 'Tauri Town',
            country: 'USA',
            createdAt: new Date().toISOString(),
          },
        ];
        pubsByYear.value = [
          { year: 2020, count: 2 },
          { year: 2021, count: 5 },
          { year: 2022, count: 3 },
          { year: 2023, count: 7 },
          { year: 2024, count: 4 },
        ];
      }
    } catch (err) {
      console.error('Failed to load author details:', err);
      institutions.value = [];
      pubsByYear.value = [];
    } finally {
      loading.value = false;
      pubsLoading.value = false;
    }
  },
  { immediate: true }
);

const coAuthors = computed(() => {
  if (!props.author || !props.graph) return [];
  const g = props.graph;
  const nodeId = props.author.id;
  if (!g.hasNode(nodeId)) return [];

  return g
    .neighbors(nodeId)
    .map((n: string) => {
      const attrs = g.getNodeAttributes(n);
      return {
        id: n,
        label: attrs.label ?? n,
        weight: attrs.weight ?? 0,
      };
    })
    .sort((a: { weight: number }, b: { weight: number }) => b.weight - a.weight);
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
