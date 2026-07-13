<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener';
import type { OpenAlexResultItem } from '@/types/openalex';

defineProps<{
  item: OpenAlexResultItem;
  importing?: boolean;
}>();

const emit = defineEmits<{
  close: [];
  add: [];
}>();

function openExternal(url: string): void {
  openUrl(url).catch((err) => {
    console.error('Failed to open external link:', err);
  });
}
</script>

<template>
  <div class="detail-panel">
    <div class="detail-header">
      <button
        class="btn btn--sm btn--primary"
        :disabled="item.alreadyInLibrary || importing"
        @click="emit('add')"
      >
        {{ importing ? 'Adding...' : 'Add' }}
      </button>
      <button class="btn btn--sm btn--secondary" @click="emit('close')">Close</button>
    </div>

    <div class="detail-body">
      <h3 class="detail-title">{{ item.work.title ?? 'Untitled' }}</h3>

      <!-- Already in library badge -->
      <span v-if="item.alreadyInLibrary" class="already-badge">Already in library</span>

      <!-- Authors with affiliations -->
      <div v-if="item.work.authorships.length > 0" class="detail-section">
        <h4>Authors</h4>
        <div v-for="(authorship, i) in item.work.authorships" :key="i" class="author-row">
          <span class="author-name">{{ authorship.author.displayName ?? 'Unknown' }}</span>
          <span v-if="authorship.institutions.length > 0" class="author-inst">
            {{ authorship.institutions.map((inst) => inst.displayName).join(', ') }}
          </span>
        </div>
      </div>

      <!-- Publication info -->
      <div class="detail-section">
        <h4>Publication</h4>
        <div v-if="item.work.publicationYear" class="detail-field">
          <span class="field-label">Year:</span> {{ item.work.publicationYear }}
        </div>
        <div v-if="item.work.publicationDate" class="detail-field">
          <span class="field-label">Date:</span> {{ item.work.publicationDate }}
        </div>
        <div v-if="item.work.primaryLocation?.source?.displayName" class="detail-field">
          <span class="field-label">Journal:</span>
          {{ item.work.primaryLocation.source.displayName }}
        </div>
        <div v-if="item.work.biblio?.volume" class="detail-field">
          <span class="field-label">Volume:</span> {{ item.work.biblio.volume }}
        </div>
        <div v-if="item.work.biblio?.issue" class="detail-field">
          <span class="field-label">Issue:</span> {{ item.work.biblio.issue }}
        </div>
        <div v-if="item.work.biblio?.firstPage" class="detail-field">
          <span class="field-label">Pages:</span> {{ item.work.biblio.firstPage }}-{{
            item.work.biblio.lastPage
          }}
        </div>
      </div>

      <!-- DOI -->
      <div v-if="item.work.doi" class="detail-section">
        <h4>DOI</h4>
        <a class="doi-link" @click.prevent="openExternal(item.work.doi!)">{{ item.work.doi }}</a>
      </div>

      <!-- Metrics -->
      <div class="detail-section">
        <h4>Metrics</h4>
        <div class="detail-field">
          <span class="field-label">Cited by:</span> {{ item.work.citedByCount }}
        </div>
      </div>

      <!-- Language + Type -->
      <div class="detail-section">
        <div v-if="item.work.language" class="detail-field">
          <span class="field-label">Language:</span> {{ item.work.language }}
        </div>
        <div v-if="item.work.type" class="detail-field">
          <span class="field-label">Type:</span> {{ item.work.type }}
        </div>
      </div>

      <!-- Keywords -->
      <div v-if="item.work.keywords.length > 0" class="detail-section">
        <h4>Keywords</h4>
        <div class="keyword-list">
          <span v-for="kw in item.work.keywords" :key="kw.displayName" class="keyword-chip">
            {{ kw.displayName }}
          </span>
        </div>
      </div>

      <!-- Abstract -->
      <div v-if="item.abstractText" class="detail-section">
        <h4>Abstract</h4>
        <p class="abstract-text">{{ item.abstractText }}</p>
      </div>

      <!-- Open Access -->
      <div v-if="item.work.openAccess" class="detail-section">
        <h4>Open Access</h4>
        <div class="detail-field">
          <span class="field-label">OA Status:</span>
          {{ item.work.openAccess.oaStatus ?? 'Unknown' }}
        </div>
        <a
          v-if="item.work.openAccess.oaUrl"
          class="oa-link"
          @click.prevent="openExternal(item.work.openAccess.oaUrl!)"
        >
          Open Access URL
        </a>
        <a
          v-if="item.work.primaryLocation?.pdfUrl"
          class="oa-link"
          @click.prevent="openExternal(item.work.primaryLocation.pdfUrl!)"
        >
          PDF
        </a>
      </div>

      <!-- OpenAlex link -->
      <div class="detail-section">
        <a class="openalex-link" @click.prevent="openExternal(item.work.id)">Open in OpenAlex</a>
      </div>
    </div>
  </div>
</template>

<style scoped>
.detail-panel {
  width: 100%;
  height: 100%;
  background: white;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  border-left: 1px solid #e2e8f0;
}

.detail-header {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  padding: 0.5rem 1rem;
  border-bottom: 1px solid #e2e8f0;
  position: sticky;
  top: 0;
  background: white;
  z-index: 1;
}

.detail-body {
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.detail-title {
  font-size: 1.125rem;
  font-weight: 700;
  color: #1e293b;
  line-height: 1.3;
}

.already-badge {
  display: inline-block;
  padding: 0.125rem 0.5rem;
  font-size: 0.6875rem;
  color: #64748b;
  background: #f1f5f9;
  border-radius: 0.25rem;
  align-self: flex-start;
}

.detail-section h4 {
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #64748b;
  margin-bottom: 0.375rem;
}

.detail-field {
  font-size: 0.8125rem;
  color: #334155;
  margin-bottom: 0.25rem;
}

.field-label {
  font-weight: 500;
  color: #64748b;
}

.author-row {
  font-size: 0.8125rem;
  margin-bottom: 0.25rem;
}

.author-name {
  font-weight: 500;
  color: #1e293b;
}

.author-inst {
  color: #64748b;
  margin-left: 0.5rem;
}

.doi-link,
.oa-link,
.openalex-link {
  font-size: 0.8125rem;
  color: #4f46e5;
  text-decoration: none;
  display: inline-block;
  margin-top: 0.25rem;
  cursor: pointer;
}

.doi-link:hover,
.oa-link:hover,
.openalex-link:hover {
  text-decoration: underline;
}

.keyword-list {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
}

.keyword-chip {
  padding: 0.125rem 0.5rem;
  font-size: 0.6875rem;
  background: #f1f5f9;
  border-radius: 0.25rem;
  color: #475569;
}

.abstract-text {
  font-size: 0.8125rem;
  line-height: 1.5;
  color: #334155;
}

.btn--sm {
  padding: 0.25rem 0.75rem;
  font-size: 0.75rem;
  font-weight: 500;
  border-radius: 0.25rem;
  border: 1px solid #cbd5e1;
  cursor: pointer;
  transition: background 0.15s;
}

.btn--primary {
  background: #4f46e5;
  color: white;
  border-color: #4f46e5;
}

.btn--primary:hover:not(:disabled) {
  background: #4338ca;
}

.btn--primary:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.btn--secondary {
  background: white;
  color: #334155;
}

.btn--secondary:hover {
  background: #f1f5f9;
}
</style>
