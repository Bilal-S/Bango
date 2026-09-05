<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useZotero } from '@/composables/use-zotero';
import type { ZoteroCollection } from '@/types/zotero';

const emit = defineEmits<{
  collectionSelected: [
    payload: {
      collectionKey: string;
      collectionName: string;
      preview: import('@/composables/use-import').ImportPreview;
      articleKeys: string[];
      libraryVersion: number | null;
      totalItems: number;
      attachmentCount: number;
      noteCount: number;
      tagCount: number;
    },
  ];
  back: [];
}>();

const {
  collections,
  collectionsLoading,
  collectionsError,
  previewLoading,
  previewError,
  loadCollections,
  fetchPreview,
} = useZotero();

onMounted(() => {
  void loadCollections();
});

/** Root-first ordering with children directly after their parents. */
const tree = computed(() => {
  const byParent = new Map<string | null, ZoteroCollection[]>();
  for (const collection of collections.value) {
    const list = byParent.get(collection.parentKey) ?? [];
    list.push(collection);
    byParent.set(collection.parentKey, list);
  }
  const ordered: { collection: ZoteroCollection; depth: number }[] = [];
  const walk = (parent: string | null, depth: number): void => {
    for (const collection of byParent.get(parent) ?? []) {
      ordered.push({ collection, depth });
      walk(collection.key, depth + 1);
    }
  };
  walk(null, 0);
  return ordered;
});

async function select(collection: ZoteroCollection): Promise<void> {
  if (previewLoading.value) return;
  try {
    const preview = await fetchPreview(collection.key);
    emit('collectionSelected', {
      collectionKey: collection.key,
      collectionName: collection.name,
      preview: preview.preview,
      articleKeys: preview.articleKeys,
      libraryVersion: preview.libraryVersion,
      totalItems: preview.totalItems,
      attachmentCount: preview.attachmentCount,
      noteCount: preview.noteCount,
      tagCount: preview.tagCount,
    });
  } catch {
    // fetchPreview stores the message in previewError; the error card renders it.
  }
}
</script>

<template>
  <div class="zotero-picker">
    <p class="zotero-picker__hint">
      Choose a Zotero collection. Subcollection items are included automatically.
    </p>

    <div v-if="collectionsLoading" class="zotero-picker__state">
      <span class="spinner" /> Loading collections...
    </div>

    <div v-else-if="collectionsError" class="zotero-picker__error">
      {{ collectionsError }}
      <button class="btn btn--outline" @click="loadCollections">Retry</button>
    </div>

    <div v-else-if="tree.length === 0" class="zotero-picker__state">
      No collections found in this Zotero library.
    </div>

    <ul v-else class="zotero-picker__list">
      <li v-for="entry in tree" :key="entry.collection.key">
        <button
          class="zotero-picker__item"
          :style="{ paddingLeft: `${12 + entry.depth * 20}px` }"
          :disabled="previewLoading"
          @click="select(entry.collection)"
        >
          {{ entry.depth > 0 ? '- ' : '' }}{{ entry.collection.name }}
        </button>
      </li>
    </ul>

    <div v-if="previewLoading" class="zotero-picker__state">
      <span class="spinner" /> Loading preview...
    </div>
    <div v-if="previewError" class="zotero-picker__error">
      {{ previewError }}
    </div>

    <div class="zotero-picker__actions">
      <button class="btn btn--outline" @click="emit('back')">Back</button>
    </div>
  </div>
</template>

<style scoped>
.zotero-picker {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.zotero-picker__hint {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
}

.zotero-picker__list {
  list-style: none;
  margin: 0;
  padding: 0;
  border: 1px solid var(--color-outline-variant);
  border-radius: var(--radius-default);
  max-height: 320px;
  overflow-y: auto;
}

.zotero-picker__item {
  display: block;
  width: 100%;
  text-align: left;
  padding: var(--space-2) var(--space-3);
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--color-surface-container);
  color: var(--color-on-surface);
  font-size: var(--font-size-body);
  cursor: pointer;
}

.zotero-picker__item:hover {
  background-color: var(--color-surface-container);
}

.zotero-picker__item:disabled {
  opacity: 0.5;
  cursor: wait;
}

.zotero-picker__state {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
}

.zotero-picker__error {
  padding: var(--space-3);
  background-color: var(--color-error-container, #fdecea);
  color: var(--color-on-error-container, #5f2120);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.zotero-picker__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}

.btn {
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
}

.btn--outline {
  background: transparent;
  color: var(--color-on-surface-variant);
  border: 1px solid var(--color-outline);
}
</style>
