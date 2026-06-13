<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useRouter } from 'vue-router';
import { open as fileDialog } from '@tauri-apps/plugin-dialog';
import type { Article, ArticleReference } from '@/types';
import {
  useReferences,
  isAutoDownloading,
  autoDownloadReferences,
  useBatchReferenceScraping,
} from '@/composables/use-references';
import type { PreviewPaper } from '@/composables/use-references';
import { flattenRawReferences } from '@/utils/reference-flatten';
import { useToast } from '@/composables/use-toast';
import { useFeatureFlags } from '@/composables/use-feature-flags';

const props = defineProps<{
  article: Article;
}>();

const emit = defineEmits<{
  navigateToArticle: [id: string];
  articlePromoted: [articleId: string];
  referencesUpdated: [];
}>();

const toast = useToast();
const router = useRouter();

// References section
const { getArticleReferences, promoteReferenceToArticle } = useReferences();
const articleReferences = ref<ArticleReference[]>([]);
const expandedRefs = ref<Set<string>>(new Set());
const promotingRefId = ref<string | null>(null);
const refsLoading = ref(false);
const refTab = ref<'reference' | 'citation'>('reference');
const showRefImportDialog = ref(false);

async function loadReferences(): Promise<void> {
  if (!props.article.id) return;
  refsLoading.value = true;
  try {
    const raw = await getArticleReferences(props.article.id);
    articleReferences.value = flattenRawReferences(raw as unknown[]);
  } catch {
    articleReferences.value = [];
  } finally {
    refsLoading.value = false;
  }
}

// Import references from file - two-step flow
const { previewReferencesImport, importReferencesForArticle } = useReferences();
const refImportType = ref<'reference' | 'citation'>('reference');
const refImportBusy = ref(false);
const refImportStep = ref<'select' | 'preview'>('select');
const refImportPreview = ref<PreviewPaper[] | null>(null);
const refImportFilePath = ref<string | null>(null);

/** Step 1: Choose file and preview what would be imported */
async function handleChooseFile(): Promise<void> {
  if (refImportBusy.value) return;
  try {
    const selected = await fileDialog({
      multiple: false,
      filters: [
        { name: 'RIS Files', extensions: ['ris'] },
        { name: 'BibTeX Files', extensions: ['bib'] },
        { name: 'All Files', extensions: ['*'] },
      ],
    });
    if (!selected) return;
    refImportFilePath.value = selected;
    refImportBusy.value = true;
    const result = await previewReferencesImport(selected);
    if (result) {
      refImportPreview.value = result.papers;
      refImportStep.value = 'preview';
    }
  } catch (e) {
    console.error('[references] preview failed:', e);
  } finally {
    refImportBusy.value = false;
  }
}

/** Step 2: Confirm and actually import */
async function handleConfirmImport(): Promise<void> {
  if (refImportBusy.value || !refImportFilePath.value) return;
  refImportBusy.value = true;
  try {
    const result = await importReferencesForArticle(
      props.article.id,
      refImportFilePath.value,
      refImportType.value
    );
    if (result) {
      await loadReferences();
      emit('referencesUpdated');
      showRefImportDialog.value = false;
      refImportStep.value = 'select';
      refImportPreview.value = null;
      refImportFilePath.value = null;
    }
  } catch (e) {
    console.error('[references] import failed:', e);
  } finally {
    refImportBusy.value = false;
  }
}

function closeRefImportDialog(): void {
  showRefImportDialog.value = false;
  refImportStep.value = 'select';
  refImportPreview.value = null;
  refImportFilePath.value = null;
}

function openHelp(): void {
  closeRefImportDialog();
  router.push('/help?tab=reference#ref-references-citations').catch(() => {});
}

// Load references when article changes
watch(
  () => props.article.id,
  () => {
    articleReferences.value = [];
    refTab.value = 'reference';
    void loadReferences();
  },
  { immediate: true }
);

const refRefCount = computed(() => {
  const count = articleReferences.value.filter((r) => r.referenceType === 'reference').length;
  if (count > 0) return count;
  return props.article.numReferences ?? 0;
});
const citationRefCount = computed(() => {
  const count = articleReferences.value.filter((r) => r.referenceType === 'citation').length;
  if (count > 0) return count;
  return props.article.numCited ?? 0;
});
const activeRefs = computed(() =>
  articleReferences.value.filter((r) => r.referenceType === refTab.value)
);

function toggleRefExpand(id: string): void {
  const next = new Set(expandedRefs.value);
  if (next.has(id)) {
    next.delete(id);
  } else {
    next.add(id);
  }
  expandedRefs.value = next;
}

async function handlePromoteReference(paperId: string): Promise<void> {
  promotingRefId.value = paperId;
  try {
    const result = await promoteReferenceToArticle(paperId);
    if (result) {
      const ref = articleReferences.value.find((r) => r.id === paperId);
      if (ref) {
        ref.matchStatus = 'imported';
        ref.matchedArticleId = result.articleId;
      }
      const msg = result.wasLinked
        ? `"${result.articleTitle}" already in library — linked to existing article`
        : `"${result.articleTitle}" added to library`;
      toast.show(msg, 'success');
      emit('articlePromoted', result.articleId);
    }
  } catch (e) {
    toast.show(`Failed to promote: ${e instanceof Error ? e.message : String(e)}`, 'error');
  } finally {
    promotingRefId.value = null;
  }
}

function handleRefNavigate(item: ArticleReference): void {
  if (item.matchedArticleId) {
    emit('navigateToArticle', item.matchedArticleId);
  }
}

// ── Auto-download references (premium feature) ──
const { isPremium } = useFeatureFlags();
const { batchProgress } = useBatchReferenceScraping();

const isBatchRunning = computed(() => batchProgress.value.isRunning);

const canAutoDownload = computed(() => {
  if (!isPremium.value) return false;
  if (props.article.status !== 'included') return false;
  if (!props.article.doi) return false;
  if (isBatchRunning.value) return false;
  return !props.article.hasReferenceDetails || !props.article.hasCitationDetails;
});

const autoDownloadInProgress = computed(
  () => isAutoDownloading(props.article.id) || isBatchRunning.value
);

function handleAutoDownload(): void {
  if (!canAutoDownload.value || autoDownloadInProgress.value) return;
  autoDownloadReferences(
    props.article.id,
    props.article.doi!,
    props.article.title || '(untitled)',
    !props.article.hasReferenceDetails,
    !props.article.hasCitationDetails,
    async () => {
      await loadReferences();
      emit('referencesUpdated');
    },
    (success) => {
      if (success && showRefImportDialog.value) {
        closeRefImportDialog();
      }
    }
  );
}
</script>

<template>
  <section>
    <div class="flex border-b border-slate-200 mb-3">
      <button
        class="px-3 py-1.5 text-xs font-label-caps uppercase tracking-wider transition-colors cursor-pointer"
        :class="
          refTab === 'reference'
            ? 'text-indigo-700 border-b-2 border-indigo-600 font-semibold'
            : 'text-slate-400 hover:text-slate-600'
        "
        @click="refTab = 'reference'"
      >
        References Used
        <span class="text-[10px] ml-0.5">({{ refRefCount }})</span>
      </button>
      <button
        class="px-3 py-1.5 text-xs font-label-caps uppercase tracking-wider transition-colors cursor-pointer"
        :class="
          refTab === 'citation'
            ? 'text-indigo-700 border-b-2 border-indigo-600 font-semibold'
            : 'text-slate-400 hover:text-slate-600'
        "
        @click="refTab = 'citation'"
      >
        Cited By
        <span class="text-[10px] ml-0.5">({{ citationRefCount }})</span>
      </button>
      <button
        class="ml-auto text-xs text-indigo-500 hover:text-indigo-700 cursor-pointer font-semibold transition-colors"
        :disabled="refImportBusy"
        @click="showRefImportDialog = true"
      >
        <span class="material-symbols-outlined text-[14px] align-middle">upload_file</span>
        import
      </button>
    </div>

    <div v-if="refsLoading" class="text-xs text-slate-400 italic py-2">Loading…</div>
    <div v-else-if="activeRefs.length === 0" class="text-xs text-slate-400 italic py-2">
      <template v-if="refTab === 'reference' && article.numReferences && article.numReferences > 0">
        {{ article.numReferences }} referenced papers noted by import without details
      </template>
      <template v-else-if="refTab === 'citation' && article.numCited && article.numCited > 0">
        {{ article.numCited }} citations noted by import without details
      </template>
      <template v-else>
        No {{ refTab === 'reference' ? 'references' : 'citations' }} yet.
      </template>
    </div>
    <ul v-else class="space-y-0.5 text-body-sm max-h-72 overflow-y-auto">
      <li
        v-for="item in activeRefs"
        :key="item.id"
        class="group border-b border-slate-100 last:border-0"
      >
        <!-- Row header -->
        <div class="flex items-start gap-2 py-1.5">
          <div class="min-w-0 flex-1">
            <span
              class="text-slate-700 truncate block"
              :class="{
                'hover:text-indigo-600 cursor-pointer':
                  item.matchStatus === 'matched' || item.matchStatus === 'imported',
              }"
              @click="
                item.matchStatus === 'matched' || item.matchStatus === 'imported'
                  ? handleRefNavigate(item)
                  : toggleRefExpand(item.id)
              "
            >
              {{ item.title || '(untitled)' }}
            </span>
            <span class="text-[11px] text-slate-400">
              {{ item.authors.join(', ') }}
              <span v-if="item.publicationYear"> ({{ item.publicationYear }})</span>
            </span>
          </div>

          <!-- Action buttons -->
          <div
            class="shrink-0 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity"
          >
            <button
              class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer rounded"
              title="Toggle details"
              @click="toggleRefExpand(item.id)"
            >
              {{ expandedRefs.has(item.id) ? 'expand_less' : 'expand_more' }}
            </button>
            <button
              v-if="item.matchStatus === 'matched' || item.matchStatus === 'imported'"
              class="material-symbols-outlined text-[14px] text-emerald-500 hover:text-emerald-700 hover:bg-emerald-50 cursor-pointer rounded"
              title="Go to article"
              @click="handleRefNavigate(item)"
            >
              link
            </button>
            <button
              v-if="item.matchStatus === 'unmatched' || item.matchStatus === 'not_in_library'"
              class="material-symbols-outlined text-[14px] text-indigo-500 hover:text-indigo-700 hover:bg-indigo-50 cursor-pointer rounded"
              :title="promotingRefId === item.id ? 'Promoting…' : 'Add to library'"
              :disabled="promotingRefId === item.id"
              @click="handlePromoteReference(item.id)"
            >
              {{ promotingRefId === item.id ? 'progress_activity' : 'add_circle' }}
            </button>
          </div>
        </div>

        <!-- Expanded details -->
        <div
          v-if="expandedRefs.has(item.id)"
          class="pl-7 pb-2 text-[11px] text-slate-500 space-y-1"
        >
          <p v-if="item.abstractText" class="leading-relaxed line-clamp-3">
            {{ item.abstractText }}
          </p>
          <div class="flex flex-wrap gap-x-3 gap-y-0.5">
            <span v-if="item.journal"> <strong>Journal:</strong> {{ item.journal }} </span>
            <span v-if="item.doi">
              <strong>DOI:</strong>
              <a
                class="text-indigo-500 hover:underline"
                :href="'https://doi.org/' + item.doi"
                target="_blank"
                rel="noopener noreferrer"
              >
                {{ item.doi }}
              </a>
            </span>
            <span v-if="item.volume"><strong>Vol:</strong> {{ item.volume }}</span>
            <span v-if="item.issue"><strong>Issue:</strong> {{ item.issue }}</span>
            <span v-if="item.startPage">
              <strong>Pages:</strong> {{ item.startPage
              }}<template v-if="item.endPage">–{{ item.endPage }}</template>
            </span>
          </div>
          <div v-if="item.keywords && item.keywords.length > 0" class="flex flex-wrap gap-1">
            <span
              v-for="kw in item.keywords.slice(0, 5)"
              :key="kw"
              class="bg-slate-100 text-slate-600 px-1.5 py-0.5 rounded text-[10px]"
            >
              {{ kw }}
            </span>
          </div>
        </div>
      </li>
    </ul>

    <Teleport to="body">
      <div
        v-if="showRefImportDialog"
        class="fixed inset-0 z-[100] flex items-center justify-center bg-black/30"
        @click.self="closeRefImportDialog"
      >
        <div
          class="bg-white rounded-xl shadow-xl border border-slate-200 w-full max-w-md p-5 space-y-4"
        >
          <div class="flex items-center justify-between">
            <h3 class="text-sm font-semibold text-slate-800">Import References</h3>
            <div class="flex items-center gap-2">
              <a
                href="#"
                class="text-[11px] text-indigo-600 hover:text-indigo-800 font-medium transition-colors"
                @click.prevent="openHelp"
              >
                Help
              </a>
              <button
                class="material-symbols-outlined text-[18px] text-slate-400 hover:text-slate-600 cursor-pointer rounded transition-colors"
                title="Close"
                @click="closeRefImportDialog"
              >
                close
              </button>
            </div>
          </div>

          <!-- Step 1: Select type and file -->
          <template v-if="refImportStep === 'select'">
            <p class="text-xs text-slate-500">
              Import references or citations for
              <strong class="text-slate-700 truncate block">{{ article.title }}</strong>
            </p>
            <div class="flex gap-2 text-xs">
              <label
                class="flex items-center gap-1.5 cursor-pointer px-3 py-2 rounded-lg border transition-colors flex-1"
                :class="
                  refImportType === 'reference'
                    ? 'border-indigo-300 bg-indigo-50 text-indigo-700'
                    : 'border-slate-200 text-slate-500 hover:border-slate-300'
                "
              >
                <input
                  v-model="refImportType"
                  type="radio"
                  value="reference"
                  class="accent-indigo-600"
                />
                Backward (cited refs)
              </label>
              <label
                class="flex items-center gap-1.5 cursor-pointer px-3 py-2 rounded-lg border transition-colors flex-1"
                :class="
                  refImportType === 'citation'
                    ? 'border-indigo-300 bg-indigo-50 text-indigo-700'
                    : 'border-slate-200 text-slate-500 hover:border-slate-300'
                "
              >
                <input
                  v-model="refImportType"
                  type="radio"
                  value="citation"
                  class="accent-indigo-600"
                />
                Forward (cited by)
              </label>
            </div>
            <div class="flex gap-2 justify-end">
              <button
                class="text-xs text-slate-500 hover:text-slate-700 font-semibold cursor-pointer px-3 py-1.5 border border-slate-300 rounded-lg"
                @click="closeRefImportDialog"
              >
                Cancel
              </button>
              <button
                class="flex items-center gap-1.5 text-xs font-semibold text-white bg-indigo-600 hover:bg-indigo-700 px-3 py-1.5 rounded-lg cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="refImportBusy"
                @click="handleChooseFile"
              >
                <span
                  v-if="refImportBusy"
                  class="material-symbols-outlined text-[14px] animate-spin"
                  >progress_activity</span
                >
                <span v-else class="material-symbols-outlined text-[14px]">upload_file</span>
                {{ refImportBusy ? 'Parsing…' : 'Choose File (RIS / BibTeX)' }}
              </button>
            </div>

            <!-- Auto-download section (premium) -->
            <template v-if="canAutoDownload || autoDownloadInProgress">
              <div
                class="flex items-center gap-2 text-[10px] text-slate-300 uppercase tracking-wider"
              >
                <span class="flex-1 h-px bg-slate-200"></span>
                <span>or</span>
                <span class="flex-1 h-px bg-slate-200"></span>
              </div>
              <div
                v-if="autoDownloadInProgress"
                class="flex items-center gap-2 p-3 rounded-lg bg-amber-50 border border-amber-200"
              >
                <span class="material-symbols-outlined text-amber-600 text-[18px] animate-spin"
                  >progress_activity</span
                >
                <span class="text-xs text-amber-700 font-medium">Auto download in progress…</span>
              </div>
              <button
                v-else
                class="w-full flex items-center justify-center gap-1.5 text-xs font-semibold text-white bg-emerald-600 hover:bg-emerald-700 px-3 py-2 rounded-lg cursor-pointer transition-colors"
                @click="handleAutoDownload"
              >
                <span class="material-symbols-outlined text-[14px]">download</span>
                Auto download references
              </button>
            </template>
          </template>

          <!-- Step 2: Preview and confirm -->
          <template v-else-if="refImportStep === 'preview'">
            <div
              class="flex items-center gap-2 p-3 rounded-lg bg-emerald-50 border border-emerald-200"
            >
              <span class="material-symbols-outlined text-emerald-600 text-[20px]"
                >check_circle</span
              >
              <div>
                <span class="text-sm font-semibold text-emerald-800"
                  >Found {{ refImportPreview?.length ?? 0 }} references</span
                >
                <p class="text-[11px] text-emerald-600">
                  Review the list below, then click Add to import.
                </p>
              </div>
            </div>

            <ul
              v-if="refImportPreview && refImportPreview.length > 0"
              class="max-h-48 overflow-y-auto space-y-1 border border-slate-200 rounded-lg p-2"
            >
              <li
                v-for="(paper, idx) in refImportPreview"
                :key="idx"
                class="text-xs py-1 border-b border-slate-100 last:border-0"
              >
                <span class="text-slate-700 font-medium">{{ paper.title || '(untitled)' }}</span>
                <span class="text-slate-400 block">
                  {{ paper.authors.join(', ') }}
                  <span v-if="paper.publicationYear"> ({{ paper.publicationYear }})</span>
                  <span v-if="paper.journal"> — {{ paper.journal }}</span>
                </span>
              </li>
            </ul>
            <p v-else class="text-xs text-slate-400 italic">
              No references found in the selected file.
            </p>

            <div class="flex gap-2 justify-end">
              <button
                class="text-xs text-slate-500 hover:text-slate-700 font-semibold cursor-pointer px-3 py-1.5 border border-slate-300 rounded-lg"
                @click="closeRefImportDialog"
              >
                Cancel
              </button>
              <button
                class="flex items-center gap-1.5 text-xs font-semibold text-white bg-emerald-600 hover:bg-emerald-700 px-4 py-1.5 rounded-lg cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="refImportBusy || !refImportPreview?.length"
                @click="handleConfirmImport"
              >
                <span
                  v-if="refImportBusy"
                  class="material-symbols-outlined text-[14px] animate-spin"
                  >progress_activity</span
                >
                <span v-else class="material-symbols-outlined text-[14px]">add_circle</span>
                {{ refImportBusy ? 'Importing…' : 'Add' }}
              </button>
            </div>
          </template>
        </div>
      </div>
    </Teleport>
  </section>
</template>
