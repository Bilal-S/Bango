<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useReferencesSearch } from '@/composables/use-references-search';
import { useToast } from '@/composables/use-toast';
import ReferencePaperDetailPanel from './reference-paper-detail-panel.vue';
import type { ReferencePaperQuery } from '@/types';
import { formatAuthors, doiLink } from '@/utils/formatters';

const emit = defineEmits<{
  (e: 'article-promoted', articleId: string): void;
  (e: 'navigate-to-article', articleId: string): void;
}>();

const toast = useToast();
const {
  searchText,
  statusFilter,
  papers,
  articlesOfInterest,
  loading,
  total,
  currentPage,
  totalPages,
  canGoPrev,
  canGoNext,
  error,
  linkedArticlesMap,
  linkedArticlesLoading,
  search,
  goToPage,
  loadArticlesOfInterest,
  loadLinkedArticles,
  promotePaper,
} = useReferencesSearch();

// Track which cards are expanded
const expandedIds = ref<Set<string>>(new Set());

// Detail panel state
const selectedPaperId = ref<string | null>(null);
const selectedPaperData = ref<ReferencePaperQuery | null>(null);

onMounted(async () => {
  await Promise.all([search(), loadArticlesOfInterest()]);
});

function toggleExpand(paper: ReferencePaperQuery): void {
  if (expandedIds.value.has(paper.id)) {
    expandedIds.value.delete(paper.id);
  } else {
    expandedIds.value.add(paper.id);
    // Load linked articles on first expand
    if (!linkedArticlesMap.value[paper.id]) {
      loadLinkedArticles(paper.id);
    }
  }
}

function openDetail(paper: ReferencePaperQuery): void {
  selectedPaperId.value = paper.id;
  selectedPaperData.value = paper;
}

function closeDetail(): void {
  selectedPaperId.value = null;
  selectedPaperData.value = null;
}

async function handlePromote(paper: ReferencePaperQuery): Promise<void> {
  const articleId = await promotePaper(paper.id);
  if (articleId) {
    toast.show(`"${paper.title ?? 'Untitled'}" promoted to article`, 'success');
    emit('article-promoted', articleId);
    closeDetail();
  }
}

function handleExecuteSearch(): void {
  search(searchText.value);
}

function handleClearSearch(): void {
  searchText.value = '';
  search('');
}

function canPromote(paper: ReferencePaperQuery): boolean {
  if (paper.matchStatus === 'matched' || paper.matchStatus === 'imported') return false;
  return !!paper.abstractText?.trim();
}

function typeLabel(refType: string): string {
  return refType === 'citation' ? 'Cited by' : 'Ref';
}

function refTypeIcon(refType: string): string {
  return refType === 'citation' ? 'north_west' : 'south_east';
}
</script>

<template>
  <div class="references-view">
    <!-- Error Banner -->
    <div
      v-if="error"
      class="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg flex items-start gap-2"
    >
      <span class="material-symbols-outlined text-red-500 text-base mt-0.5">error</span>
      <div class="flex-1">
        <p class="text-sm text-red-700 font-medium">Error loading reference data</p>
        <p class="text-xs text-red-600 mt-0.5">{{ error }}</p>
      </div>
      <button
        class="material-symbols-outlined text-sm text-red-400 hover:text-red-600 cursor-pointer"
        @click="search()"
      >
        refresh
      </button>
    </div>

    <!-- Section A: Articles of Interest -->
    <section v-if="articlesOfInterest.length > 0" class="mb-8">
      <h2 class="text-sm font-semibold text-slate-700 uppercase tracking-wide mb-3">
        <span class="material-symbols-outlined text-amber-500 align-middle text-base mr-1"
          >trending_up</span
        >
        Articles of Interest
        <span class="text-xs font-normal text-slate-400 ml-1">(Top unmatched)</span>
      </h2>
      <div class="grid gap-2 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        <div
          v-for="paper in articlesOfInterest"
          :key="paper.id"
          class="bg-amber-50 border border-amber-200 rounded-lg p-3 hover:bg-amber-100 transition-colors cursor-pointer"
          @click="openDetail(paper)"
        >
          <div class="text-sm font-medium text-slate-800 line-clamp-2 mb-1">
            {{ paper.title || 'Untitled' }}
          </div>
          <div class="text-xs text-slate-500 mb-1">
            {{ formatAuthors(paper.authors) }}
            <span v-if="paper.publicationYear"> ({{ paper.publicationYear }})</span>
          </div>
          <div class="flex items-center gap-2 mt-2">
            <span class="text-[10px] px-1.5 py-0.5 bg-amber-200 text-amber-800 rounded-full">
              {{ paper.citationCount + paper.referenceCount }} uses
            </span>
            <a
              v-if="paper.doi"
              :href="doiLink(paper.doi)"
              target="_blank"
              rel="noopener"
              class="material-symbols-outlined text-xs text-blue-600 hover:text-blue-800"
              title="Open DOI"
              @click.stop
            >
              open_in_new
            </a>
            <button
              v-if="canPromote(paper)"
              class="ml-auto material-symbols-outlined text-xs text-green-600 hover:text-green-800 cursor-pointer"
              title="Add to Working list"
              @click.stop="handlePromote(paper)"
            >
              add_circle
            </button>
          </div>
        </div>
      </div>
    </section>

    <!-- Section B: Search & Reference Table -->
    <section>
      <!-- Search Bar -->
      <div class="flex items-center gap-2 mb-4">
        <div class="relative flex-1">
          <span
            class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 text-lg"
            >search</span
          >
          <input
            v-model="searchText"
            type="text"
            placeholder="Search title, author, abstract, journal..."
            class="w-full pl-9 pr-3 py-2 text-sm border border-slate-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:border-indigo-400"
            @keydown.enter="handleExecuteSearch"
          />
        </div>
        <select
          v-model="statusFilter"
          class="px-3 py-2 text-sm border border-slate-300 rounded-lg bg-white focus:outline-none focus:ring-2 focus:ring-indigo-400 cursor-pointer"
          @change="handleExecuteSearch"
        >
          <option value="all">All</option>
          <option value="unmatched">Unmatched</option>
          <option value="matched">Matched</option>
          <option value="imported">Imported</option>
        </select>
        <button
          class="px-3 py-2 text-sm bg-indigo-600 text-white rounded-lg hover:bg-indigo-700 transition-colors cursor-pointer"
          @click="handleExecuteSearch"
        >
          Search
        </button>
        <button
          class="px-3 py-2 text-sm border border-slate-300 rounded-lg hover:bg-slate-50 transition-colors cursor-pointer"
          @click="handleClearSearch"
        >
          Clear
        </button>
      </div>

      <!-- Loading -->
      <div v-if="loading" class="text-center py-12 text-slate-400 text-sm">Loading...</div>

      <!-- Empty state -->
      <div v-else-if="papers.length === 0" class="text-center py-12 text-slate-400 text-sm">
        <span class="material-symbols-outlined text-4xl mb-2 block">library_books</span>
        <p v-if="searchText">No reference papers found matching "{{ searchText }}"</p>
        <p v-else>No reference papers in the database yet.</p>
      </div>

      <!-- Paper List -->
      <ul v-else class="divide-y divide-slate-200 border border-slate-200 rounded-lg">
        <li v-for="paper in papers" :key="paper.id" class="hover:bg-slate-50 transition-colors">
          <!-- Card header (always visible) -->
          <div class="flex items-start gap-3 px-4 py-3">
            <!-- Type icon / click to open detail -->
            <div
              class="flex flex-col items-center gap-1 pt-0.5 cursor-pointer"
              @click="openDetail(paper)"
            >
              <span class="material-symbols-outlined text-sm text-indigo-500">article</span>
            </div>

            <!-- Main info -->
            <div class="flex-1 min-w-0 cursor-pointer" @click="openDetail(paper)">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium text-slate-800 line-clamp-1">
                  {{ paper.title || 'Untitled' }}
                </span>
                <a
                  v-if="paper.matchedArticleId"
                  class="material-symbols-outlined text-xs text-blue-600 hover:text-blue-800 shrink-0"
                  title="Open matched article"
                  @click.stop="emit('navigate-to-article', paper.matchedArticleId!)"
                >
                  link
                </a>
              </div>
              <div class="text-xs text-slate-500 mt-0.5">
                {{ formatAuthors(paper.authors) }}
                <span v-if="paper.publicationYear"> ({{ paper.publicationYear }})</span>
                <span v-if="paper.journal"> — {{ paper.journal }}</span>
              </div>
            </div>

            <!-- Badges & expand toggle -->
            <div class="flex items-center gap-2 shrink-0">
              <span
                class="text-[10px] px-1.5 py-0.5 rounded-full"
                :class="
                  paper.matchStatus === 'matched'
                    ? 'bg-green-100 text-green-700'
                    : paper.matchStatus === 'imported'
                      ? 'bg-blue-100 text-blue-700'
                      : 'bg-slate-100 text-slate-600'
                "
              >
                {{ paper.matchStatus }}
              </span>
              <span
                v-if="paper.citationCount + paper.referenceCount > 0"
                class="text-[10px] px-1.5 py-0.5 bg-indigo-50 text-indigo-700 rounded-full"
              >
                {{ paper.citationCount + paper.referenceCount }} uses
              </span>
              <button
                class="material-symbols-outlined text-base text-slate-400 transition-transform cursor-pointer"
                :class="{ 'rotate-180': expandedIds.has(paper.id) }"
                @click.stop="toggleExpand(paper)"
              >
                expand_more
              </button>
            </div>
          </div>

          <!-- Expanded detail (toggle open) -->
          <div v-if="expandedIds.has(paper.id)" class="px-4 pb-3 pt-0">
            <div
              class="ml-7 bg-slate-50 border border-slate-200 rounded-lg p-3 text-xs text-slate-600 space-y-1.5"
            >
              <!-- Meta-data row -->
              <div v-if="paper.abstractText" class="text-slate-700">
                <span class="font-medium">Abstract:</span>
                <span class="line-clamp-4">{{ paper.abstractText }}</span>
              </div>
              <div v-if="paper.journal">
                <span class="font-medium">Journal:</span> {{ paper.journal }}
              </div>
              <div class="flex flex-wrap gap-x-4 gap-y-1">
                <span v-if="paper.volume"
                  ><span class="font-medium">Vol:</span> {{ paper.volume }}</span
                >
                <span v-if="paper.issue"
                  ><span class="font-medium">Issue:</span> {{ paper.issue }}</span
                >
                <span v-if="paper.startPage"
                  ><span class="font-medium">Pages:</span> {{ paper.startPage
                  }}{{ paper.endPage ? `-${paper.endPage}` : '' }}</span
                >
                <span v-if="paper.publicationYear"
                  ><span class="font-medium">Year:</span> {{ paper.publicationYear }}</span
                >
              </div>
              <div v-if="paper.keywords && paper.keywords.length" class="flex flex-wrap gap-1">
                <span
                  v-for="kw in paper.keywords.slice(0, 8)"
                  :key="kw"
                  class="px-1.5 py-0.5 bg-slate-200 text-slate-600 rounded text-[10px]"
                >
                  {{ kw }}
                </span>
              </div>
              <div class="flex gap-4">
                <span v-if="paper.citationCount"
                  ><span class="font-medium">Cited by:</span> {{ paper.citationCount }}</span
                >
                <span v-if="paper.referenceCount"
                  ><span class="font-medium">References:</span> {{ paper.referenceCount }}</span
                >
              </div>
              <div v-if="paper.doi" class="flex items-center gap-1">
                <span class="font-medium">DOI:</span>
                <a
                  :href="doiLink(paper.doi)"
                  target="_blank"
                  rel="noopener"
                  class="text-blue-600 hover:underline"
                >
                  {{ paper.doi }}
                </a>
              </div>

              <!-- Linked Articles (Cited By sub-table) -->
              <div v-if="linkedArticlesLoading[paper.id]" class="text-slate-400 pt-1">
                Loading linked articles…
              </div>
              <div v-else-if="linkedArticlesMap[paper.id]?.length" class="pt-1">
                <div class="font-medium text-slate-700 mb-1">Cited By / Referenced In:</div>
                <ul class="space-y-1">
                  <li
                    v-for="linked in linkedArticlesMap[paper.id]"
                    :key="linked.id"
                    class="flex items-center gap-2 text-slate-600"
                  >
                    <span
                      class="material-symbols-outlined text-xs"
                      :class="
                        linked.referenceType === 'citation' ? 'text-amber-500' : 'text-indigo-500'
                      "
                      :title="linked.referenceType === 'citation' ? 'Citation' : 'Reference'"
                    >
                      {{ refTypeIcon(linked.referenceType) }}
                    </span>
                    <span class="line-clamp-1 flex-1">{{ linked.title }}</span>
                    <span class="text-[10px] text-slate-400">
                      {{ typeLabel(linked.referenceType) }}
                    </span>
                    <button
                      class="material-symbols-outlined text-xs text-blue-600 hover:text-blue-800 cursor-pointer"
                      title="Go to article"
                      @click="emit('navigate-to-article', linked.id)"
                    >
                      north_east
                    </button>
                  </li>
                </ul>
              </div>

              <!-- Promote button (only if abstract exists and unmatched) -->
              <div v-if="canPromote(paper)" class="pt-2 border-t border-slate-200">
                <button
                  class="px-3 py-1.5 text-xs bg-green-600 text-white rounded-lg hover:bg-green-700 transition-colors cursor-pointer"
                  @click="handlePromote(paper)"
                >
                  Add to Working list
                </button>
              </div>
            </div>
          </div>
        </li>
      </ul>

      <!-- Pagination -->
      <div v-if="total > 0" class="flex items-center justify-center gap-2 mt-4 pb-4">
        <button
          class="px-3 py-1.5 text-xs rounded-lg border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors cursor-pointer"
          :disabled="!canGoPrev"
          @click="goToPage(1)"
        >
          First
        </button>
        <button
          class="px-3 py-1.5 text-xs rounded-lg border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors cursor-pointer"
          :disabled="!canGoPrev"
          @click="goToPage(currentPage - 1)"
        >
          &laquo; Prev
        </button>
        <span class="text-xs text-slate-600 min-w-[6rem] text-center">
          Page {{ currentPage }} of {{ totalPages }}
          <span class="text-slate-400 ml-1">({{ total }} papers)</span>
        </span>
        <button
          class="px-3 py-1.5 text-xs rounded-lg border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors cursor-pointer"
          :disabled="!canGoNext"
          @click="goToPage(currentPage + 1)"
        >
          Next &raquo;
        </button>
        <button
          class="px-3 py-1.5 text-xs rounded-lg border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors cursor-pointer"
          :disabled="!canGoNext"
          @click="goToPage(totalPages)"
        >
          Last
        </button>
      </div>
    </section>

    <!-- Detail Panel Overlay -->
    <Teleport to="body">
      <div v-if="selectedPaperId" class="fixed inset-0 z-[60] bg-black/20" @click="closeDetail" />
      <ReferencePaperDetailPanel
        v-if="selectedPaperId"
        :paper-id="selectedPaperId"
        :initial-data="selectedPaperData ?? undefined"
        @close="closeDetail"
        @promoted="
          (id) => {
            emit('article-promoted', id);
            closeDetail();
            search();
            loadArticlesOfInterest();
          }
        "
        @navigate-to-article="
          (id) => {
            emit('navigate-to-article', id);
          }
        "
      />
    </Teleport>
  </div>
</template>

<style scoped>
.line-clamp-1 {
  display: -webkit-box;
  -webkit-line-clamp: 1;
  line-clamp: 1;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.line-clamp-2 {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.line-clamp-4 {
  display: -webkit-box;
  -webkit-line-clamp: 4;
  line-clamp: 4;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
</style>
