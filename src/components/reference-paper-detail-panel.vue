<script setup lang="ts">
import { ref, watch } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useToast } from '@/composables/use-toast';
import type { ReferencePaperQuery, LinkedArticleInfo } from '@/types';
import { formatAuthors, doiLink, getPublicationTypeLabel } from '@/utils/formatters';

const props = defineProps<{
  paperId: string;
  initialData?: ReferencePaperQuery;
}>();

const emit = defineEmits<{
  close: [];
  promoted: [articleId: string];
  'navigate-to-article': [articleId: string];
}>();

const toast = useToast();

const paper = ref<ReferencePaperQuery | null>(props.initialData ?? null);
const linkedArticles = ref<LinkedArticleInfo[]>([]);
const linkedLoading = ref(false);
const loading = ref(false);
const promoting = ref(false);
const error = ref<string | null>(null);

async function loadPaper(): Promise<void> {
  if (props.initialData) {
    paper.value = props.initialData;
    return;
  }
  loading.value = true;
  try {
    paper.value = await tauriCommand<ReferencePaperQuery>('get_reference_paper', {
      paperId: props.paperId,
    });
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function loadLinked(): Promise<void> {
  linkedLoading.value = true;
  try {
    linkedArticles.value = await tauriCommand<LinkedArticleInfo[]>(
      'get_linked_articles_for_paper',
      {
        paperId: props.paperId,
      }
    );
  } catch {
    linkedArticles.value = [];
  } finally {
    linkedLoading.value = false;
  }
}

async function handlePromote(): Promise<void> {
  if (!paper.value || promoting.value) return;
  promoting.value = true;
  try {
    const result = await tauriCommand<{
      articleId: string;
      articleTitle: string;
      wasLinked: boolean;
    }>('promote_reference_to_article', { referencePaperId: paper.value.id });
    const msg = result.wasLinked
      ? `"${result.articleTitle}" already in library — linked to existing article`
      : `"${result.articleTitle}" added to library`;
    toast.show(msg, 'success');
    if (paper.value) {
      paper.value.matchStatus = 'matched';
      paper.value.matchedArticleId = result.articleId;
    }
    emit('promoted', result.articleId);
  } catch (e: unknown) {
    toast.show(`Promote failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
  } finally {
    promoting.value = false;
  }
}

function typeLabel(refType: string): string {
  return refType === 'citation' ? 'Cited by' : 'Ref';
}

function refTypeIcon(refType: string): string {
  return refType === 'citation' ? 'north_west' : 'south_east';
}

const canPromote = () => {
  if (!paper.value) return false;
  if (paper.value.matchStatus === 'matched' || paper.value.matchStatus === 'imported') return false;
  return !!paper.value.abstractText?.trim();
};

watch(
  () => props.paperId,
  () => {
    error.value = null;
    void Promise.all([loadPaper(), loadLinked()]);
  },
  { immediate: true }
);
</script>

<template>
  <aside
    class="fixed inset-y-0 right-0 w-full sm:w-[480px] z-[70] bg-white shadow-2xl border-l border-slate-200 flex flex-col"
  >
    <!-- Header -->
    <div class="flex items-center gap-2 px-4 py-3 border-b border-slate-200 bg-slate-50">
      <button
        class="material-symbols-outlined text-xl text-slate-500 hover:text-slate-800 cursor-pointer"
        @click="emit('close')"
      >
        close
      </button>
      <h2 class="text-sm font-semibold text-slate-800 truncate flex-1">
        {{
          paper
            ? `${getPublicationTypeLabel(paper.referenceType)} Detail`
            : 'Reference Paper Detail'
        }}
      </h2>
      <span
        class="text-[10px] px-2 py-0.5 rounded-full font-medium"
        :class="
          paper?.matchStatus === 'matched'
            ? 'bg-green-100 text-green-700'
            : paper?.matchStatus === 'imported'
              ? 'bg-blue-100 text-blue-700'
              : 'bg-slate-100 text-slate-600'
        "
      >
        {{ paper?.matchStatus ?? '...' }}
      </span>
    </div>

    <!-- Body -->
    <div v-if="loading" class="flex-1 flex items-center justify-center text-slate-400 text-sm">
      Loading...
    </div>
    <div v-else-if="error" class="flex-1 flex items-center justify-center p-6">
      <div class="text-center">
        <span class="material-symbols-outlined text-3xl text-red-400 mb-2 block">error</span>
        <p class="text-sm text-red-600">{{ error }}</p>
      </div>
    </div>
    <div v-else-if="paper" class="flex-1 overflow-y-auto p-5 space-y-5">
      <!-- Title & Authors -->
      <div>
        <h3 class="text-base font-semibold text-slate-900 leading-snug mb-1">
          {{ paper.title || 'Untitled' }}
        </h3>
        <p class="text-sm text-slate-600">
          {{ formatAuthors(paper.authors) }}
          <span v-if="paper.publicationYear"> ({{ paper.publicationYear }})</span>
        </p>
      </div>

      <!-- Abstract -->
      <div v-if="paper.abstractText" class="bg-slate-50 rounded-lg p-3">
        <h4 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1">Abstract</h4>
        <p class="text-sm text-slate-700 leading-relaxed">{{ paper.abstractText }}</p>
      </div>
      <div v-else class="bg-amber-50 border border-amber-200 rounded-lg p-3 text-xs text-amber-700">
        <span class="material-symbols-outlined text-sm align-middle mr-1">info</span>
        No abstract available. Promotion to article requires an abstract.
      </div>

      <!-- Meta-data grid -->
      <div class="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
        <div v-if="paper.journal">
          <span class="text-slate-500 font-medium">Journal:</span>
          <span class="text-slate-800">{{ paper.journal }}</span>
        </div>
        <div v-if="paper.doi">
          <span class="text-slate-500 font-medium">DOI:</span>
          <a
            :href="doiLink(paper.doi)"
            target="_blank"
            rel="noopener"
            class="text-indigo-600 hover:underline"
          >
            {{ paper.doi }}
          </a>
        </div>
        <div v-if="paper.volume">
          <span class="text-slate-500 font-medium">Volume:</span> {{ paper.volume }}
        </div>
        <div v-if="paper.issue">
          <span class="text-slate-500 font-medium">Issue:</span> {{ paper.issue }}
        </div>
        <div v-if="paper.startPage">
          <span class="text-slate-500 font-medium">Pages:</span>
          {{ paper.startPage }}{{ paper.endPage ? `–${paper.endPage}` : '' }}
        </div>
        <div v-if="paper.publicationYear">
          <span class="text-slate-500 font-medium">Year:</span> {{ paper.publicationYear }}
        </div>
        <div v-if="paper.url">
          <span class="text-slate-500 font-medium">URL:</span>
          <a
            :href="paper.url"
            target="_blank"
            rel="noopener"
            class="text-indigo-600 hover:underline text-xs"
          >
            {{ paper.url }}
          </a>
        </div>
      </div>

      <!-- Keywords -->
      <div v-if="paper.keywords && paper.keywords.length > 0">
        <h4 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1">Keywords</h4>
        <div class="flex flex-wrap gap-1">
          <span
            v-for="kw in paper.keywords"
            :key="kw"
            class="px-2 py-0.5 bg-slate-100 text-slate-600 rounded text-xs"
          >
            {{ kw }}
          </span>
        </div>
      </div>

      <!-- Citation / Reference counts -->
      <div class="flex gap-6 text-sm">
        <span v-if="paper.citationCount">
          <span class="text-slate-500 font-medium">Cited by:</span> {{ paper.citationCount }}
        </span>
        <span v-if="paper.referenceCount">
          <span class="text-slate-500 font-medium">References:</span> {{ paper.referenceCount }}
        </span>
      </div>

      <!-- Matched article link -->
      <div
        v-if="paper.matchedArticleId"
        class="bg-green-50 border border-green-200 rounded-lg p-3 flex items-center gap-2"
      >
        <span class="material-symbols-outlined text-green-600 text-lg">link</span>
        <span class="text-sm text-green-800 font-medium flex-1">Matched to article in library</span>
        <button
          class="px-3 py-1 text-xs bg-green-600 text-white rounded-lg hover:bg-green-700 cursor-pointer"
          @click="emit('navigate-to-article', paper.matchedArticleId!)"
        >
          Open
        </button>
      </div>

      <!-- Linked Articles -->
      <div>
        <h4 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">
          Linked Articles
        </h4>
        <div v-if="linkedLoading" class="text-xs text-slate-400 italic">Loading…</div>
        <div v-else-if="linkedArticles.length === 0" class="text-xs text-slate-400 italic">
          Not linked to any articles in the library.
        </div>
        <ul v-else class="space-y-1">
          <li
            v-for="linked in linkedArticles"
            :key="linked.id"
            class="flex items-center gap-2 px-3 py-2 bg-slate-50 rounded-lg text-sm hover:bg-slate-100 transition-colors cursor-pointer"
            @click="emit('navigate-to-article', linked.id)"
          >
            <span
              class="material-symbols-outlined text-xs"
              :class="linked.referenceType === 'citation' ? 'text-amber-500' : 'text-indigo-500'"
              :title="linked.referenceType === 'citation' ? 'Citation' : 'Reference'"
            >
              {{ refTypeIcon(linked.referenceType) }}
            </span>
            <span class="flex-1 truncate text-slate-700">{{ linked.title }}</span>
            <span class="text-[10px] text-slate-400">{{ typeLabel(linked.referenceType) }}</span>
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
    </div>

    <!-- Footer: Promote button -->
    <div v-if="paper && canPromote()" class="p-4 border-t border-slate-200 bg-slate-50">
      <button
        class="w-full py-2.5 text-sm font-semibold bg-emerald-600 text-white rounded-lg hover:bg-emerald-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
        :disabled="promoting"
        @click="handlePromote"
      >
        <span
          v-if="promoting"
          class="material-symbols-outlined text-sm animate-spin align-middle mr-1"
          >progress_activity</span
        >
        <span v-else class="material-symbols-outlined text-sm align-middle mr-1">add_circle</span>
        {{ promoting ? 'Adding…' : 'Add to Working list' }}
      </button>
    </div>
  </aside>
</template>
