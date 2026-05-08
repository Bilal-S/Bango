import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Tag, TagWithCount } from '@/types';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

const DEMO_TAGS: TagWithCount[] = [
  {
    id: '1',
    name: 'machine-learning',
    source: 'user_created',
    color: '#3b82f6',
    articleCount: 142,
  },
  { id: '2', name: 'clinical-trial', source: 'user_created', color: '#10b981', articleCount: 89 },
  { id: '3', name: 'nlp-models', source: 'ai_suggested', color: '#8b5cf6', articleCount: 56 },
  { id: '4', name: 'deep-learning', source: 'user_created', color: '#f59e0b', articleCount: 34 },
  { id: '5', name: 'systematic-review', source: 'ris_keyword', color: '#ef4444', articleCount: 67 },
  { id: '6', name: 'meta-analysis', source: 'ai_suggested', color: '#ec4899', articleCount: 23 },
  { id: '7', name: 'data-extraction', source: 'user_created', color: '#06b6d4', articleCount: 45 },
  { id: '8', name: 'bias-assessment', source: 'ai_suggested', color: '#84cc16', articleCount: 18 },
];

export const useTagsStore = defineStore('tags', () => {
  const tags = ref<TagWithCount[]>([]);
  const loading = ref(false);
  const suggesting = ref(false);
  const error = ref<string | null>(null);
  const initialized = ref(false);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value) return;
    await fetchTags();
  }

  async function fetchTags(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      if (!isTauri()) {
        // Demo data for browser-only mode
        tags.value = DEMO_TAGS;
        initialized.value = true;
        return;
      }
      tags.value = await tauriCommand<TagWithCount[]>('get_tags_with_counts');
      initialized.value = true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  function invalidate(): void {
    tags.value = [];
    initialized.value = false;
  }

  async function createTag(name: string): Promise<void> {
    try {
      if (!isTauri()) {
        const newTag: TagWithCount = {
          id: String(Date.now()),
          name,
          source: 'user_created',
          color: null,
          articleCount: 0,
        };
        tags.value = [...tags.value, newTag];
        return;
      }
      await tauriCommand<Tag>('create_tag', { request: { name } });
      invalidate();
      await fetchTags();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      invalidate();
      await fetchTags();
    }
  }

  async function renameTag(id: string, newName: string): Promise<void> {
    try {
      if (!isTauri()) {
        tags.value = tags.value.map((t) => (t.id === id ? { ...t, name: newName } : t));
        return;
      }
      await tauriCommand<Tag>('rename_tag', { request: { id, newName } });
      await fetchTags();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function deleteTag(id: string): Promise<void> {
    try {
      if (!isTauri()) {
        tags.value = tags.value.filter((t) => t.id !== id);
        return;
      }
      await tauriCommand('delete_tag', { id });
      await fetchTags();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function updateTagColor(id: string, color: string | null): Promise<void> {
    try {
      if (!isTauri()) {
        tags.value = tags.value.map((t) => (t.id === id ? { ...t, color } : t));
        return;
      }
      await tauriCommand<Tag>('update_tag_color', { request: { id, color } });
      await fetchTags();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function suggestTags(): Promise<void> {
    suggesting.value = true;
    error.value = null;
    try {
      if (!isTauri()) {
        // Simulate AI suggestion in demo mode
        await new Promise((r) => setTimeout(r, 1500));
        const suggested: TagWithCount[] = [
          {
            id: 's1',
            name: 'neural-network',
            source: 'ai_suggested',
            color: null,
            articleCount: 0,
          },
          {
            id: 's2',
            name: 'sentiment-analysis',
            source: 'ai_suggested',
            color: null,
            articleCount: 0,
          },
          {
            id: 's3',
            name: 'knowledge-graph',
            source: 'ai_suggested',
            color: null,
            articleCount: 0,
          },
        ];
        tags.value = [...tags.value, ...suggested];
        return;
      }
      await tauriCommand('suggest_tags');
      await fetchTags();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      suggesting.value = false;
    }
  }

  return {
    tags,
    loading,
    suggesting,
    error,
    initialized,
    fetchIfNeeded,
    fetchTags,
    createTag,
    renameTag,
    deleteTag,
    updateTagColor,
    suggestTags,
    invalidate,
  };
});
