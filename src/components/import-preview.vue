<script setup lang="ts">
import { ref } from 'vue';
import type { PreviewArticle, ErrorGroup } from '@/composables/use-import';

const props = defineProps<{
  articles: PreviewArticle[];
  errorCount: number;
  errorGroups: ErrorGroup[];
  removedIndices: Set<number>;
  totalValidCount: number;
}>();

defineEmits<{
  remove: [index: number];
}>();

const expandedGroups = ref<Set<string>>(new Set());

function toggleGroup(message: string): void {
  const next = new Set(expandedGroups.value);
  if (next.has(message)) {
    next.delete(message);
  } else {
    next.add(message);
  }
  expandedGroups.value = next;
}

function isExpanded(message: string): boolean {
  return expandedGroups.value.has(message);
}
</script>

<template>
  <div class="preview">
    <div v-if="errorCount > 0" class="preview__errors">
      <h2>Validation Issues ({{ errorCount }} records affected)</h2>
      <div class="preview__error-groups">
        <div v-for="group in errorGroups" :key="group.message" class="preview__error-group">
          <button
            class="preview__error-summary"
            :aria-expanded="isExpanded(group.message)"
            @click="toggleGroup(group.message)"
          >
            <span class="preview__error-chevron">
              {{ isExpanded(group.message) ? '▾' : '▸' }}
            </span>
            <span class="preview__error-text">
              {{ group.count }} record{{ group.count !== 1 ? 's' : '' }} - {{ group.message }}
            </span>
          </button>
          <div v-if="isExpanded(group.message)" class="preview__error-detail">
            <span class="preview__error-indices">
              Record{{ group.recordIndices.length !== 1 ? 's' : '' }}:
              {{ group.recordIndices.join(', ') }}
            </span>
          </div>
        </div>
      </div>
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
              <td>{{ article.publicationYear ?? '-' }}</td>
              <td>{{ article.journal ?? '-' }}</td>
              <td><button class="preview__remove" @click="$emit('remove', i)">×</button></td>
            </tr>
          </template>
        </tbody>
      </table>
      <p class="preview__note">
        <template v-if="totalValidCount > 10">
          Showing first {{ articles.length - props.removedIndices.size }} articles as sample
        </template>
        <template v-else>
          Showing {{ articles.length - props.removedIndices.size }} articles
        </template>
      </p>
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

.preview__error-groups {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.preview__error-group {
  border-radius: var(--radius-default);
  overflow: hidden;
}

.preview__error-summary {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-2) var(--space-3);
  background: none;
  border: none;
  cursor: pointer;
  font-size: var(--font-size-caption);
  color: var(--color-error);
  text-align: left;
  border-radius: var(--radius-default);
  transition: background-color 0.15s;
}

.preview__error-summary:hover {
  background-color: var(--color-hover);
}

.preview__error-chevron {
  flex-shrink: 0;
  width: 1em;
  text-align: center;
}

.preview__error-text {
  flex: 1;
}

.preview__error-detail {
  padding: var(--space-1) var(--space-3) var(--space-2) calc(1em + var(--space-5));
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.preview__error-indices {
  font-family: var(--font-mono, monospace);
  font-size: 0.85em;
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
  transition:
    color 0.15s,
    background-color 0.15s;
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
