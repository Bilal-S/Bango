<script setup lang="ts">
import { computed, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useJournalInfo } from '@/composables/use-journal-info';

const props = defineProps<{
  /** journal_index_id to load, or null to hide the card. */
  journalIndexId: string | null;
  /** When true, the journal is a raw (unmatched) title - show the enrichment hint. */
  isRaw?: boolean;
}>();

const emit = defineEmits<{ close: [] }>();

const router = useRouter();
const { info, loading, error, getJournalInfo, clear } = useJournalInfo();

watch(
  () => props.journalIndexId,
  (id) => {
    if (id) {
      void getJournalInfo(id);
    } else {
      clear();
    }
  },
  { immediate: true }
);

const wosCategories = computed(() => {
  const raw = info.value?.webOfScienceCategories ?? '';
  if (!raw.trim()) return [];
  return raw
    .split(';')
    .map((c) => c.trim())
    .filter(Boolean);
});

const maxSpark = computed(() => Math.max(1, ...(info.value?.pubsByYear.map((p) => p.count) ?? [])));

function viewArticles(): void {
  if (!info.value) return;
  void router.push({
    name: 'articles',
    query: { journal: info.value.journalTitle, filterCollapsed: '1', from: 'timeline' },
  });
}
</script>

<template>
  <Transition name="slide">
    <aside
      v-if="journalIndexId"
      class="journal-card"
      role="complementary"
      aria-label="Journal details"
    >
      <!-- Header -->
      <header class="journal-card__header">
        <h3 class="journal-card__title" :title="info?.journalTitle ?? 'Loading…'">
          {{ info?.journalTitle ?? (loading ? 'Loading…' : 'Journal') }}
        </h3>
        <button class="journal-card__close" title="Close" @click="emit('close')">
          <span class="material-symbols-outlined">close</span>
        </button>
      </header>

      <!-- Body -->
      <div class="journal-card__body">
        <div v-if="loading" class="journal-card__loading">
          <span class="material-symbols-outlined journal-card__spin">progress_activity</span>
        </div>

        <div v-else-if="error" class="journal-card__error">
          <span class="material-symbols-outlined">error</span>
          <p>{{ error }}</p>
        </div>

        <template v-else-if="info">
          <!-- Raw fallback hint -->
          <p v-if="isRaw" class="journal-card__raw-hint">
            <span class="material-symbols-outlined">info</span>
            Not matched to the journal index - run Rematch to enrich.
          </p>

          <!-- Metadata grid -->
          <dl class="journal-card__meta">
            <div v-if="info.publisherName" class="journal-card__meta-row">
              <dt>Publisher</dt>
              <dd>{{ info.publisherName }}</dd>
            </div>
            <div v-if="info.publisherAddress" class="journal-card__meta-row">
              <dt>Address</dt>
              <dd>{{ info.publisherAddress }}</dd>
            </div>
            <div v-if="info.issn || info.eissn" class="journal-card__meta-row">
              <dt>ISSN</dt>
              <dd>
                <span v-if="info.issn">{{ info.issn }}</span>
                <span v-if="info.issn && info.eissn"> · eISSN: </span>
                <span v-if="info.eissn">{{ info.eissn }}</span>
              </dd>
            </div>
            <div v-if="info.languages" class="journal-card__meta-row">
              <dt>Languages</dt>
              <dd>{{ info.languages }}</dd>
            </div>
          </dl>

          <!-- WoS categories -->
          <div v-if="wosCategories.length > 0" class="journal-card__categories">
            <span class="journal-card__categories-label">Web of Science Categories</span>
            <div class="journal-card__chips">
              <span v-for="cat in wosCategories" :key="cat" class="journal-card__chip">{{
                cat
              }}</span>
            </div>
          </div>

          <!-- Stats row -->
          <div class="journal-card__stats">
            <div class="journal-card__stat">
              <span class="journal-card__stat-value">{{ info.articleCount }}</span>
              <span class="journal-card__stat-label">Articles</span>
            </div>
            <div class="journal-card__stat">
              <span class="journal-card__stat-value">
                {{ info.firstYear ?? '-' }}{{ info.firstYear && info.lastYear ? '–' : ''
                }}{{ info.lastYear ?? '' }}
              </span>
              <span class="journal-card__stat-label">Years</span>
            </div>
            <div class="journal-card__stat">
              <span class="journal-card__stat-value">{{
                info.citationsTotal.toLocaleString()
              }}</span>
              <span class="journal-card__stat-label">Citations</span>
            </div>
          </div>

          <!-- Mini sparkline -->
          <div v-if="info.pubsByYear.length > 0" class="journal-card__sparkline">
            <div class="journal-card__spark-bars">
              <div
                v-for="yc in info.pubsByYear"
                :key="yc.year"
                class="journal-card__spark-bar"
                :style="{ height: (yc.count / maxSpark) * 100 + '%' }"
              >
                <span class="journal-card__spark-tooltip">{{ yc.year }}: {{ yc.count }}</span>
              </div>
            </div>
            <div class="journal-card__spark-years">
              <span>{{ info.pubsByYear[0]?.year }}</span>
              <span>{{ info.pubsByYear[info.pubsByYear.length - 1]?.year }}</span>
            </div>
          </div>

          <!-- View articles deep-link -->
          <button class="journal-card__view-btn" @click="viewArticles">
            <span class="material-symbols-outlined">article</span>
            View this journal's articles
          </button>
        </template>
      </div>
    </aside>
  </Transition>
</template>

<style scoped>
.journal-card {
  position: absolute;
  top: 0;
  right: 0;
  height: 100%;
  width: 20rem;
  background: #ffffff;
  border-left: 1px solid var(--color-border, #e2e8f0);
  box-shadow: -8px 0 24px rgba(0, 0, 0, 0.08);
  z-index: 40;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.journal-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid #f1f5f9;
  flex-shrink: 0;
}

.journal-card__title {
  font-size: 0.875rem;
  font-weight: 600;
  color: #1e293b;
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.journal-card__close {
  background: none;
  border: none;
  cursor: pointer;
  padding: 0.25rem;
  border-radius: 0.25rem;
  color: #94a3b8;
  display: flex;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.journal-card__close:hover {
  background: #f1f5f9;
  color: #475569;
}

.journal-card__close .material-symbols-outlined {
  font-size: 1.125rem;
}

.journal-card__body {
  flex: 1;
  overflow-y: auto;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.journal-card__loading,
.journal-card__error {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  padding: 2rem 0;
  color: #94a3b8;
}

.journal-card__spin {
  animation: journal-spin 1s linear infinite;
  font-size: 1.5rem;
  color: var(--color-primary, #4f46e5);
}

@keyframes journal-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.journal-card__raw-hint {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  font-size: 0.75rem;
  color: #b45309;
  background: #fef3c7;
  border: 1px solid #fde68a;
  border-radius: 0.375rem;
  padding: 0.5rem 0.75rem;
  margin: 0;
}

.journal-card__raw-hint .material-symbols-outlined {
  font-size: 1rem;
  flex-shrink: 0;
  margin-top: 0.0625rem;
}

.journal-card__meta {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.journal-card__meta-row {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}

.journal-card__meta-row dt {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #94a3b8;
}

.journal-card__meta-row dd {
  margin: 0;
  font-size: 0.8125rem;
  color: #334155;
  line-height: 1.4;
}

.journal-card__categories {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.journal-card__categories-label {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #94a3b8;
}

.journal-card__chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
}

.journal-card__chip {
  font-size: 0.6875rem;
  background: #f1f5f9;
  color: #475569;
  border-radius: 0.25rem;
  padding: 0.125rem 0.5rem;
  border: 1px solid #e2e8f0;
}

.journal-card__stats {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.5rem;
}

.journal-card__stat {
  background: #f8fafc;
  border-radius: 0.375rem;
  padding: 0.5rem;
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}

.journal-card__stat-value {
  font-size: 1rem;
  font-weight: 700;
  color: var(--color-primary, #4f46e5);
  line-height: 1.1;
}

.journal-card__stat-label {
  font-size: 0.625rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #94a3b8;
  font-weight: 600;
}

.journal-card__sparkline {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.journal-card__spark-bars {
  display: flex;
  align-items: flex-end;
  gap: 1px;
  height: 2.5rem;
}

.journal-card__spark-bar {
  flex: 1;
  min-width: 0;
  min-height: 2px;
  background: linear-gradient(to top, #fbbf24, #f59e0b);
  border-radius: 2px 2px 0 0;
  position: relative;
  cursor: default;
}

.journal-card__spark-tooltip {
  position: absolute;
  bottom: 100%;
  left: 50%;
  transform: translateX(-50%);
  background: #1e293b;
  color: #fff;
  font-size: 0.625rem;
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
  white-space: nowrap;
  opacity: 0;
  transition: opacity 0.15s;
  pointer-events: none;
}

.journal-card__spark-bar:hover .journal-card__spark-tooltip {
  opacity: 1;
}

.journal-card__spark-years {
  display: flex;
  justify-content: space-between;
  font-size: 0.5625rem;
  color: #94a3b8;
}

.journal-card__view-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  padding: 0.5rem 0.75rem;
  background: var(--color-primary, #4f46e5);
  color: #ffffff;
  border: none;
  border-radius: 0.375rem;
  font-size: 0.8125rem;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  transition: opacity 0.15s;
  margin-top: auto;
}

.journal-card__view-btn:hover {
  opacity: 0.9;
}

.journal-card__view-btn .material-symbols-outlined {
  font-size: 1.125rem;
}

/* Slide transition (matches keyword-detail-panel.vue) */
.slide-enter-active,
.slide-leave-active {
  transition:
    transform 0.25s ease,
    opacity 0.25s ease;
}
.slide-enter-from,
.slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
</style>
