<script setup lang="ts">
import type { OpenAlexResultItem } from '@/types/openalex';

defineProps<{
  item: OpenAlexResultItem;
  selected: boolean;
  detailOpen: boolean;
}>();

const emit = defineEmits<{
  toggleSelect: [];
  openDetail: [];
}>();
</script>

<template>
  <div
    class="result-item"
    :class="{ 'result-item--selected': selected, 'result-item--detail-open': detailOpen }"
    @click="emit('openDetail')"
  >
    <div class="result-item__checkbox" @click.stop>
      <input type="checkbox" :checked="selected" @change="emit('toggleSelect')" />
    </div>

    <div class="result-item__content">
      <h4 class="result-item__title">{{ item.work.title ?? 'Untitled' }}</h4>

      <div class="result-item__meta">
        <span v-if="item.work.authorships.length > 0" class="meta-author">
          {{ item.work.authorships[0]?.author.displayName ?? 'Unknown' }}
          <template v-if="item.work.authorships.length > 1"> et al.</template>
        </span>
        <span v-if="item.work.primaryLocation?.source?.displayName" class="meta-journal">
          {{ item.work.primaryLocation.source.displayName }}
        </span>
        <span v-if="item.work.publicationYear" class="meta-year">{{
          item.work.publicationYear
        }}</span>
        <span v-if="item.work.openAccess?.isOa" class="meta-oa">OA</span>
        <span v-if="item.work.citedByCount > 0" class="meta-cited"
          >Cited: {{ item.work.citedByCount }}</span
        >
      </div>

      <p v-if="item.snippet" class="result-item__snippet">{{ item.snippet }}</p>
      <p v-else class="result-item__snippet result-item__snippet--empty">No abstract available</p>

      <span v-if="item.alreadyInLibrary" class="already-in-library">Already in library</span>
      <span v-if="item.work.isRetracted" class="retracted-badge">Retracted</span>
    </div>
  </div>
</template>

<style scoped>
.result-item {
  display: flex;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  border: 1px solid #e2e8f0;
  border-radius: 0.375rem;
  background: white;
  transition:
    border-color 0.15s,
    background 0.15s,
    box-shadow 0.15s;
  cursor: pointer;
}

.result-item:hover {
  border-color: #94a3b8;
}

.result-item--selected {
  border-color: #6366f1;
  background: #eef2ff;
}

.result-item--detail-open {
  border-color: #4f46e5;
  background: #f5f3ff;
  box-shadow: 0 0 0 2px rgba(79, 70, 229, 0.2);
}

.result-item--detail-open.result-item--selected {
  border-color: #4f46e5;
  background: #ede9fe;
  box-shadow: 0 0 0 2px rgba(79, 70, 229, 0.25);
}

.result-item__checkbox {
  padding-top: 0.125rem;
}

.result-item__content {
  flex: 1;
}

.result-item__title {
  font-weight: 600;
  font-size: 0.875rem;
  color: #1e293b;
  margin-bottom: 0.25rem;
}

.result-item__meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  font-size: 0.75rem;
  color: #64748b;
  margin-bottom: 0.375rem;
}

.meta-oa {
  color: #16a34a;
  font-weight: 500;
}

.meta-cited {
  color: #475569;
}

.result-item__snippet {
  font-size: 0.8125rem;
  color: #475569;
  line-height: 1.4;
}

.result-item__snippet--empty {
  color: #94a3b8;
  font-style: italic;
}

.already-in-library {
  display: inline-block;
  margin-top: 0.25rem;
  padding: 0.125rem 0.5rem;
  font-size: 0.6875rem;
  color: #64748b;
  background: #f1f5f9;
  border-radius: 0.25rem;
}

.retracted-badge {
  display: inline-block;
  margin-top: 0.25rem;
  margin-left: 0.5rem;
  padding: 0.125rem 0.5rem;
  font-size: 0.6875rem;
  color: white;
  background: #dc2626;
  border-radius: 0.25rem;
}
</style>
