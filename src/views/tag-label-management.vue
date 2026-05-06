<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import TagChip from '@/components/tag-chip.vue';
import LabelChip from '@/components/label-chip.vue';

const tagsStore = useTagsStore();
const labelsStore = useLabelsStore();

const newTagName = ref('');
const newLabelName = ref('');
const editingTagId = ref<string | null>(null);
const editingTagName = ref('');
const editingLabelId = ref<string | null>(null);
const editingLabelName = ref('');

const isLoading = computed(() => tagsStore.loading && labelsStore.loading);
const hasError = computed(() => tagsStore.error || labelsStore.error);
const errorMessage = computed(() => tagsStore.error || labelsStore.error || 'Unknown error');

onMounted(async () => {
  await Promise.all([tagsStore.fetchTags(), labelsStore.fetchLabels()]);
});

async function addTag(): Promise<void> {
  const name = newTagName.value.trim();
  if (!name) return;
  await tagsStore.createTag(name);
  newTagName.value = '';
}

async function addLabel(): Promise<void> {
  const name = newLabelName.value.trim();
  if (!name) return;
  await labelsStore.createLabel(name);
  newLabelName.value = '';
}

function startEditingTag(id: string, currentName: string): void {
  editingTagId.value = id;
  editingTagName.value = currentName;
}

async function saveTagEdit(): Promise<void> {
  if (!editingTagId.value) return;
  const name = editingTagName.value.trim();
  if (!name) {
    cancelTagEdit();
    return;
  }
  await tagsStore.renameTag(editingTagId.value, name);
  editingTagId.value = null;
  editingTagName.value = '';
}

function cancelTagEdit(): void {
  editingTagId.value = null;
  editingTagName.value = '';
}

function startEditingLabel(id: string, currentName: string): void {
  editingLabelId.value = id;
  editingLabelName.value = currentName;
}

async function saveLabelEdit(): Promise<void> {
  if (!editingLabelId.value) return;
  const name = editingLabelName.value.trim();
  if (!name) {
    cancelLabelEdit();
    return;
  }
  await labelsStore.renameLabel(editingLabelId.value, name);
  editingLabelId.value = null;
  editingLabelName.value = '';
}

function cancelLabelEdit(): void {
  editingLabelId.value = null;
  editingLabelName.value = '';
}

async function retry(): Promise<void> {
  await Promise.all([tagsStore.fetchTags(), labelsStore.fetchLabels()]);
}
</script>

<template>
  <div class="p-container-padding bg-surface-container-low min-h-full">
    <div class="max-w-7xl mx-auto space-y-stack-gap">
      <!-- Page Header -->
      <div class="flex items-center justify-between pb-4">
        <div>
          <h1 class="font-display text-display text-on-surface">Tag & Label Management</h1>
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
        <!-- Tags Panel -->
        <section
          class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm overflow-hidden flex flex-col h-[700px]"
        >
          <div class="p-5 border-b border-surface-variant bg-surface-bright flex-shrink-0">
            <div class="flex items-center justify-between mb-4">
              <div>
                <h2 class="font-h2 text-h2 text-on-surface flex items-center gap-2">
                  <span class="material-symbols-outlined text-primary text-[20px]">sell</span>
                  Tags
                </h2>
                <p class="font-body-sm text-body-sm text-on-surface-variant mt-0.5">
                  Content-category labels for grouping related research.
                </p>
              </div>
              <span
                class="bg-surface-variant text-on-surface-variant px-2 py-0.5 rounded-full font-label-caps text-label-caps"
              >
                {{ tagsStore.tags.length }} Total
              </span>
            </div>
            <div class="flex gap-2">
              <div class="relative flex-1">
                <span
                  class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-outline text-[18px]"
                  >add</span
                >
                <input
                  v-model="newTagName"
                  class="w-full pl-9 pr-3 py-2 bg-surface-container-lowest border border-outline-variant rounded-lg focus:border-primary focus:ring-1 focus:ring-primary font-body-main text-body-main text-on-surface transition-all"
                  placeholder="Add new tag..."
                  type="text"
                  @keyup.enter="addTag"
                />
              </div>
              <button
                class="flex items-center gap-2 px-4 py-2 bg-secondary-container text-on-secondary-container hover:bg-secondary-fixed transition-colors rounded-lg font-body-main text-body-main font-medium border border-secondary-fixed-dim whitespace-nowrap disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="tagsStore.suggesting"
                @click="tagsStore.suggestTags()"
              >
                <span class="material-symbols-outlined text-[18px]">auto_awesome</span>
                {{ tagsStore.suggesting ? 'Generating...' : 'Generate from AI' }}
              </button>
            </div>
          </div>
          <div class="p-5 overflow-y-auto flex-1 space-y-3">
            <div
              v-for="tag in tagsStore.tags"
              :key="tag.id"
              class="flex items-center justify-between group p-2 hover:bg-surface-container rounded-lg transition-colors"
            >
              <div class="flex items-center gap-3">
                <template v-if="editingTagId === tag.id">
                  <input
                    v-model="editingTagName"
                    class="px-2 py-1 bg-surface-container-lowest border border-primary rounded-lg focus:ring-1 focus:ring-primary font-mono text-mono text-on-surface transition-all w-48"
                    @keyup.enter="saveTagEdit"
                    @keyup.escape="cancelTagEdit"
                  />
                </template>
                <template v-else>
                  <TagChip :name="tag.name" />
                </template>
              </div>
              <div class="flex items-center gap-4">
                <span class="font-body-sm text-body-sm text-on-surface-variant"
                  >{{ tag.articleCount }} articles</span
                >
                <div class="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <template v-if="editingTagId === tag.id">
                    <button
                      class="p-1 text-primary hover:bg-surface-variant rounded transition-colors"
                      @click="saveTagEdit"
                    >
                      <span class="material-symbols-outlined text-[16px]">check</span>
                    </button>
                    <button
                      class="p-1 text-outline hover:bg-surface-variant rounded transition-colors"
                      @click="cancelTagEdit"
                    >
                      <span class="material-symbols-outlined text-[16px]">close</span>
                    </button>
                  </template>
                  <template v-else>
                    <button
                      class="p-1 text-outline hover:text-primary rounded hover:bg-surface-variant transition-colors"
                      @click="startEditingTag(tag.id, tag.name)"
                    >
                      <span class="material-symbols-outlined text-[16px]">edit</span>
                    </button>
                    <button
                      class="p-1 text-outline hover:text-error rounded hover:bg-error-container transition-colors"
                      @click="tagsStore.deleteTag(tag.id)"
                    >
                      <span class="material-symbols-outlined text-[16px]">close</span>
                    </button>
                  </template>
                </div>
              </div>
            </div>
            <p
              v-if="tagsStore.tags.length === 0"
              class="text-on-surface-variant font-body-sm text-body-sm text-center py-8"
            >
              No tags yet. Add one above or generate from AI.
            </p>
          </div>
        </section>

        <!-- Labels Panel -->
        <section
          class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm overflow-hidden flex flex-col h-[700px]"
        >
          <div class="p-5 border-b border-surface-variant bg-surface-bright flex-shrink-0">
            <div class="flex items-center justify-between mb-4">
              <div>
                <h2 class="font-h2 text-h2 text-on-surface flex items-center gap-2">
                  <span class="material-symbols-outlined text-secondary text-[20px]"
                    >bookmark_manager</span
                  >
                  Labels
                </h2>
                <p class="font-body-sm text-body-sm text-on-surface-variant mt-0.5">
                  Workflow markers indicating state or priority.
                </p>
              </div>
              <span
                class="bg-surface-variant text-on-surface-variant px-2 py-0.5 rounded-full font-label-caps text-label-caps"
              >
                {{ labelsStore.labels.length }} Total
              </span>
            </div>
            <div class="flex gap-2">
              <div class="relative flex-1">
                <span
                  class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-outline text-[18px]"
                  >add</span
                >
                <input
                  v-model="newLabelName"
                  class="w-full pl-9 pr-3 py-2 bg-surface-container-lowest border border-outline-variant rounded-lg focus:border-secondary focus:ring-1 focus:ring-secondary font-body-main text-body-main text-on-surface transition-all"
                  placeholder="Add new label..."
                  type="text"
                  @keyup.enter="addLabel"
                />
              </div>
              <button
                class="flex items-center gap-2 px-4 py-2 bg-surface-container text-on-surface hover:bg-surface-variant transition-colors rounded-lg font-body-main text-body-main font-medium border border-outline-variant whitespace-nowrap disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="labelsStore.suggesting"
                @click="labelsStore.suggestLabels()"
              >
                <span class="material-symbols-outlined text-[18px]">auto_awesome</span>
                {{ labelsStore.suggesting ? 'Generating...' : 'Generate from AI' }}
              </button>
            </div>
          </div>
          <div class="p-5 overflow-y-auto flex-1 space-y-3">
            <div
              v-for="label in labelsStore.labels"
              :key="label.id"
              class="flex items-center justify-between group p-2 hover:bg-surface-container rounded-lg transition-colors"
            >
              <div class="flex items-center gap-3">
                <template v-if="editingLabelId === label.id">
                  <input
                    v-model="editingLabelName"
                    class="px-2 py-1 bg-surface-container-lowest border border-secondary rounded-lg focus:ring-1 focus:ring-secondary font-mono text-mono text-on-surface transition-all w-48"
                    @keyup.enter="saveLabelEdit"
                    @keyup.escape="cancelLabelEdit"
                  />
                </template>
                <template v-else>
                  <LabelChip :name="label.name" />
                </template>
              </div>
              <div class="flex items-center gap-4">
                <span class="font-body-sm text-body-sm text-on-surface-variant"
                  >{{ label.articleCount }} articles</span
                >
                <div class="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <template v-if="editingLabelId === label.id">
                    <button
                      class="p-1 text-secondary hover:bg-surface-variant rounded transition-colors"
                      @click="saveLabelEdit"
                    >
                      <span class="material-symbols-outlined text-[16px]">check</span>
                    </button>
                    <button
                      class="p-1 text-outline hover:bg-surface-variant rounded transition-colors"
                      @click="cancelLabelEdit"
                    >
                      <span class="material-symbols-outlined text-[16px]">close</span>
                    </button>
                  </template>
                  <template v-else>
                    <button
                      class="p-1 text-outline hover:text-secondary rounded hover:bg-surface-variant transition-colors"
                      @click="startEditingLabel(label.id, label.name)"
                    >
                      <span class="material-symbols-outlined text-[16px]">edit</span>
                    </button>
                    <button
                      class="p-1 text-outline hover:text-error rounded hover:bg-error-container transition-colors"
                      @click="labelsStore.deleteLabel(label.id)"
                    >
                      <span class="material-symbols-outlined text-[16px]">close</span>
                    </button>
                  </template>
                </div>
              </div>
            </div>
            <p
              v-if="labelsStore.labels.length === 0"
              class="text-on-surface-variant font-body-sm text-body-sm text-center py-8"
            >
              No labels yet. Add one above or generate from AI.
            </p>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>
