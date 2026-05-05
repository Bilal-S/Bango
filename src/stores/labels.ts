import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Label, LabelWithCount } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useLabelsStore = defineStore('labels', () => {
  const labels = ref<LabelWithCount[]>([]);
  const loading = ref(false);
  const suggesting = ref(false);

  async function fetchLabels(): Promise<void> {
    loading.value = true;
    try {
      labels.value = await tauriCommand<LabelWithCount[]>('get_labels_with_counts');
    } finally {
      loading.value = false;
    }
  }

  async function createLabel(name: string): Promise<void> {
    await tauriCommand<Label>('create_label', { request: { name } });
    await fetchLabels();
  }

  async function renameLabel(id: string, newName: string): Promise<void> {
    await tauriCommand<Label>('rename_label', { request: { id, newName } });
    await fetchLabels();
  }

  async function deleteLabel(id: string): Promise<void> {
    await tauriCommand('delete_label', { id });
    await fetchLabels();
  }

  async function suggestLabels(): Promise<void> {
    suggesting.value = true;
    try {
      await tauriCommand('suggest_labels');
      await fetchLabels();
    } finally {
      suggesting.value = false;
    }
  }

  return {
    labels,
    loading,
    suggesting,
    fetchLabels,
    createLabel,
    renameLabel,
    deleteLabel,
    suggestLabels,
  };
});
