import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Label, LabelWithCount, MergeResult } from '@/types';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

const DEMO_LABELS: LabelWithCount[] = [
  { id: 'l1', name: 'priority-read', source: 'user_created', color: '#ef4444', articleCount: 12 },
  { id: 'l2', name: 'disputed', source: 'user_created', color: '#f59e0b', articleCount: 4 },
  { id: 'l3', name: 'needs-review', source: 'ai_generated', color: '#3b82f6', articleCount: 38 },
  {
    id: 'l4',
    name: 'strong-methodology',
    source: 'ai_generated',
    color: '#10b981',
    articleCount: 21,
  },
  { id: 'l5', name: 'needs-full-text', source: 'user_created', color: '#8b5cf6', articleCount: 7 },
  {
    id: 'l6',
    name: 'exclude-candidate',
    source: 'user_created',
    color: '#6b7280',
    articleCount: 15,
  },
];

export const useLabelsStore = defineStore('labels', () => {
  const labels = ref<LabelWithCount[]>([]);
  const loading = ref(false);
  const suggesting = ref(false);
  const error = ref<string | null>(null);
  const initialized = ref(false);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value) return;
    await fetchLabels();
  }

  async function fetchLabels(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      if (!isTauri()) {
        labels.value = DEMO_LABELS;
        initialized.value = true;
        return;
      }
      labels.value = await tauriCommand<LabelWithCount[]>('get_labels_with_counts');
      initialized.value = true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  function invalidate(): void {
    labels.value = [];
    initialized.value = false;
  }

  async function createLabel(name: string): Promise<void> {
    try {
      if (!isTauri()) {
        const newLabel: LabelWithCount = {
          id: String(Date.now()),
          name,
          source: 'user_created',
          color: null,
          articleCount: 0,
        };
        labels.value = [...labels.value, newLabel];
        return;
      }
      await tauriCommand<Label>('create_label', { request: { name } });
      invalidate();
      await fetchLabels();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      invalidate();
      await fetchLabels();
    }
  }

  async function renameLabel(id: string, newName: string): Promise<void> {
    try {
      if (!isTauri()) {
        labels.value = labels.value.map((l) => (l.id === id ? { ...l, name: newName } : l));
        return;
      }
      await tauriCommand<Label>('rename_label', { request: { id, newName } });
      await fetchLabels();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function deleteLabel(id: string): Promise<void> {
    try {
      if (!isTauri()) {
        labels.value = labels.value.filter((l) => l.id !== id);
        return;
      }
      await tauriCommand('delete_label', { id });
      await fetchLabels();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function updateLabelColor(id: string, color: string | null): Promise<void> {
    try {
      if (!isTauri()) {
        labels.value = labels.value.map((l) => (l.id === id ? { ...l, color } : l));
        return;
      }
      await tauriCommand<Label>('update_label_color', { request: { id, color } });
      await fetchLabels();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function suggestLabels(): Promise<void> {
    suggesting.value = true;
    error.value = null;
    try {
      if (!isTauri()) {
        await new Promise((r) => setTimeout(r, 1500));
        const suggested: LabelWithCount[] = [
          {
            id: 's1',
            name: 'high-relevance',
            source: 'ai_generated',
            color: null,
            articleCount: 0,
          },
          { id: 's2', name: 'low-quality', source: 'ai_generated', color: null, articleCount: 0 },
        ];
        // Merge case-insensitive: skip suggestions that already exist
        const existingLower = new Set(labels.value.map((l) => l.name.toLowerCase()));
        const newLabels = suggested.filter((s) => !existingLower.has(s.name.toLowerCase()));
        labels.value = [...labels.value, ...newLabels];
        return;
      }
      await tauriCommand('suggest_labels');
      await fetchLabels();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      suggesting.value = false;
    }
  }

  /** Replace one label with another. Mirrors `mergeTag`. No `invalidate()`. */
  async function mergeLabel(fromId: string, intoId: string): Promise<MergeResult> {
    if (!isTauri()) {
      const from = labels.value.find((l) => l.id === fromId);
      const into = labels.value.find((l) => l.id === intoId);
      if (!from || !into) throw new Error('Label not found');
      const moved = from.articleCount;
      labels.value = labels.value
        .filter((l) => l.id !== fromId)
        .map((l) => (l.id === intoId ? { ...l, articleCount: l.articleCount + moved } : l));
      return {
        fromName: from.name,
        intoName: into.name,
        reassignedCount: moved,
        alreadyHadSurvivorCount: 0,
      };
    }
    const result = await tauriCommand<MergeResult>('merge_label', {
      request: { fromId, intoId },
    });
    await fetchLabels();
    return result;
  }

  return {
    labels,
    loading,
    suggesting,
    error,
    initialized,
    fetchIfNeeded,
    fetchLabels,
    createLabel,
    renameLabel,
    deleteLabel,
    updateLabelColor,
    suggestLabels,
    mergeLabel,
    invalidate,
  };
});
