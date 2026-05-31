<script setup lang="ts">
import { ref, computed, watch } from 'vue';
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
    class="detail-panel h-full bg-white shadow-[0_4px_24px_rgba(0,0,0,0.15)] border-l border-slate-200 flex flex-col z-50 relative"
    :class="{ 'transition-none': isResizing }"
    :style="{ '--detail-panel-width': panelWidth + 'px' }"
  >
    <!-- Resize Handle (desktop only) -->
    <div
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
        </div>
        <button
          class="material-symbols-outlined text-slate-400 hover:text-slate-900 transition-colors cursor-pointer"
          @click="emit('close')"
        >
          {{ hasReturnTarget ? 'arrow_back' : 'close' }}
        </button>
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
                href="#"
                @click.prevent
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
                class="text-[10px] font-bold text-emerald-600 bg-emerald-50 rounded px-1 leading-tight"
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
                class="text-[10px] font-bold text-rose-500 bg-rose-50 rounded px-1 leading-tight"
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

    <!-- Footer Actions -->
    <div class="p-4 border-t border-slate-100 flex gap-3 bg-slate-50/50 items-center">
      <button
        v-if="hasPrevious"
        class="material-symbols-outlined text-slate-400 hover:text-indigo-600 cursor-pointer transition-colors p-1 rounded-lg hover:bg-indigo-50"
        title="Previous article"
        @click="emit('navigatePrev')"
      >
        chevron_left
      </button>
      <div class="flex gap-3 flex-1">
        <button
          v-if="article.status !== 'included'"
          class="flex-1 bg-emerald-600 text-white py-2 rounded-lg font-semibold text-sm hover:bg-emerald-700 active:scale-95 transition-all shadow-sm cursor-pointer"
          @click="emit('moveArticle', article.id, 'included')"
        >
          Include
        </button>
        <button
          v-if="article.status !== 'rejected'"
          class="flex-1 bg-white border border-slate-200 text-rose-700 py-2 rounded-lg font-semibold text-sm hover:bg-rose-50 transition-colors shadow-sm cursor-pointer"
          @click="emit('moveArticle', article.id, 'rejected')"
        >
          Reject
        </button>
        <button
          v-if="article.status !== 'working'"
          class="flex-1 bg-white border border-slate-200 text-slate-700 py-2 rounded-lg font-semibold text-sm hover:bg-slate-50 transition-colors shadow-sm cursor-pointer"
          @click="emit('moveArticle', article.id, 'working')"
        >
          Move to Working
        </button>
      </div>
      <button
        v-if="hasNext"
        class="material-symbols-outlined text-slate-400 hover:text-indigo-600 cursor-pointer transition-colors p-1 rounded-lg hover:bg-indigo-50"
        title="Next article"
        @click="emit('navigateNext')"
      >
        chevron_right
      </button>
    </div>
  </aside>
</template>

<style scoped>
.detail-panel {
  width: var(--detail-panel-width);
  flex-shrink: 0;
  transition: width 0.2s ease;
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
</style>
