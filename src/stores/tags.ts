import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Tag } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useTagsStore = defineStore('tags', () => {
  const tags = ref<Tag[]>([]);
  const loading = ref(false);

  async function fetchTags(): Promise<void> {
    loading.value = true;
    try {
      tags.value = await tauriCommand<Tag[]>('get_tags');
    } finally {
      loading.value = false;
    }
  }

  return { tags, loading, fetchTags };
});
