import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { ResearchAim, Criterion } from '@/types';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

export const useCriteriaStore = defineStore('criteria', () => {
  const aims = ref<ResearchAim[]>([]);
  const criteria = ref<Criterion[]>([]);
  const loading = ref(false);
  const initialized = ref(false);

  const inclusionCriteria = ref<Criterion[]>([]);
  const exclusionCriteria = ref<Criterion[]>([]);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetchAll();
  }

  async function fetchAll(): Promise<void> {
    loading.value = true;
    try {
      const [aimsResult, criteriaResult] = await Promise.all([
        tauriCommand<ResearchAim[]>('get_research_aims'),
        tauriCommand<Criterion[]>('get_criteria'),
      ]);
      aims.value = aimsResult;
      criteria.value = criteriaResult;
      inclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'inclusion');
      exclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'exclusion');
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  function invalidate(): void {
    aims.value = [];
    criteria.value = [];
    inclusionCriteria.value = [];
    exclusionCriteria.value = [];
    initialized.value = false;
  }

  /** Re-fetch from backend without clearing arrays first (preserves scroll position). */
  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      const [aimsResult, criteriaResult] = await Promise.all([
        tauriCommand<ResearchAim[]>('get_research_aims'),
        tauriCommand<Criterion[]>('get_criteria'),
      ]);
      aims.value = aimsResult;
      criteria.value = criteriaResult;
      inclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'inclusion');
      exclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'exclusion');
      initialized.value = true;
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
    initialized,
    fetchIfNeeded,
    fetchAll,
    invalidate,
    refresh,
  };
});
