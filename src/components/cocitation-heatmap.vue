<template>
  <div class="heatmap-container">
    <div class="flex items-center justify-between mb-2">
      <h4 class="text-xs font-semibold text-slate-700 uppercase tracking-wide">
        Similarity Matrix (Top {{ papers.length }} Papers)
      </h4>
      <button
        class="text-[11px] text-slate-500 hover:text-slate-700 flex items-center gap-0.5"
        @click="$emit('toggle')"
      >
        <span class="material-symbols-outlined text-sm">expand_more</span>
        Collapse
      </button>
    </div>

    <VueApexCharts
      v-if="series.length > 0"
      type="heatmap"
      :options="chartOptions"
      :series="series"
      height="320"
    />
    <div v-else class="text-center text-slate-400 text-xs py-8">
      No matrix data available. Adjust thresholds to populate the heatmap.
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import VueApexCharts from 'vue3-apexcharts';
import type { ApexOptions } from 'apexcharts';
import type { CocitationEdge, CocitationNode } from '../types/biblio-cocitation';

const props = defineProps<{
  nodes: CocitationNode[];
  edges: CocitationEdge[];
  /** Max papers to show in the matrix (default 20). */
  maxPapers?: number;
}>();

defineEmits<{
  (e: 'toggle'): void;
}>();

/** Select the top-N papers by coCitationCount for the matrix. */
const papers = computed<CocitationNode[]>(() => {
  const limit = props.maxPapers ?? 20;
  return [...props.nodes].sort((a, b) => b.coCitationCount - a.coCitationCount).slice(0, limit);
});

/** Build a weight lookup map: (sourceId, targetId) → weight. */
const weightLookup = computed<Map<string, number>>(() => {
  const map = new Map<string, number>();
  const ids = new Set(papers.value.map((p) => p.id));
  for (const edge of props.edges) {
    if (ids.has(edge.source) && ids.has(edge.target)) {
      const key1 = `${edge.source}|${edge.target}`;
      const key2 = `${edge.target}|${edge.source}`;
      map.set(key1, edge.weight);
      map.set(key2, edge.weight);
    }
  }
  return map;
});

/** Short label for a paper: last name + year. */
function shortLabel(node: CocitationNode): string {
  const authors = node.authors || '';
  const firstAuthor = authors.split(';')[0]?.trim() || 'Unknown';
  const lastName = firstAuthor.split(',')[0] || firstAuthor;
  const yearSuffix = node.year ? ` '${String(node.year).slice(-2)}` : '';
  return `${lastName}${yearSuffix}`;
}

/** ApexCharts series (one per row = one paper). */
const series = computed(() => {
  const ps = papers.value;
  if (ps.length === 0) return [];

  return ps.map((rowPaper) => ({
    name: shortLabel(rowPaper),
    data: ps.map((colPaper) => {
      const w =
        rowPaper.id === colPaper.id
          ? 1
          : (weightLookup.value.get(`${rowPaper.id}|${colPaper.id}`) ?? 0);
      return {
        x: shortLabel(colPaper),
        y: Math.round(w * 1000) / 1000,
      };
    }),
  }));
});

const chartOptions = computed<ApexOptions>(() => ({
  chart: {
    type: 'heatmap',
    toolbar: { show: false },
    animations: { enabled: false },
    fontFamily: 'Inter, system-ui, sans-serif',
  },
  dataLabels: { enabled: false },
  colors: ['#4f46e5'],
  plotOptions: {
    heatmap: {
      radius: 2,
      enableShades: false,
      colorScale: {
        ranges: [
          { from: 0, to: 0, color: '#f8fafc', name: '0' },
          { from: 0.001, to: 0.25, color: '#e0e7ff', name: 'low' },
          { from: 0.251, to: 0.5, color: '#a5b4fc', name: 'med' },
          { from: 0.501, to: 0.75, color: '#6366f1', name: 'high' },
          { from: 0.751, to: 1, color: '#4338ca', name: 'very high' },
        ],
      },
    },
  },
  xaxis: {
    type: 'category',
    labels: {
      style: { fontSize: '9px' },
      rotate: -45,
      rotateAlways: true,
    },
    tickPlacement: 'on',
  },
  yaxis: {
    labels: { style: { fontSize: '9px' } },
  },
  grid: { padding: { right: 20 } },
  tooltip: {
    y: {
      formatter: (val: number) => val.toFixed(3),
    },
  },
}));
</script>

<style scoped>
.heatmap-container {
  padding: 0.5rem 0;
}
</style>
