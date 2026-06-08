<script setup lang="ts">
import { computed, ref } from 'vue';
import type { Article } from '@/types';
import { useCriteriaStore } from '@/stores/criteria';
import CriteriaEditDialog from './criteria-edit-dialog.vue';

const props = defineProps<{
  article: Article;
}>();

const emit = defineEmits<{
  updateCriteria: [id: string, inclusionIds: string[], exclusionIds: string[]];
}>();

const criteriaStore = useCriteriaStore();

const showCriteriaDialog = ref(false);

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

/** Resolve a criterion UUID to its human-readable text */
function criterionText(id: string): string {
  return criteriaStore.criteria.find((c) => c.id === id)?.text ?? id;
}

function truncate(text: string, max = 20): string {
  return text.length > max ? text.slice(0, max) + '…' : text;
}

function handleCriteriaSave(
  _articleId: string,
  inclusionIds: string[],
  exclusionIds: string[]
): void {
  emit('updateCriteria', props.article.id, inclusionIds, exclusionIds);
}
</script>

<template>
  <section>
    <div class="flex items-center justify-between mb-3">
      <h3 class="text-xs font-label-caps text-slate-500 uppercase tracking-wider">
        Matched Criteria
      </h3>
      <button
        class="material-symbols-outlined text-[16px] text-slate-400 hover:text-indigo-600 cursor-pointer"
        title="Edit matched criteria"
        @click="showCriteriaDialog = true"
      >
        edit
      </button>
    </div>
    <template
      v-if="
        article.matchedInclusionCriteria.length > 0 || article.matchedExclusionCriteria.length > 0
      "
    >
      <div class="grid grid-cols-2 gap-x-3 gap-y-1.5">
        <div
          v-for="criterion in article.matchedInclusionCriteria"
          :key="'inc-' + criterion"
          class="flex items-center gap-1.5 text-body-sm"
          :title="criterionText(criterion)"
        >
          <span
            class="text-[12px] font-bold text-emerald-600 bg-emerald-50 rounded px-1 leading-tight"
            >{{ criterionIndexMap.get(criterion) ?? '-' }}</span
          >
          <span class="truncate">{{ truncate(criterionText(criterion)) }}</span>
        </div>
        <div
          v-for="criterion in article.matchedExclusionCriteria"
          :key="'exc-' + criterion"
          class="flex items-center gap-1.5 text-body-sm text-slate-400"
          :title="criterionText(criterion)"
        >
          <span class="text-[12px] font-bold text-rose-500 bg-rose-50 rounded px-1 leading-tight">{{
            criterionIndexMap.get(criterion) ?? '-'
          }}</span>
          <span class="truncate line-through">{{ truncate(criterionText(criterion)) }}</span>
        </div>
      </div>
    </template>
    <p v-else class="text-xs text-slate-400 italic">No criteria matched. Click edit to assign.</p>

    <CriteriaEditDialog
      v-model="showCriteriaDialog"
      :article-id="article.id"
      :matched-inclusion-ids="article.matchedInclusionCriteria"
      :matched-exclusion-ids="article.matchedExclusionCriteria"
      :inclusion-criteria="criteriaStore.inclusionCriteria"
      :exclusion-criteria="criteriaStore.exclusionCriteria"
      @save="handleCriteriaSave"
    />
  </section>
</template>
