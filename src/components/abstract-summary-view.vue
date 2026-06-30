<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import type { Article } from '@/types';
import {
  parseAiSummary,
  requestFigureDescriptions,
  pendingFigureDescriptions,
} from '@/composables/use-ai-summary';
import type { AiSummaryData } from '@/composables/use-ai-summary';

const props = defineProps<{
  article: Article;
}>();

const emit = defineEmits<{ refreshArticle: [articleId: string] }>();

/** True when a figure-description LLM call is in flight for this article. */
const isGeneratingFigures = computed(() => pendingFigureDescriptions.value.has(props.article.id));

/** Trigger the batched figure/table description LLM call (Tier 2 Phase 4).
 *  On completion, emits `refreshArticle` so the parent re-fetches the article
 *  and the new `figures`/`tables` keys render. */
async function onGenerateFigureDescriptions(): Promise<void> {
  await requestFigureDescriptions(props.article.id, props.article.title, async (id) => {
    emit('refreshArticle', id);
  });
}

// Parsed AI summary data
const aiSummaryData = computed<AiSummaryData | null>(() =>
  parseAiSummary(props.article.fullTextAiSummary)
);

// Default to the AI Summary tab when one exists, otherwise Abstract.
const defaultTab = (): 'abstract' | 'aiSummary' => (aiSummaryData.value ? 'aiSummary' : 'abstract');

// Active tab for Abstract/AI Summary
const abstractTab = ref<'abstract' | 'aiSummary'>(defaultTab());

// Per-section expand/collapse state for the Section Summaries block.
const expandedSections = ref<Set<string>>(new Set());
const sectionsExpanded = ref(false);

function toggleSection(name: string): void {
  // Mutate the Set reactively by creating a new instance.
  const next = new Set(expandedSections.value);
  if (next.has(name)) {
    next.delete(name);
  } else {
    next.add(name);
  }
  expandedSections.value = next;
}

// Reset to the default tab whenever the selected article changes so that
// articles with an AI summary land on the AI Summary tab automatically.
watch(
  () => props.article.id,
  () => {
    abstractTab.value = defaultTab();
    expandedSections.value = new Set();
    sectionsExpanded.value = false;
  }
);
</script>

<template>
  <section v-if="article.abstractText || aiSummaryData">
    <!-- Tab bar (only show when AI summary exists) -->
    <div v-if="aiSummaryData" class="flex border-b border-slate-200 mb-3">
      <button
        class="px-3 py-1.5 text-xs font-label-caps uppercase tracking-wider transition-colors cursor-pointer"
        :class="
          abstractTab === 'abstract'
            ? 'text-indigo-700 border-b-2 border-indigo-600'
            : 'text-slate-400 hover:text-slate-600'
        "
        @click="abstractTab = 'abstract'"
      >
        Abstract
      </button>
      <button
        class="px-3 py-1.5 text-xs font-label-caps uppercase tracking-wider transition-colors cursor-pointer flex items-center gap-1"
        :class="
          abstractTab === 'aiSummary'
            ? 'text-violet-700 border-b-2 border-violet-600'
            : 'text-slate-400 hover:text-slate-600'
        "
        @click="abstractTab = 'aiSummary'"
      >
        <span class="material-symbols-outlined text-[14px]">auto_awesome</span>
        AI Summary
      </button>
    </div>
    <h3 v-else class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">
      Abstract
    </h3>
    <!-- Abstract content -->
    <p
      v-if="abstractTab === 'abstract'"
      class="text-body-main font-body-main text-on-surface-variant leading-relaxed"
    >
      {{ article.abstractText }}
    </p>
    <!-- AI Summary content -->
    <div v-else-if="abstractTab === 'aiSummary' && aiSummaryData" class="space-y-4">
      <div class="flex items-center gap-2 text-xs text-slate-500">
        <span class="bg-violet-100 text-violet-700 px-2 py-0.5 rounded font-semibold capitalize">
          {{ aiSummaryData.field.replace(/_/g, ' ') }}
        </span>
        <span class="text-slate-400">·</span>
        <span class="italic">{{ aiSummaryData.subfield }}</span>
      </div>
      <!-- Structured extraction -->
      <div v-if="aiSummaryData.structured_extraction" class="space-y-2">
        <div v-for="(value, key) in aiSummaryData.structured_extraction" :key="key" class="text-sm">
          <span class="font-semibold text-slate-700 capitalize">{{ key.replace(/_/g, ' ') }}</span>
          <p class="text-slate-600 leading-relaxed">{{ value }}</p>
        </div>
      </div>
      <!-- Summary -->
      <div v-if="aiSummaryData.summary_150_250_words">
        <h4 class="text-xs font-label-caps text-violet-600 uppercase tracking-wider mb-1">
          Summary
        </h4>
        <p class="text-body-main font-body-main text-on-surface-variant leading-relaxed">
          {{ aiSummaryData.summary_150_250_words }}
        </p>
      </div>
      <!-- Key Insights -->
      <div v-if="aiSummaryData.key_insights.length > 0">
        <h4 class="text-xs font-label-caps text-violet-600 uppercase tracking-wider mb-1">
          Key Insights
        </h4>
        <ul class="space-y-1">
          <li
            v-for="(insight, idx) in aiSummaryData.key_insights"
            :key="idx"
            class="flex gap-2 text-sm text-slate-600"
          >
            <span class="text-violet-400 mt-0.5 shrink-0">•</span>
            <span>{{ insight }}</span>
          </li>
        </ul>
      </div>
      <!-- Keywords -->
      <div v-if="aiSummaryData.keywords.length > 0">
        <h4 class="text-xs font-label-caps text-violet-600 uppercase tracking-wider mb-1">
          Keywords
        </h4>
        <div class="flex flex-wrap gap-1.5">
          <span
            v-for="kw in aiSummaryData.keywords"
            :key="kw"
            class="text-xs bg-slate-100 text-slate-600 px-2 py-0.5 rounded"
          >
            {{ kw }}
          </span>
        </div>
      </div>
      <!-- Section Summaries (schema v2; optional) -->
      <div v-if="(aiSummaryData.section_summaries ?? []).length > 0">
        <div class="flex items-center justify-between mb-1">
          <h4 class="text-xs font-label-caps text-violet-600 uppercase tracking-wider">
            Section Summaries
          </h4>
          <button
            class="text-xs text-slate-500 hover:text-violet-700 cursor-pointer select-none"
            @click="sectionsExpanded = !sectionsExpanded"
          >
            {{ sectionsExpanded ? 'Collapse all' : 'Expand all' }}
          </button>
        </div>
        <div class="space-y-2">
          <div
            v-for="sec in aiSummaryData.section_summaries ?? []"
            :key="sec.section"
            class="border border-slate-200 rounded-md overflow-hidden"
          >
            <button
              class="w-full flex items-center justify-between px-3 py-2 bg-slate-50 hover:bg-slate-100 transition-colors text-left"
              @click="toggleSection(sec.section)"
            >
              <span class="flex items-center gap-2">
                <span
                  class="text-xs font-label-caps text-violet-700 uppercase tracking-wider font-semibold"
                  >{{ sec.section }}</span
                >
                <span
                  v-if="sec.study_design"
                  class="text-[10px] bg-violet-100 text-violet-700 px-1.5 py-0.5 rounded"
                  >{{ sec.study_design }}</span
                >
                <span
                  v-if="sec.effect_size"
                  class="text-[10px] bg-emerald-100 text-emerald-700 px-1.5 py-0.5 rounded"
                  >{{ sec.effect_size }}</span
                >
              </span>
              <span class="material-symbols-outlined text-[16px] text-slate-400">{{
                expandedSections.has(sec.section) || sectionsExpanded
                  ? 'expand_less'
                  : 'expand_more'
              }}</span>
            </button>
            <div
              v-if="expandedSections.has(sec.section) || sectionsExpanded"
              class="px-3 py-2 space-y-2"
            >
              <p class="text-sm text-slate-600 leading-relaxed">{{ sec.summary }}</p>
              <ul v-if="(sec.key_points ?? []).length > 0" class="space-y-1">
                <li
                  v-for="(point, idx) in sec.key_points ?? []"
                  :key="idx"
                  class="flex gap-2 text-xs text-slate-500"
                >
                  <span class="text-violet-400 mt-0.5 shrink-0">•</span>
                  <span>{{ point }}</span>
                </li>
              </ul>
              <div
                v-if="sec.confidence_interval"
                class="text-xs text-slate-500 flex gap-1 items-center"
              >
                <span class="material-symbols-outlined text-[14px] text-slate-400"
                  >confidence_interval</span
                >
                <span>{{ sec.confidence_interval }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
      <!-- ── Tier 2 Phase 4: Figures & Tables ───────────────────────── -->
      <!-- Trigger button (gated on full text attached). Shown when no
           descriptions exist yet OR when regenerating. -->
      <div v-if="article.hasFullText" class="flex items-center gap-2">
        <button
          class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-violet-200 bg-violet-50 hover:bg-violet-100 text-violet-700 text-xs font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          :disabled="isGeneratingFigures"
          @click="onGenerateFigureDescriptions"
        >
          <span
            v-if="isGeneratingFigures"
            class="material-symbols-outlined text-[14px] animate-spin"
            >progress_activity</span
          >
          <span v-else class="material-symbols-outlined text-[14px]">image_search</span>
          {{
            aiSummaryData?.figures?.length || aiSummaryData?.tables?.length
              ? 'Regenerate'
              : 'Describe Figures & Tables'
          }}
        </button>
        <span v-if="isGeneratingFigures" class="text-xs text-slate-400">Analyzing captions...</span>
      </div>
      <!-- Figures grid (T2 Phase 4). Only renders when descriptions exist. -->
      <div v-if="(aiSummaryData?.figures ?? []).length > 0">
        <h4 class="text-xs font-label-caps text-violet-600 uppercase tracking-wider mb-2">
          Figures
        </h4>
        <div class="grid grid-cols-1 gap-2">
          <div
            v-for="fig in aiSummaryData?.figures ?? []"
            :key="`fig-${fig.number}`"
            class="border border-slate-200 rounded-md p-3 bg-slate-50/50"
          >
            <div class="flex items-center gap-2 mb-1">
              <span
                class="text-xs font-label-caps text-violet-700 uppercase tracking-wider font-semibold"
                >Figure {{ fig.number }}</span
              >
            </div>
            <p v-if="fig.caption" class="text-xs text-slate-500 italic mb-1">{{ fig.caption }}</p>
            <p v-if="fig.description" class="text-sm text-slate-600 leading-relaxed">
              {{ fig.description }}
            </p>
          </div>
        </div>
      </div>
      <!-- Tables (T2 Phase 4). Only renders when descriptions exist. -->
      <div v-if="(aiSummaryData?.tables ?? []).length > 0">
        <h4 class="text-xs font-label-caps text-violet-600 uppercase tracking-wider mb-2">
          Tables
        </h4>
        <div class="grid grid-cols-1 gap-2">
          <div
            v-for="tbl in aiSummaryData?.tables ?? []"
            :key="`tbl-${tbl.number}`"
            class="border border-slate-200 rounded-md p-3 bg-slate-50/50"
          >
            <div class="flex items-center gap-2 mb-1">
              <span
                class="text-xs font-label-caps text-violet-700 uppercase tracking-wider font-semibold"
                >Table {{ tbl.number }}</span
              >
            </div>
            <p v-if="tbl.caption" class="text-xs text-slate-500 italic mb-1">{{ tbl.caption }}</p>
            <p v-if="tbl.description" class="text-sm text-slate-600 leading-relaxed">
              {{ tbl.description }}
            </p>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
