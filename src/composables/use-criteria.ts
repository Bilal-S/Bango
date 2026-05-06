import { ref, onMounted } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { ResearchAim, Criterion } from '@/types';

export function useCriteria() {
  const aims = ref<ResearchAim[]>([]);
  const criteria = ref<Criterion[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  onMounted(() => {
    loadAll();
  });

  async function loadAll() {
    loading.value = true;
    error.value = null;
    try {
      const [loadedAims, loadedCriteria] = await Promise.all([
        tauriCommand<ResearchAim[]>('get_research_aims'),
        tauriCommand<Criterion[]>('get_criteria'),
      ]);
      aims.value = loadedAims;
      criteria.value = loadedCriteria;
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      loading.value = false;
    }
  }

  // Aims
  async function addAim(text: string) {
    try {
      const newAim = await tauriCommand<ResearchAim>('create_research_aim', { request: { text } });
      aims.value.push(newAim);
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  async function removeAim(id: string) {
    try {
      await tauriCommand('delete_research_aim', { id });
      aims.value = aims.value.filter((a) => a.id !== id);
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  // Criteria
  async function addCriterion(criterionType: string, text: string, priority: string) {
    try {
      const newCriterion = await tauriCommand<Criterion>('create_criterion', {
        request: { criterionType, text, priority },
      });
      criteria.value.push(newCriterion);
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  async function updateCriterion(id: string, text: string, priority: string) {
    try {
      const updated = await tauriCommand<Criterion>('update_criterion', {
        request: { id, text, priority },
      });
      const index = criteria.value.findIndex((c) => c.id === id);
      if (index !== -1) {
        criteria.value[index] = updated;
      }
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  async function removeCriterion(id: string) {
    try {
      await tauriCommand('delete_criterion', { id });
      criteria.value = criteria.value.filter((c) => c.id !== id);
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  return {
    aims,
    criteria,
    loading,
    error,
    loadAll,
    addAim,
    removeAim,
    addCriterion,
    updateCriterion,
    removeCriterion,
  };
}
