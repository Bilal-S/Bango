<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { openPath } from '@tauri-apps/plugin-opener';
import type { Article, AuditEntry } from '@/types';
import AuditTimeline from './audit-timeline.vue';
import SuggestInput from './suggest-input.vue';
import TagChip from './tag-chip.vue';
import LabelChip from './label-chip.vue';
import CriteriaEditDialog from './criteria-edit-dialog.vue';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import { useCriteriaStore } from '@/stores/criteria';

const props = defineProps<{
  article: Article;
  auditTrail: AuditEntry[];
  hasPrevious: boolean;
  hasNext: boolean;
  hasReturnTarget: boolean;
  fullScreen?: boolean;
  articlePosition: number;
  articleTotal: number;
  decisionMessage?: string;
  decisionType?: 'success' | 'info';
}>();

const emit = defineEmits<{
  close: [];
  navigatePrev: [];
  navigateNext: [];
  moveArticle: [id: string, newStatus: string];
  updateNotes: [id: string, notes: string];
  updateTags: [id: string, tagIds: string[]];
  updateLabels: [id: string, labelIds: string[]];
  updateCriteria: [id: string, inclusionIds: string[], exclusionIds: string[]];
  navigateToArticle: [id: string];
  toggleFullScreen: [];
  attachFullText: [id: string];
  deleteFullText: [id: string];
  readFullText: [id: string];
}>();

/** Status badge config for the header display */
const statusDisplay = computed(() => {
  const status = props.article.status;
  if (status === 'duplicate') {
    return {
      label: 'DUPLICATE',
      bg: 'bg-amber-100',
      text: 'text-amber-800',
      border: 'border-amber-300',
    };
  }
  if (status === 'included') {
    return {
      label: 'INCLUDED',
      bg: 'bg-emerald-100',
      text: 'text-emerald-800',
      border: 'border-emerald-300',
    };
  }
  if (status === 'rejected') {
    return {
      label: 'REJECTED',
      bg: 'bg-rose-100',
      text: 'text-rose-800',
      border: 'border-rose-300',
    };
  }
  return { label: 'WORKING', bg: 'bg-blue-100', text: 'text-blue-800', border: 'border-blue-300' };
});

const tagsStore = useTagsStore();
const labelsStore = useLabelsStore();
const criteriaStore = useCriteriaStore();

// Ensure criteria are loaded so we can resolve UUID → text
void criteriaStore.fetchIfNeeded();

// Metadata expand/collapse state (persisted)
const metadataExpanded = ref(localStorage.getItem('bango-metadata-expanded') !== 'false');
function toggleMetadata(): void {
  metadataExpanded.value = !metadataExpanded.value;
  localStorage.setItem('bango-metadata-expanded', String(metadataExpanded.value));
}

// Audit trail expand/collapse state
const auditExpanded = ref(false);

// Panel resizing logic
const panelWidth = ref(parseInt(localStorage.getItem('bango-detail-panel-width') || '480'));
const isResizing = ref(false);

function startResize(e: MouseEvent): void {
  e.preventDefault();
  isResizing.value = true;
  const startX = e.clientX;
  const startWidth = panelWidth.value;

  function doResize(moveEvent: MouseEvent): void {
    const delta = startX - moveEvent.clientX;
    // Limit width between 320px and 900px
    const newWidth = Math.max(320, Math.min(900, startWidth + delta));
    panelWidth.value = newWidth;
    localStorage.setItem('bango-detail-panel-width', newWidth.toString());
  }

  function stopResize(): void {
    isResizing.value = false;
    window.removeEventListener('mousemove', doResize);
    window.removeEventListener('mouseup', stopResize);
    document.body.style.cursor = '';
  }

  window.addEventListener('mousemove', doResize);
  window.addEventListener('mouseup', stopResize);
  document.body.style.cursor = 'col-resize';
}

// Notes editing
const editingNotes = ref(false);
const noteDraft = ref('');

watch(editingNotes, (val) => {
  if (val) noteDraft.value = props.article.userNotes ?? '';
});

function saveNotes(): void {
  emit('updateNotes', props.article.id, noteDraft.value);
  editingNotes.value = false;
}

function cancelNotes(): void {
  editingNotes.value = false;
}

// Tag/Label add inputs
const newTag = ref('');
const newLabel = ref('');

// Alphabetically sorted tags and labels (case-insensitive)
const sortedTags = computed(() =>
  [...props.article.tags].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
);
const sortedLabels = computed(() =>
  [...props.article.labels].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
);

// Suggestions from global stores, excluding already-assigned values
const tagSuggestions = computed(() => {
  const assigned = new Set(props.article.tags.map((t) => t.toLowerCase()));
  return tagsStore.tags
    .map((t) => t.name)
    .filter((name) => !assigned.has(name.toLowerCase()))
    .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
});
const labelSuggestions = computed(() => {
  const assigned = new Set(props.article.labels.map((l) => l.toLowerCase()));
  return labelsStore.labels
    .map((l) => l.name)
    .filter((name) => !assigned.has(name.toLowerCase()))
    .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
});

/** Look up the color for a tag name from the global store */
function tagColor(name: string): string | null {
  return tagsStore.tags.find((t) => t.name === name)?.color ?? null;
}

/** Look up the color for a label name from the global store */
function labelColor(name: string): string | null {
  return labelsStore.labels.find((l) => l.name === name)?.color ?? null;
}

function removeTag(tag: string): void {
  const updated = props.article.tags.filter((t) => t !== tag);
  emit('updateTags', props.article.id, updated);
}

async function addTag(val: string): Promise<void> {
  if (!val || props.article.tags.includes(val)) return;
  emit('updateTags', props.article.id, [...props.article.tags, val]);
  newTag.value = '';
  // If the tag doesn't exist in the global store, create it
  const existsInStore = tagsStore.tags.some((t) => t.name.toLowerCase() === val.toLowerCase());
  if (!existsInStore) {
    await tagsStore.createTag(val);
    await tagsStore.fetchIfNeeded();
  }
}

function removeLabel(label: string): void {
  const updated = props.article.labels.filter((l) => l !== label);
  emit('updateLabels', props.article.id, updated);
}

async function addLabel(val: string): Promise<void> {
  if (!val || props.article.labels.includes(val)) return;
  emit('updateLabels', props.article.id, [...props.article.labels, val]);
  newLabel.value = '';
  // If the label doesn't exist in the global store, create it
  const existsInStore = labelsStore.labels.some((l) => l.name.toLowerCase() === val.toLowerCase());
  if (!existsInStore) {
    await labelsStore.createLabel(val);
    await labelsStore.fetchIfNeeded();
  }
}

const confidencePercentage = computed(() =>
  props.article.aiConfidence !== null ? `${Math.round(props.article.aiConfidence * 100)}%` : '---'
);

const aiDecisionLabel = computed(() => {
  if (!props.article.aiDecision) return null;
  return props.article.aiDecision === 'include' ? 'Included' : 'Excluded';
});

const aiDecisionColors = computed(() => {
  if (!props.article.aiDecision) return null;
  if (props.article.aiDecision === 'include') {
    return {
      bg: 'bg-emerald-50',
      border: 'border-emerald-200',
      icon: 'text-emerald-600',
      label: 'text-emerald-900',
      text: 'text-emerald-800',
    };
  }
  return {
    bg: 'bg-rose-50',
    border: 'border-rose-200',
    icon: 'text-rose-600',
    label: 'text-rose-900',
    text: 'text-rose-800',
  };
});

const confidenceBarWidth = computed(() =>
  props.article.aiConfidence !== null ? `${Math.round(props.article.aiConfidence * 100)}%` : '0%'
);

/** Compute global criterion index: inclusion [1]..[N], exclusion [N+1]..[N+M] */
const criterionIndexMap = computed(() => {
  const map = new Map<string, number>();
  let n = 1;
  for (const c of criteriaStore.inclusionCriteria) {
    map.set(c.id, n++);
  }
  for (const c of criteriaStore.exclusionCriteria) {
    map.set(c.id, n++);
  }
  return map;
});

/** Resolve a criterion UUID to its human-readable text */
const criteriaTextMap = computed(() => {
  const map = new Map<string, string>();
  for (const c of criteriaStore.criteria) {
    map.set(c.id, c.text);
  }
  return map;
});

function criterionText(id: string): string {
  return criteriaTextMap.value.get(id) ?? id;
}

/**
 * Replace criterion UUIDs in reasoning text with global numbered references `[n]`.
 * Done dynamically at display time so numbering stays correct when criteria are added/removed.
 * Also collapses double brackets `[[n]]` → `[n]` from LLM echoing prompt format.
 */
const displayReasoning = computed(() => {
  const raw = props.article.aiReasoning;
  if (!raw) return '';
  let result = raw;

  // Replace each known criterion UUID with its current global [n]
  const map = criterionIndexMap.value;
  for (const [uuid, n] of map) {
    if (result.includes(uuid)) {
      result = result.replaceAll(uuid, `[${n}]`);
    }
  }

  // Collapse double brackets: [[n]] → [n]
  let prev = '';
  while (prev !== result) {
    prev = result;
    result = result.replaceAll('[[', '[').replaceAll(']]', ']');
  }

  return result;
});

// Full-text reading view state
const showFullTextView = ref(false);
const fullTextContent = ref<string | null>(null);
const fullTextExpanded = ref(false);
const pdfSrc = ref<string | null>(null);

/** Whether the attached file is a PDF */
const isPdfAttachment = computed(() => {
  const name = props.article.fullTextFileName;
  return !!name && name.toLowerCase().endsWith('.pdf');
});

/** Determine the file type icon based on filename */
const fullTextFileIcon = computed(() => {
  const name = props.article.fullTextFileName;
  if (!name) return null;
  const lower = name.toLowerCase();
  if (lower.endsWith('.pdf')) return 'picture_as_pdf';
  if (lower.endsWith('.txt')) return 'description';
  return 'draft';
});

/** Absolute file path for the attachment (fetched on demand) */
const absoluteFilePath = ref<string | null>(null);

/** Open the full-text reading view */
async function openFullTextView(): Promise<void> {
  showFullTextView.value = true;
  fullTextContent.value = props.article.fullText;
  pdfSrc.value = null;
  absoluteFilePath.value = null;

  if (isPdfAttachment.value) {
    // Read file bytes via Tauri command and create a Blob URL for the iframe
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    try {
      const bytes = await tauriCommand<ArrayBuffer | null>('read_full_text_file_bytes', {
        articleId: props.article.id,
      });
      if (bytes) {
        const blob = new Blob([new Uint8Array(bytes as unknown as ArrayLike<number>)], {
          type: 'application/pdf',
        });
        pdfSrc.value = URL.createObjectURL(blob);
      }
    } catch (e) {
      console.warn('Failed to load PDF bytes for inline viewing:', e);
      // Fallback: extracted text will be shown instead
    }
    // Also fetch the path for "Open externally"
    const filePath = await tauriCommand<string | null>('get_full_text_file_path', {
      articleId: props.article.id,
    });
    if (filePath) {
      absoluteFilePath.value = filePath;
    }
  }
}

/** Open the attached file in the platform's default viewer */
async function openFileExternally(): Promise<void> {
  if (!absoluteFilePath.value) {
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    const filePath = await tauriCommand<string | null>('get_full_text_file_path', {
      articleId: props.article.id,
    });
    if (filePath) {
      absoluteFilePath.value = filePath;
    }
  }
  if (absoluteFilePath.value) {
    await openPath(absoluteFilePath.value);
  }
}

/** Revoke the Blob URL to free memory */
function revokePdfSrc(): void {
  if (pdfSrc.value && pdfSrc.value.startsWith('blob:')) {
    URL.revokeObjectURL(pdfSrc.value);
  }
}

/** Close the full-text reading view */
function closeFullTextView(): void {
  revokePdfSrc();
  pdfSrc.value = null;
  showFullTextView.value = false;
  fullTextExpanded.value = false;
}

/** Toggle full-text expand (use full width by toggling panel fullscreen) */
function toggleFullTextExpand(): void {
  fullTextExpanded.value = !fullTextExpanded.value;
  // Only toggle fullscreen if not already fullscreen to avoid double-toggle
  if (fullTextExpanded.value && !props.fullScreen) {
    emit('toggleFullScreen');
  }
}

/** Delete the full-text attachment */
function handleDeleteFullText(): void {
  emit('deleteFullText', props.article.id);
  showFullTextView.value = false;
  fullTextExpanded.value = false;
}

// Reset full text view when article changes
watch(
  () => props.article.id,
  () => {
    showFullTextView.value = false;
    fullTextExpanded.value = false;
    fullTextContent.value = null;
    pdfSrc.value = null;
    absoluteFilePath.value = null;
  }
);

// Criteria edit dialog
const showCriteriaDialog = ref(false);

function truncate(text: string, max = 20): string {
  return text.length > max ? text.slice(0, max) + '…' : text;
}

function handleCriteriaSave(
  _articleId: string,
  inclusionIds: string[],
  exclusionIds: string[]
): void {
  emit('updateCriteria', props.article.id, inclusionIds, exclusionIds);
}
</script>

<template>
  <aside
    class="detail-panel h-full bg-white flex flex-col z-50 relative"
    :class="{
      'transition-none': isResizing,
      'detail-panel--fullscreen': fullScreen,
      'shadow-[0_4px_24px_rgba(0,0,0,0.15)] border-l border-slate-200': !fullScreen,
    }"
    :style="fullScreen ? {} : { '--detail-panel-width': panelWidth + 'px' }"
  >
    <!-- Resize Handle (desktop only, hidden in fullscreen) -->
    <div
      v-if="!fullScreen"
      class="resizer hidden lg:block absolute left-0 top-0 bottom-0 w-1.5 cursor-col-resize z-50 hover:bg-indigo-400/50 active:bg-indigo-600 transition-colors"
      @mousedown="startResize"
    />
    <!-- Header -->
    <div class="p-6 border-b border-slate-100 sticky top-0 bg-white z-10">
      <div class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-2">
          <span
            class="text-xs font-label-caps text-primary uppercase bg-primary/5 px-2 py-0.5 rounded"
          >
            Current Selection
          </span>
          <span
            class="text-[10px] font-bold uppercase tracking-wider px-2 py-0.5 rounded border"
            :class="[statusDisplay.bg, statusDisplay.text, statusDisplay.border]"
          >
            {{ statusDisplay.label }}
          </span>
          <!-- Full-text attachment icon -->
          <button
            v-if="article.hasFullText && fullTextFileIcon"
            class="material-symbols-outlined text-[18px] cursor-pointer rounded px-1 transition-colors"
            :class="
              article.fullTextFileName?.toLowerCase().endsWith('.pdf')
                ? 'text-red-500 hover:bg-red-50'
                : 'text-blue-500 hover:bg-blue-50'
            "
            :title="'Open full text: ' + (article.fullTextFileName ?? '')"
            @click="openFullTextView"
          >
            {{ fullTextFileIcon }}
          </button>
          <button
            v-else
            class="material-symbols-outlined text-[18px] text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer rounded px-1 transition-colors"
            title="Attach full text (PDF or TXT)"
            @click="emit('attachFullText', article.id)"
          >
            attach_file
          </button>
        </div>
        <div class="flex items-center gap-1">
          <button
            class="material-symbols-outlined text-slate-400 hover:text-slate-900 transition-colors cursor-pointer"
            title="Toggle full screen"
            @click="emit('toggleFullScreen')"
          >
            {{ fullScreen ? 'close_fullscreen' : 'open_in_full' }}
          </button>
          <button
            class="material-symbols-outlined text-slate-400 hover:text-slate-900 transition-colors cursor-pointer"
            :title="hasReturnTarget ? 'Return to previous article' : 'Close detail panel'"
            @click="emit('close')"
          >
            {{ hasReturnTarget ? 'arrow_back' : 'close' }}
          </button>
        </div>
      </div>
      <h2 class="font-h1 text-h1 text-on-surface leading-tight mb-4">
        {{ article.title }}
      </h2>
      <!-- Collapsible Metadata -->
      <div class="border border-slate-200 rounded-lg overflow-hidden">
        <button
          class="w-full flex items-center justify-between px-3 py-2 text-xs font-label-caps text-slate-500 uppercase tracking-wider hover:bg-slate-50 cursor-pointer transition-colors"
          @click="toggleMetadata"
        >
          <span>Metadata</span>
          <span
            class="material-symbols-outlined text-[16px] transition-transform duration-200"
            :class="{ 'rotate-180': metadataExpanded }"
          >
            expand_more
          </span>
        </button>
        <div v-show="metadataExpanded" class="px-3 pb-3 space-y-3">
          <div
            v-if="article.authors.length > 0"
            class="flex flex-col gap-1 text-body-sm font-body-sm"
          >
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >Authors</span
            >
            <span class="text-on-surface">{{ article.authors.join(', ') }}</span>
          </div>
          <div class="grid grid-cols-2 gap-4 text-body-sm font-body-sm">
            <div class="flex flex-col gap-1">
              <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
                >Journal</span
              >
              <span class="text-on-surface truncate">{{ article.journal ?? '---' }}</span>
            </div>
            <div class="flex flex-col gap-1">
              <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
                >Year</span
              >
              <span class="text-on-surface">{{ article.publicationYear ?? '---' }}</span>
            </div>
            <div v-if="article.doi" class="flex flex-col gap-1 col-span-2">
              <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
                >DOI</span
              >
              <a
                class="text-primary hover:underline flex items-center gap-1"
                :href="'https://doi.org/' + article.doi"
                target="_blank"
                rel="noopener noreferrer"
              >
                {{ article.doi }}
                <span class="material-symbols-outlined text-[14px]">open_in_new</span>
              </a>
            </div>
            <div v-if="article.keywords.length > 0" class="flex flex-col gap-1 col-span-2">
              <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
                >Keywords</span
              >
              <span class="text-on-surface">{{ article.keywords.join(', ') }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Scrollable Content -->
    <div class="flex-1 overflow-y-auto p-6 space-y-8">
      <!-- AI Decision Card -->
      <section v-if="aiDecisionLabel && aiDecisionColors">
        <div class="rounded-xl p-4 border" :class="[aiDecisionColors.bg, aiDecisionColors.border]">
          <div class="flex items-center justify-between mb-2">
            <div class="flex items-center gap-2">
              <span class="material-symbols-outlined" :class="aiDecisionColors.icon">
                {{ article.aiDecision === 'include' ? 'verified' : 'cancel' }}
              </span>
              <span class="font-bold" :class="aiDecisionColors.label">
                {{ aiDecisionLabel }}
              </span>
            </div>
            <span
              class="text-[11px] font-bold bg-white px-2 py-0.5 rounded-full shadow-sm"
              :class="aiDecisionColors.label"
            >
              {{ confidencePercentage }} Confidence
            </span>
          </div>
          <!-- Confidence bar -->
          <div class="w-full bg-white/50 h-2 rounded-full overflow-hidden mb-3">
            <div
              class="h-full rounded-full transition-all duration-500"
              :class="article.aiDecision === 'include' ? 'bg-emerald-500' : 'bg-rose-400'"
              :style="{ width: confidenceBarWidth }"
            />
          </div>
          <p
            v-if="article.aiReasoning"
            class="text-body-sm leading-relaxed"
            :class="aiDecisionColors.text"
          >
            <span class="font-semibold">Reasoning:</span> {{ displayReasoning }}
          </p>
        </div>
      </section>

      <!-- Matched Criteria -->
      <section>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-xs font-label-caps text-slate-500 uppercase tracking-wider">
            Matched Criteria
          </h3>
          <button
            class="material-symbols-outlined text-[16px] text-slate-400 hover:text-indigo-600 cursor-pointer"
            title="Edit matched criteria"
            @click="showCriteriaDialog = true"
          >
            edit
          </button>
        </div>
        <template
          v-if="
            article.matchedInclusionCriteria.length > 0 ||
            article.matchedExclusionCriteria.length > 0
          "
        >
          <div class="grid grid-cols-2 gap-x-3 gap-y-1.5">
            <div
              v-for="criterion in article.matchedInclusionCriteria"
              :key="'inc-' + criterion"
              class="flex items-center gap-1.5 text-body-sm"
              :title="criterionText(criterion)"
            >
              <span
                class="text-[12px] font-bold text-emerald-600 bg-emerald-50 rounded px-1 leading-tight"
                >{{ criterionIndexMap.get(criterion) ?? '-' }}</span
              >
              <span class="truncate">{{ truncate(criterionText(criterion)) }}</span>
            </div>
            <div
              v-for="criterion in article.matchedExclusionCriteria"
              :key="'exc-' + criterion"
              class="flex items-center gap-1.5 text-body-sm text-slate-400"
              :title="criterionText(criterion)"
            >
              <span
                class="text-[12px] font-bold text-rose-500 bg-rose-50 rounded px-1 leading-tight"
                >{{ criterionIndexMap.get(criterion) ?? '-' }}</span
              >
              <span class="truncate line-through">{{ truncate(criterionText(criterion)) }}</span>
            </div>
          </div>
        </template>
        <p v-else class="text-xs text-slate-400 italic">
          No criteria matched. Click edit to assign.
        </p>
      </section>

      <!-- Criteria Edit Dialog -->
      <CriteriaEditDialog
        v-model="showCriteriaDialog"
        :article-id="article.id"
        :matched-inclusion-ids="article.matchedInclusionCriteria"
        :matched-exclusion-ids="article.matchedExclusionCriteria"
        :inclusion-criteria="criteriaStore.inclusionCriteria"
        :exclusion-criteria="criteriaStore.exclusionCriteria"
        @save="handleCriteriaSave"
      />

      <!-- Abstract -->
      <section v-if="article.abstractText">
        <h3 class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">
          Abstract
        </h3>
        <p class="text-body-main font-body-main text-on-surface-variant leading-relaxed">
          {{ article.abstractText }}
        </p>
      </section>

      <!-- Tags -->
      <section>
        <h3 class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">Tags</h3>
        <div class="flex flex-wrap gap-2 mb-2">
          <span
            v-for="tag in sortedTags"
            :key="'tag-' + tag"
            class="inline-flex items-center gap-1 group"
          >
            <TagChip :name="tag" :color="tagColor(tag)" />
            <button
              class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer rounded-full hover:bg-slate-100 leading-none opacity-0 group-hover:opacity-100 transition-opacity"
              @click="removeTag(tag)"
            >
              close
            </button>
          </span>
        </div>
        <div class="flex gap-2">
          <SuggestInput
            v-model="newTag"
            :suggestions="tagSuggestions"
            placeholder="Add tag…"
            class="flex-1"
            @select="addTag"
            @enter="addTag"
          />
        </div>
      </section>

      <!-- Labels -->
      <section>
        <h3 class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">Labels</h3>
        <div class="flex flex-wrap gap-2 mb-2">
          <span
            v-for="label in sortedLabels"
            :key="'label-' + label"
            class="inline-flex items-center gap-1 group"
          >
            <LabelChip :name="label" :color="labelColor(label)" />
            <button
              class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer rounded-full hover:bg-slate-100 leading-none opacity-0 group-hover:opacity-100 transition-opacity"
              @click="removeLabel(label)"
            >
              close
            </button>
          </span>
        </div>
        <div class="flex gap-2">
          <SuggestInput
            v-model="newLabel"
            :suggestions="labelSuggestions"
            placeholder="Add label…"
            class="flex-1"
            @select="addLabel"
            @enter="addLabel"
          />
        </div>
      </section>

      <!-- Imported Notes (read-only, shown only when present) -->
      <section v-if="article.notes">
        <h3 class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">
          Imported Notes
        </h3>
        <p
          class="text-body-main font-body-main text-on-surface-variant leading-relaxed bg-amber-50 border border-amber-200 p-3 rounded-lg"
        >
          {{ article.notes }}
        </p>
      </section>

      <!-- User Notes -->
      <section>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-xs font-label-caps text-slate-500 uppercase tracking-wider">Notes</h3>
          <button
            v-if="!editingNotes"
            class="material-symbols-outlined text-[16px] text-slate-400 hover:text-indigo-600 cursor-pointer"
            @click="editingNotes = true"
          >
            edit
          </button>
        </div>
        <div v-if="editingNotes" class="space-y-2">
          <textarea
            v-model="noteDraft"
            class="w-full text-sm border border-slate-200 rounded-lg p-3 focus:outline-none focus:ring-1 focus:ring-indigo-400 resize-y min-h-[80px]"
            placeholder="Add notes about this article…"
          />
          <div class="flex gap-2 justify-end">
            <button
              class="text-xs text-slate-500 hover:text-slate-700 font-semibold cursor-pointer px-3 py-1 border border-slate-300 rounded-lg"
              @click="cancelNotes"
            >
              Cancel
            </button>
            <button
              class="text-xs bg-indigo-600 text-white px-3 py-1 rounded-lg font-semibold hover:bg-indigo-700 cursor-pointer"
              @click="saveNotes"
            >
              Save
            </button>
          </div>
        </div>
        <p
          v-else-if="article.userNotes"
          class="text-body-main font-body-main text-on-surface-variant leading-relaxed bg-slate-50 p-3 rounded-lg"
        >
          {{ article.userNotes }}
        </p>
        <p v-else class="text-xs text-slate-400 italic">No notes yet. Click edit to add.</p>
      </section>

      <!-- Audit Trail -->
      <section>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-xs font-label-caps text-slate-500 uppercase tracking-wider">
            Audit Trail
          </h3>
          <button
            class="material-symbols-outlined text-[18px] text-slate-400 hover:text-slate-700 cursor-pointer transition-colors"
            @click="auditExpanded = !auditExpanded"
          >
            {{ auditExpanded ? 'expand_less' : 'expand_more' }}
          </button>
        </div>
        <template v-if="auditExpanded">
          <AuditTimeline
            :entries="auditTrail"
            :show-header="false"
            @navigate-to-article="emit('navigateToArticle', $event)"
          />
        </template>
      </section>

      <div class="pb-10" />
    </div>

    <!-- Inline Decision Notification -->
    <Transition name="decision-toast">
      <div
        v-if="decisionMessage"
        class="px-4 py-2 text-center text-sm font-semibold text-white"
        :class="decisionType === 'info' ? 'bg-blue-500' : 'bg-emerald-500'"
      >
        {{ decisionMessage }}
      </div>
    </Transition>

    <!-- Full-Text Reading View Overlay -->
    <div
      v-if="showFullTextView"
      class="absolute inset-0 z-50 bg-white flex flex-col"
      :class="fullTextExpanded ? '' : ''"
    >
      <!-- Full-text header bar -->
      <div
        class="flex items-center justify-between px-4 py-3 border-b border-slate-200 bg-slate-50 shrink-0"
      >
        <div class="flex items-center gap-2">
          <span
            class="material-symbols-outlined text-[18px]"
            :class="
              article.fullTextFileName?.toLowerCase().endsWith('.pdf')
                ? 'text-red-500'
                : 'text-blue-500'
            "
          >
            {{ fullTextFileIcon ?? 'description' }}
          </span>
          <span class="text-sm font-semibold text-slate-700 truncate max-w-[200px]">
            {{ article.fullTextFileName ?? 'Full Text' }}
          </span>
        </div>
        <div class="flex items-center gap-1">
          <button
            class="material-symbols-outlined text-[18px] text-slate-400 hover:text-slate-900 cursor-pointer rounded px-1 transition-colors"
            :title="fullTextExpanded ? 'Collapse' : 'Expand to full width'"
            @click="toggleFullTextExpand"
          >
            {{ fullTextExpanded ? 'close_fullscreen' : 'open_in_full' }}
          </button>
          <button
            class="material-symbols-outlined text-[18px] text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer rounded px-1 transition-colors"
            title="Open in system viewer"
            @click="openFileExternally"
          >
            open_in_new
          </button>
          <button
            class="material-symbols-outlined text-[18px] text-red-400 hover:text-red-600 hover:bg-red-50 cursor-pointer rounded px-1 transition-colors"
            title="Delete full text attachment"
            @click="handleDeleteFullText"
          >
            delete
          </button>
          <button
            class="material-symbols-outlined text-[18px] text-slate-400 hover:text-slate-900 cursor-pointer rounded px-1 transition-colors"
            title="Close full text view"
            @click="closeFullTextView"
          >
            close
          </button>
        </div>
      </div>
      <!-- PDF inline viewer using Blob URL -->
      <div v-if="isPdfAttachment && pdfSrc" class="flex-1 overflow-hidden">
        <iframe :src="pdfSrc" class="w-full h-full border-0" title="PDF Viewer" />
      </div>
      <!-- Fallback: extracted text (for TXT, or when Blob URL failed) -->
      <div v-else class="flex-1 overflow-y-auto p-6">
        <pre
          v-if="fullTextContent || article.fullText"
          class="whitespace-pre-wrap font-body-main text-body-main text-on-surface leading-relaxed break-words"
          >{{ fullTextContent ?? article.fullText }}</pre
        >
        <div v-else class="text-center py-16 text-slate-400 text-sm">
          No full text content available.
        </div>
      </div>
    </div>

    <!-- Footer Actions -->
    <div class="p-4 border-t border-slate-100 flex gap-3 bg-slate-50/50 items-center">
      <!-- Left: Navigation -->
      <div class="flex items-center gap-1 shrink-0">
        <button
          class="material-symbols-outlined p-1 rounded-lg transition-colors"
          :class="
            hasPrevious
              ? 'text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer'
              : 'text-slate-200 cursor-not-allowed'
          "
          :title="hasPrevious ? 'Previous article' : 'No previous article'"
          @click="hasPrevious && emit('navigatePrev')"
        >
          chevron_left
        </button>
        <span class="text-xs text-slate-500 font-medium tabular-nums min-w-[4rem] text-center">
          {{ articlePosition }} of {{ articleTotal }}
        </span>
        <button
          class="material-symbols-outlined p-1 rounded-lg transition-colors"
          :class="
            hasNext
              ? 'text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer'
              : 'text-slate-200 cursor-not-allowed'
          "
          :title="hasNext ? 'Next article' : 'No next article'"
          @click="hasNext && emit('navigateNext')"
        >
          chevron_right
        </button>
      </div>
      <!-- Right: Action buttons -->
      <div class="flex gap-3 flex-1 justify-end">
        <button
          v-if="article.status !== 'included'"
          class="bg-emerald-600 text-white px-4 py-2 rounded-lg font-semibold text-sm hover:bg-emerald-700 active:scale-95 transition-all shadow-sm cursor-pointer"
          title="Include this article in your systematic review"
          @click="emit('moveArticle', article.id, 'included')"
        >
          Include
        </button>
        <button
          v-if="article.status !== 'rejected'"
          class="bg-white border border-slate-200 text-rose-700 px-4 py-2 rounded-lg font-semibold text-sm hover:bg-rose-50 transition-colors shadow-sm cursor-pointer"
          title="Reject this article from your systematic review"
          @click="emit('moveArticle', article.id, 'rejected')"
        >
          Reject
        </button>
        <button
          v-if="article.status !== 'working'"
          class="bg-white border border-slate-200 text-slate-700 px-4 py-2 rounded-lg font-semibold text-sm hover:bg-slate-50 transition-colors shadow-sm cursor-pointer"
          title="Move this article back to Working status"
          @click="emit('moveArticle', article.id, 'working')"
        >
          Move to Working
        </button>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.detail-panel {
  width: var(--detail-panel-width);
  flex-shrink: 0;
  transition: width 0.2s ease;
}

.detail-panel--fullscreen {
  width: 100%;
  flex-shrink: 1;
  max-width: 960px;
  margin: 0 auto;
  border-left: none;
  box-shadow: none;
}

@media (max-width: 1023px) {
  .detail-panel {
    position: fixed;
    top: 0;
    right: 0;
    width: 100%;
    max-width: 100%;
    height: 100vh;
    border-left: none;
    z-index: 60;
    animation: slideInRight 0.25s ease;
  }
}

@keyframes slideInRight {
  from {
    transform: translateX(100%);
  }
  to {
    transform: translateX(0);
  }
}

/* Inline decision toast animation */
.decision-toast-enter-active {
  transition: all 0.3s ease-out;
}
.decision-toast-leave-active {
  transition: all 0.25s ease-in;
}
.decision-toast-enter-from {
  transform: translateX(100%);
  opacity: 0;
}
.decision-toast-leave-to {
  opacity: 0;
}
</style>
