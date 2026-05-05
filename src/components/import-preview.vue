<script setup lang="ts">
import type { PreviewArticle, ImportError } from '@/composables/use-import';

const props = defineProps<{
  articles: PreviewArticle[];
  errorCount: number;
  errors: ImportError[];
  removedIndices: Set<number>;
}>();

defineEmits<{
  remove: [index: number];
}>();
</script>

<template>
  <div class="preview">
    <div v-if="errorCount > 0" class="preview__errors">
      <h2>Validation Errors ({{ errorCount }})</h2>
      <ul class="preview__error-list">
        <li v-for="err in errors" :key="err.recordIndex" class="preview__error-item">
          Record {{ err.recordIndex }}: {{ err.message }}
        </li>
      </ul>
    </div>

    <div class="preview__table-wrapper">
      <table class="preview__table">
        <thead>
          <tr>
            <th>Title</th>
            <th>Authors</th>
            <th>Year</th>
            <th>Journal</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <template v-for="(article, i) in articles" :key="i">
            <tr v-if="!removedIndices.has(i)">
              <td>{{ article.title }}</td>
              <td>{{ article.authors.join('; ') }}</td>
              <td>{{ article.publicationYear ?? '—' }}</td>
              <td>{{ article.journal ?? '—' }}</td>
              <td><button class="preview__remove" @click="$emit('remove', i)">×</button></td>
            </tr>
          </template>
        </tbody>
      </table>
      <p class="preview__note">Showing {{ articles.length - props.removedIndices.size }} articles</p>
    </div>
  </div>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.preview__errors {
  padding: var(--space-3);
  background-color: var(--color-error-container);
  border-radius: var(--radius-default);
}

.preview__errors h2 {
  font-size: var(--font-size-h2);
  color: var(--color-error);
  margin-bottom: var(--space-2);
}

.preview__error-list {
  list-style: none;
  font-size: var(--font-size-caption);
  color: var(--color-error);
}

.preview__error-item {
  padding: var(--space-1) 0;
}

.preview__table-wrapper {
  overflow-x: auto;
}

.preview__table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--font-size-caption);
}

.preview__table th {
  text-align: left;
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
  color: var(--color-on-surface-variant);
}

.preview__table td {
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  color: var(--color-on-surface);
}

.preview__table tr:hover td {
  background-color: var(--color-hover);
}

.preview__remove {
  background: none;
  border: none;
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-body);
  cursor: pointer;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-default);
  line-height: 1;
  transition: color 0.15s, background-color 0.15s;
}

.preview__remove:hover {
  color: var(--color-error);
  background-color: var(--color-error-container);
}

.preview__note {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  margin-top: var(--space-2);
}
</style>
