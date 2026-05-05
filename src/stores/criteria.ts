import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { ResearchAim, Criterion } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useCriteriaStore = defineStore('criteria', () => {
  const aims = ref<ResearchAim[]>([]);
  const criteria = ref<Criterion[]>([]);
  const loading = ref(false);

  const inclusionCriteria = ref<Criterion[]>([]);
  const exclusionCriteria = ref<Criterion[]>([]);

  async function fetchAll(): Promise<void> {
    loading.value = true;
    try {
      const [aimsResult, criteriaResult] = await Promise.all([
        tauriCommand<ResearchAim[]>('get_research_aims'),
        tauriCommand<Criterion[]>('get_criteria'),
      ]);
      aims.value = aimsResult;
      criteria.value = criteriaResult;
      inclusionCriteria.value = criteriaResult.filter(
        (c) => c.criterionType === 'inclusion',
      );
      exclusionCriteria.value = criteriaResult.filter(
        (c) => c.criterionType === 'exclusion',
      );
    } finally {
      loading.value = false;
    }
  }

  return {
    aims,
    criteria,
    inclusionCriteria,
    exclusionCriteria,
    loading,
    fetchAll,
  };
});
