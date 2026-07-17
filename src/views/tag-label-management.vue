<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import TagLabelPanel from '@/components/tag-label-panel.vue';

const router = useRouter();
const tagsStore = useTagsStore();
const labelsStore = useLabelsStore();

// Re-fetch on mount in case stores were invalidated (e.g. after project backup import)
onMounted(() => {
  Promise.all([tagsStore.fetchIfNeeded(), labelsStore.fetchIfNeeded()]);
});

const isLoading = computed(() => tagsStore.loading && labelsStore.loading);
const hasError = computed(() => tagsStore.error || labelsStore.error);
const errorMessage = computed(() => tagsStore.error || labelsStore.error || 'Unknown error');

// Stores are pre-warmed at startup - no onMounted fetch needed.
// Only refetch if something went wrong (error state) or store was invalidated.
async function retry(): Promise<void> {
  tagsStore.invalidate();
  labelsStore.invalidate();
  await Promise.all([tagsStore.fetchIfNeeded(), labelsStore.fetchIfNeeded()]);
}

// ── Tag panel wiring ───────────────────────────────────────────────────
function onCreateTag(name: string): void {
  void tagsStore.createTag(name);
}
function onRenameTag(id: string, newName: string): void {
  void tagsStore.renameTag(id, newName);
}
function onDeleteTag(id: string): void {
  void tagsStore.deleteTag(id);
}
function onUpdateTagColor(id: string, color: string | null): void {
  void tagsStore.updateTagColor(id, color);
}
function onFilterTag(tagId: string): void {
  void router.push({
    path: '/articles',
    query: { tags: tagId, status: 'all', filterCollapsed: '1' },
  });
}
function onSuggestTags(): void {
  void tagsStore.suggestTags();
}

// ── Label panel wiring ─────────────────────────────────────────────────
function onCreateLabel(name: string): void {
  void labelsStore.createLabel(name);
}
function onRenameLabel(id: string, newName: string): void {
  void labelsStore.renameLabel(id, newName);
}
function onDeleteLabel(id: string): void {
  void labelsStore.deleteLabel(id);
}
function onUpdateLabelColor(id: string, color: string | null): void {
  void labelsStore.updateLabelColor(id, color);
}
function onFilterLabel(labelId: string): void {
  void router.push({
    path: '/articles',
    query: { labels: labelId, status: 'all', filterCollapsed: '1' },
  });
}
function onSuggestLabels(): void {
  void labelsStore.suggestLabels();
}
</script>

<template>
  <div class="p-container-padding bg-surface-container-low min-h-full">
    <div class="max-w-7xl mx-auto space-y-stack-gap">
      <!-- Page Header -->
      <div class="flex items-center justify-between pb-4">
        <div>
          <h1 class="page-title">Tags & Labels</h1>
          <p class="font-body-main text-body-main text-on-surface-variant mt-1">
            Organize your academic taxonomy and workflow states.
          </p>
        </div>
      </div>

      <!-- Error State -->
      <div
        v-if="hasError && !isLoading"
        class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm p-6 text-center"
      >
        <span class="material-symbols-outlined text-error text-[32px] mb-2 block">cloud_off</span>
        <h2 class="font-h2 text-h2 text-on-surface mb-1">Unable to load tags & labels</h2>
        <p class="font-body-sm text-body-sm text-on-surface-variant mb-4">
          {{ errorMessage }}
        </p>
        <button
          class="inline-flex items-center gap-2 px-4 py-2 bg-primary-container text-on-primary rounded-lg font-body-main text-body-main font-medium hover:opacity-90 transition-opacity"
          @click="retry"
        >
          <span class="material-symbols-outlined text-[18px]">refresh</span>
          Retry
        </button>
      </div>

      <!-- Loading State -->
      <div
        v-else-if="isLoading"
        class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm p-6 text-center"
      >
        <span class="material-symbols-outlined text-primary text-[32px] mb-2 block animate-spin"
          >progress_activity</span
        >
        <p class="font-body-main text-body-main text-on-surface-variant">Loading tags & labels…</p>
      </div>

      <!-- Dual-Panel Layout -->
      <div v-else class="grid grid-cols-1 lg:grid-cols-2 gap-container-padding items-start">
        <TagLabelPanel
          kind="tag"
          :items="tagsStore.tags"
          :suggesting="tagsStore.suggesting"
          @create="onCreateTag"
          @rename="onRenameTag"
          @delete="onDeleteTag"
          @update-color="onUpdateTagColor"
          @filter="onFilterTag"
          @suggest="onSuggestTags"
        />
        <TagLabelPanel
          kind="label"
          :items="labelsStore.labels"
          :suggesting="labelsStore.suggesting"
          @create="onCreateLabel"
          @rename="onRenameLabel"
          @delete="onDeleteLabel"
          @update-color="onUpdateLabelColor"
          @filter="onFilterLabel"
          @suggest="onSuggestLabels"
        />
      </div>
    </div>
  </div>
</template>
