<script setup lang="ts">
import { computed } from 'vue';
import type { Article } from '@/types';
import { useCriteriaStore } from '@/stores/criteria';

const props = defineProps<{
  article: Article;
}>();

const criteriaStore = useCriteriaStore();

const confidencePercentage = computed(() =>
  props.article.aiConfidence !== null ? `${Math.round(props.article.aiConfidence * 100)}%` : '---'
);

const confidenceBarWidth = computed(() =>
  props.article.aiConfidence !== null ? `${Math.round(props.article.aiConfidence * 100)}%` : '0%'
);

const aiDecisionLabel = computed(() => {
  if (!props.article.aiDecision) return null;
  return props.article.aiDecision === 'include' ? 'Included' : 'Excluded';
});

const aiDecisionColors = computed(() => {
  if (!props.article.aiDecision) return null;
  if (props.article.aiDecision === 'include') {
    return {
      bg: 'bg-emerald-50',
      border: 'border-emerald-200',
      icon: 'text-emerald-600',
      label: 'text-emerald-900',
      text: 'text-emerald-800',
    };
  }
  return {
    bg: 'bg-rose-50',
    border: 'border-rose-200',
    icon: 'text-rose-600',
    label: 'text-rose-900',
    text: 'text-rose-800',
  };
});

/** Compute global criterion index: inclusion [1]..[N], exclusion [N+1]..[N+M] */
const criterionIndexMap = computed(() => {
  const map = new Map<string, number>();
  let n = 1;
  for (const c of criteriaStore.inclusionCriteria) {
    map.set(c.id, n++);
  }
  for (const c of criteriaStore.exclusionCriteria) {
    map.set(c.id, n++);
  }
  return map;
});

/**
 * Replace criterion UUIDs in reasoning text with global numbered references `[n]`.
 * Also collapses double brackets `[[n]]` → `[n]` from LLM echoing prompt format.
 */
const displayReasoning = computed(() => {
  const raw = props.article.aiReasoning;
  if (!raw) return '';
  let result = raw;

  const map = criterionIndexMap.value;
  for (const [uuid, n] of map) {
    if (result.includes(uuid)) {
      result = result.replaceAll(uuid, `[${n}]`);
    }
  }

  let prev = '';
  while (prev !== result) {
    prev = result;
    result = result.replaceAll('[[', '[').replaceAll(']]', ']');
  }

  return result;
});
</script>

<template>
  <section v-if="aiDecisionLabel && aiDecisionColors">
    <div class="rounded-xl p-4 border" :class="[aiDecisionColors.bg, aiDecisionColors.border]">
      <div class="flex items-center justify-between mb-2">
        <div class="flex items-center gap-2">
          <span class="material-symbols-outlined" :class="aiDecisionColors.icon">
            {{ article.aiDecision === 'include' ? 'verified' : 'cancel' }}
          </span>
          <span class="font-bold" :class="aiDecisionColors.label">
            {{ aiDecisionLabel }}
          </span>
        </div>
        <span
          class="text-[11px] font-bold bg-white px-2 py-0.5 rounded-full shadow-sm"
          :class="aiDecisionColors.label"
        >
          {{ confidencePercentage }} Confidence
        </span>
      </div>
      <!-- Confidence bar -->
      <div class="w-full bg-white/50 h-2 rounded-full overflow-hidden mb-3">
        <div
          class="h-full rounded-full transition-all duration-500"
          :class="article.aiDecision === 'include' ? 'bg-emerald-500' : 'bg-rose-400'"
          :style="{ width: confidenceBarWidth }"
        />
      </div>
      <p
        v-if="article.aiReasoning"
        class="text-body-sm leading-relaxed"
        :class="aiDecisionColors.text"
      >
        <span class="font-semibold">Reasoning:</span> {{ displayReasoning }}
      </p>
    </div>
  </section>
</template>
