<script setup lang="ts">
import { computed, ref } from 'vue';
import type { Article } from '@/types';
import { useCriteriaStore } from '@/stores/criteria';

const props = defineProps<{
  article: Article;
}>();

const emit = defineEmits<{
  /** Fired when the user clicks the trashcan icon in the expanded header.
   *  The parent owns the confirmation dialog + IPC call. */
  clearReasoning: [id: string];
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

const criterionIndexMap = computed(() => criteriaStore.criterionIndexMap);

/**
 * Replace criterion UUIDs in reasoning text with global numbered references `[n]`.
 * Also collapses double brackets `[[n]]` to `[n]` from LLM echoing prompt format.
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

// Collapse state persisted across sessions. Mirrors the
// `bango-metadata-expanded` key used by `article-metadata.vue`.
const reasoningExpanded = ref(localStorage.getItem('bango-ai-reasoning-expanded') !== 'false');

function toggleExpanded(): void {
  reasoningExpanded.value = !reasoningExpanded.value;
  localStorage.setItem('bango-ai-reasoning-expanded', String(reasoningExpanded.value));
}

/** Stop click propagation so the trash icon does not toggle the header. */
function onDeleteClick(event: MouseEvent): void {
  event.stopPropagation();
  emit('clearReasoning', props.article.id);
}
</script>

<template>
  <section v-if="aiDecisionLabel && aiDecisionColors">
    <div
      class="rounded-xl border overflow-hidden"
      :class="[aiDecisionColors.bg, aiDecisionColors.border]"
    >
      <!-- Collapsible header. Clicking anywhere toggles; the trash icon
           stops propagation so it does not toggle. -->
      <button
        type="button"
        class="w-full grid grid-cols-3 items-center px-4 py-3 cursor-pointer transition-colors hover:bg-black/[0.03]"
        @click="toggleExpanded"
      >
        <!-- Left: decision icon + label -->
        <span class="flex items-center gap-2 justify-self-start min-w-0">
          <span class="material-symbols-outlined shrink-0" :class="aiDecisionColors.icon">
            {{ article.aiDecision === 'include' ? 'verified' : 'cancel' }}
          </span>
          <span class="font-bold truncate" :class="aiDecisionColors.label">
            {{ aiDecisionLabel }}
          </span>
        </span>
        <!-- Center: confidence pill (truly centered in its grid column) -->
        <span
          class="justify-self-center text-[11px] font-bold bg-white px-2 py-0.5 rounded-full shadow-sm"
          :class="aiDecisionColors.label"
        >
          {{ confidencePercentage }} Confidence
        </span>
        <!-- Right: delete icon (expanded only) + caret -->
        <span class="flex items-center gap-0.5 justify-self-end shrink-0">
          <span
            v-if="reasoningExpanded"
            role="button"
            tabindex="0"
            class="material-symbols-outlined text-[18px] text-slate-400 hover:text-rose-600 cursor-pointer transition-colors"
            title="Delete AI reasoning and confidence"
            @click="onDeleteClick"
            @keydown.enter.prevent="onDeleteClick($event as unknown as MouseEvent)"
            @keydown.space.prevent="onDeleteClick($event as unknown as MouseEvent)"
          >
            delete
          </span>
          <span
            class="material-symbols-outlined text-[18px] transition-transform duration-200"
            :class="[aiDecisionColors.icon, { 'rotate-180': reasoningExpanded }]"
          >
            expand_more
          </span>
        </span>
      </button>
      <!-- Expanded body: confidence bar + reasoning paragraph.
           Hidden in collapsed state so the header stays compact. -->
      <div v-show="reasoningExpanded" class="px-4 pb-4">
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
    </div>
  </section>
</template>
