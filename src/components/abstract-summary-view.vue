<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import type { Article } from '@/types';
import { parseAiSummary } from '@/composables/use-ai-summary';
import type { AiSummaryData } from '@/composables/use-ai-summary';

const props = defineProps<{
  article: Article;
}>();

// Parsed AI summary data
const aiSummaryData = computed<AiSummaryData | null>(() =>
  parseAiSummary(props.article.fullTextAiSummary)
);

// Default to the AI Summary tab when one exists, otherwise Abstract.
const defaultTab = (): 'abstract' | 'aiSummary' => (aiSummaryData.value ? 'aiSummary' : 'abstract');

// Active tab for Abstract/AI Summary
const abstractTab = ref<'abstract' | 'aiSummary'>(defaultTab());

// Reset to the default tab whenever the selected article changes so that
// articles with an AI summary land on the AI Summary tab automatically.
watch(
  () => props.article.id,
  () => {
    abstractTab.value = defaultTab();
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
    </div>
  </section>
</template>
