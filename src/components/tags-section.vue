<script setup lang="ts">
import { ref, computed } from 'vue';
import type { Article } from '@/types';
import { useTagsStore } from '@/stores/tags';
import TagChip from './tag-chip.vue';
import SuggestInput from './suggest-input.vue';

const props = defineProps<{
  article: Article;
}>();

const emit = defineEmits<{
  updateTags: [id: string, tagIds: string[]];
}>();

const tagsStore = useTagsStore();
const newTag = ref('');

// Alphabetically sorted tags (case-insensitive)
const sortedTags = computed(() =>
  [...props.article.tags].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
);

// Suggestions from global store, excluding already-assigned values
const tagSuggestions = computed(() => {
  const assigned = new Set(props.article.tags.map((t) => t.toLowerCase()));
  return tagsStore.tags
    .map((t) => t.name)
    .filter((name) => !assigned.has(name.toLowerCase()))
    .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
});

/** Look up the color for a tag name from the global store */
function tagColor(name: string): string | null {
  return tagsStore.tags.find((t) => t.name === name)?.color ?? null;
}

function removeTag(tag: string): void {
  const updated = props.article.tags.filter((t) => t !== tag);
  emit('updateTags', props.article.id, updated);
}

async function addTag(val: string): Promise<void> {
  if (!val || props.article.tags.includes(val)) return;
  emit('updateTags', props.article.id, [...props.article.tags, val]);
  newTag.value = '';
  // If the tag doesn't exist in the global store, create it
  const existsInStore = tagsStore.tags.some((t) => t.name.toLowerCase() === val.toLowerCase());
  if (!existsInStore) {
    await tagsStore.createTag(val);
    await tagsStore.fetchIfNeeded();
  }
}
</script>

<template>
  <section>
    <h3 class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">Tags</h3>
    <div class="flex flex-wrap gap-2 mb-2">
      <span
        v-for="tag in sortedTags"
        :key="'tag-' + tag"
        class="inline-flex items-center gap-1 group"
      >
        <TagChip :name="tag" :color="tagColor(tag)" />
        <button
          class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer rounded-full hover:bg-slate-100 leading-none opacity-0 group-hover:opacity-100 transition-opacity"
          @click="removeTag(tag)"
        >
          close
        </button>
      </span>
    </div>
    <div class="flex gap-2">
      <SuggestInput
        v-model="newTag"
        :suggestions="tagSuggestions"
        placeholder="Add content tag…"
        class="flex-1"
        @select="addTag"
        @enter="addTag"
      />
    </div>
  </section>
</template>
