<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { marked } from 'marked';
import { useWiki } from '@/composables/use-wiki';
import type { WikiPage } from '@/types/wiki';

const props = defineProps<{
  slug: string | null;
}>();

const emit = defineEmits<{
  saved: [page: WikiPage];
  cancel: [];
}>();

const { getPage, updatePage } = useWiki();

const originalPage = ref<WikiPage | null>(null);
const title = ref('');
const summary = ref('');
const body = ref('');
const loading = ref(false);
const saving = ref(false);
const error = ref<string | null>(null);

const isDirty = computed(() => {
  if (!originalPage.value) return false;
  return (
    title.value !== originalPage.value.title ||
    summary.value !== originalPage.value.summary ||
    body.value !== originalPage.value.body
  );
});

const previewHtml = computed(() => {
  if (!body.value) return '';
  return marked.parse(body.value) as string;
});

async function loadPage(): Promise<void> {
  if (!props.slug) {
    originalPage.value = null;
    return;
  }
  loading.value = true;
  error.value = null;
  try {
    const p = await getPage(props.slug);
    if (!p) {
      error.value = `Page "${props.slug}" not found.`;
      return;
    }
    originalPage.value = p;
    title.value = p.title;
    summary.value = p.summary;
    body.value = p.body;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function handleSave(): Promise<void> {
  if (!props.slug) return;
  saving.value = true;
  error.value = null;
  try {
    const updated = await updatePage(props.slug, title.value, summary.value, body.value);
    originalPage.value = updated;
    emit('saved', updated);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    saving.value = false;
  }
}

watch(() => props.slug, loadPage, { immediate: true });
</script>

<template>
  <div class="wiki-page-editor">
    <div v-if="loading" class="wiki-page-editor__loading">Loading...</div>
    <div v-else-if="error" class="wiki-page-editor__error">{{ error }}</div>
    <div v-else-if="!originalPage" class="wiki-page-editor__empty">No page selected.</div>
    <template v-else>
      <div class="wiki-page-editor__toolbar">
        <h2>Edit: {{ originalPage.title }}</h2>
        <div class="wiki-page-editor__actions">
          <button class="btn btn--secondary" :disabled="!isDirty || saving" @click="emit('cancel')">
            Cancel
          </button>
          <button class="btn btn--primary" :disabled="!isDirty || saving" @click="handleSave">
            {{ saving ? 'Saving...' : 'Save' }}
          </button>
        </div>
      </div>

      <div class="field">
        <label class="field__label" for="wiki-title">Title</label>
        <input
          id="wiki-title"
          v-model="title"
          class="field__input"
          type="text"
          placeholder="Page title"
        />
      </div>

      <div class="field">
        <label class="field__label" for="wiki-summary">Summary</label>
        <input
          id="wiki-summary"
          v-model="summary"
          class="field__input"
          type="text"
          placeholder="One-sentence summary"
        />
      </div>

      <div class="wiki-page-editor__body">
        <div class="wiki-page-editor__pane">
          <label class="field__label">Body (Markdown)</label>
          <textarea
            v-model="body"
            class="wiki-page-editor__textarea"
            placeholder="# Heading&#10;&#10;Write your page content here. Use [[slug]] for wikilinks."
          />
        </div>
        <div class="wiki-page-editor__pane">
          <label class="field__label">Preview</label>
          <!-- eslint-disable-next-line vue/no-v-html -- preview of local content -->
          <div class="markdown-content wiki-page-editor__preview" v-html="previewHtml" />
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.wiki-page-editor {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.5rem;
  height: 100%;
  overflow-y: auto;
}

.wiki-page-editor__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.wiki-page-editor__toolbar h2 {
  font-size: 1.125rem;
  font-weight: 600;
}

.wiki-page-editor__actions {
  display: flex;
  gap: 0.5rem;
}

.wiki-page-editor__body {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;
  flex: 1;
  min-height: 300px;
}

.wiki-page-editor__pane {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.wiki-page-editor__textarea {
  flex: 1;
  resize: none;
  padding: 0.75rem;
  border: 1px solid rgb(203 213 225);
  border-radius: 0.5rem;
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.8rem;
  line-height: 1.5;
  background: rgb(248 250 252);
}

.wiki-page-editor__preview {
  flex: 1;
  padding: 0.75rem;
  border: 1px solid rgb(203 213 225);
  border-radius: 0.5rem;
  overflow-y: auto;
  background: #fff;
}

.wiki-page-editor__loading,
.wiki-page-editor__error,
.wiki-page-editor__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: rgb(148 163 184);
}

.wiki-page-editor__error {
  color: rgb(239 68 68);
}

.field__label {
  display: block;
  font-size: 0.75rem;
  font-weight: 600;
  color: rgb(71 85 105);
  margin-bottom: 0.25rem;
}

.field__input {
  width: 100%;
  padding: 0.5rem 0.75rem;
  border: 1px solid rgb(203 213 225);
  border-radius: 0.375rem;
  font-size: 0.875rem;
}

.btn {
  padding: 0.375rem 0.75rem;
  border-radius: 0.375rem;
  border: 1px solid rgb(226 232 240);
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
}

.btn--primary {
  background: rgb(79 70 229);
  border-color: rgb(79 70 229);
  color: #fff;
}

.btn--secondary {
  background: #fff;
  color: rgb(71 85 105);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
