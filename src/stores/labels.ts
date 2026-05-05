import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Label } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useLabelsStore = defineStore('labels', () => {
  const labels = ref<Label[]>([]);
  const loading = ref(false);

  async function fetchLabels(): Promise<void> {
    loading.value = true;
    try {
      labels.value = await tauriCommand<Label[]>('get_labels');
    } finally {
      loading.value = false;
    }
  }

  return { labels, loading, fetchLabels };
});
