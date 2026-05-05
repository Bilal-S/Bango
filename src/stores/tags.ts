import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Tag } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useTagsStore = defineStore('tags', () => {
  const tags = ref<Tag[]>([]);
  const loading = ref(false);
  const suggesting = ref(false);

  async function fetchTags(): Promise<void> {
    loading.value = true;
    try {
      tags.value = await tauriCommand<Tag[]>('get_tags');
    } finally {
      loading.value = false;
    }
  }

  async function createTag(name: string): Promise<void> {
    await tauriCommand<Tag>('create_tag', { request: { name } });
    await fetchTags();
  }

  async function renameTag(id: string, newName: string): Promise<void> {
    await tauriCommand<Tag>('rename_tag', { request: { id, newName } });
    await fetchTags();
  }

  async function deleteTag(id: string): Promise<void> {
    await tauriCommand('delete_tag', { id });
    await fetchTags();
  }

  async function suggestTags(): Promise<void> {
    suggesting.value = true;
    try {
      await tauriCommand('suggest_tags');
      await fetchTags();
    } finally {
      suggesting.value = false;
    }
  }

  return { tags, loading, suggesting, fetchTags, createTag, renameTag, deleteTag, suggestTags };
});
