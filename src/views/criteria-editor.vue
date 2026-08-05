<script setup lang="ts">
import { ref, computed, nextTick, watch, onMounted } from 'vue';
import { onBeforeRouteLeave, useRouter } from 'vue-router';
import { marked } from 'marked';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useCriteriaStore } from '@/stores/criteria';
import { useLlmConfigured } from '@/composables/use-llm-configured';
import { useToast } from '@/composables/use-toast';
import { formatLlmError } from '@/utils/llm-error';
import type { SearchStrategyResult } from '@/types/search-strategy';
import SearchStrategyCard from '@/components/search-strategy-card.vue';
import { useOpenAlexStore } from '@/stores/openalex';
import type { Priority } from '@/types';
import type { SmartSearchQuery } from '@/types/openalex';
import { useInlineEdit } from '@/composables/use-inline-edit';
import type { Criterion, ResearchAim } from '@/types';

const toast = useToast();

const criteriaStore = useCriteriaStore();

// Canonical LLM-configured gate (wraps `useLlmConfigStore().isConfigured`).
// Mirrors backend `llm_config_repo::has_config` contract; pre-warms store.
const llmConfigured = useLlmConfigured();

// Read directly from store - pre-warmed at startup, no onMounted fetch needed.
const aims = computed(() => criteriaStore.aims);
const criteria = computed(() => criteriaStore.criteria);

const newAimText = ref('');
const newInclusionText = ref('');
const newExclusionText = ref('');
const newInclusionPriority = ref<Priority>('standard');
const newExclusionPriority = ref<Priority>('standard');

// AI assistant state lives in the Pinia store (persists across route navigation)
const generatingInclusion = computed(() => criteriaStore.generatingInclusion);
const generatingExclusion = computed(() => criteriaStore.generatingExclusion);
const inclusionCritiqueText = computed(() => criteriaStore.inclusionCritique);
const exclusionCritiqueText = computed(() => criteriaStore.exclusionCritique);
const inclusionError = computed(() => criteriaStore.inclusionError);
const exclusionError = computed(() => criteriaStore.exclusionError);

// Search Strategy Builder state (session-scoped, mirrors the critique refs).
const generatingSearchStrategy = computed(() => criteriaStore.generatingSearchStrategy);
const searchStrategyResult = computed<SearchStrategyResult | null>(
  () => criteriaStore.searchStrategyResult
);
const searchStrategyError = computed(() => criteriaStore.searchStrategyError);

const inclusionCriteria = computed(() =>
  criteria.value.filter((c) => c.criterionType === 'inclusion')
);
const exclusionCriteria = computed(() =>
  criteria.value.filter((c) => c.criterionType === 'exclusion')
);

/** Global criterion numbering: inclusion [1]..[N], exclusion [N+1]..[N+M] */
const criterionIndexMap = computed(() => {
  const map = new Map<string, number>();
  let n = 1;
  for (const c of inclusionCriteria.value) {
    map.set(c.id, n++);
  }
  for (const c of exclusionCriteria.value) {
    map.set(c.id, n++);
  }
  return map;
});

async function refetch(): Promise<void> {
  await criteriaStore.refresh();
}

async function addAim(): Promise<void> {
  if (!newAimText.value.trim()) return;
  await tauriCommand('create_research_aim', {
    request: { text: newAimText.value.trim() },
  });
  newAimText.value = '';
  await refetch();
}

async function deleteAim(id: string): Promise<void> {
  await tauriCommand('delete_research_aim', { id });
  await refetch();
}

async function addInclusion(): Promise<void> {
  if (!newInclusionText.value.trim()) return;
  await tauriCommand('create_criterion', {
    request: {
      criterionType: 'inclusion',
      text: newInclusionText.value.trim(),
      priority: newInclusionPriority.value,
    },
  });
  newInclusionText.value = '';
  newInclusionPriority.value = 'standard';
  await refetch();
}

async function addExclusion(): Promise<void> {
  if (!newExclusionText.value.trim()) return;
  await tauriCommand('create_criterion', {
    request: {
      criterionType: 'exclusion',
      text: newExclusionText.value.trim(),
      priority: newExclusionPriority.value,
    },
  });
  newExclusionText.value = '';
  newExclusionPriority.value = 'standard';
  await refetch();
}

async function updateCriterionPriority(
  id: string,
  text: string,
  priority: Priority
): Promise<void> {
  await tauriCommand('update_criterion', {
    request: { id, text, priority },
  });
  await refetch();
}

async function deleteCriterion(id: string): Promise<void> {
  await tauriCommand('delete_criterion', { id });
  await refetch();
}

async function updateAim(id: string, text: string): Promise<void> {
  await tauriCommand('update_research_aim', {
    request: { id, text },
  });
  await refetch();
}

async function updateCriterionText(id: string, text: string, priority: Priority): Promise<void> {
  await tauriCommand('update_criterion', {
    request: { id, text, priority },
  });
  await refetch();
}

/* Inline edit controllers. One for aims, one for criteria (covers both
 * inclusion and exclusion; criterion ids are globally-unique UUIDs). */
const aimEdit = useInlineEdit<ResearchAim>({
  saveItem: async (item, newText) => {
    await updateAim(item.id, newText);
  },
  deleteItem: async (item) => {
    await deleteAim(item.id);
  },
  getText: (item) => item.text,
});

const criterionEdit = useInlineEdit<Criterion>({
  saveItem: async (item, newText) => {
    await updateCriterionText(item.id, newText, item.priority);
  },
  deleteItem: async (item) => {
    await deleteCriterion(item.id);
  },
  getText: (item) => item.text,
});

/**
 * Character offset where user double-clicked in read-only text, captured
 * BEFORE input swaps in. Places edit caret at click point. `null` = default
 * to position 0.
 */
const pendingCaretOffset = ref<number | null>(null);

/**
 * Compute character offset of text node under (clientX, clientY). Uses
 * standard `caretPositionFromPoint` (Firefox) or WebKit/Chromium
 * `caretRangeFromPoint` fallback.
 */
function caretOffsetAtPoint(x: number, y: number): number | null {
  // Standard (Firefox): returns { offsetNode, offset }.
  const perf = document as unknown as {
    caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null;
  };
  if (typeof perf.caretPositionFromPoint === 'function') {
    const pos = perf.caretPositionFromPoint(x, y);
    if (pos && typeof pos.offset === 'number') return pos.offset;
    return null;
  }
  // WebKit / Chromium: returns a Range whose start is the caret.
  const doc = document as unknown as {
    caretRangeFromPoint?: (x: number, y: number) => Range | null;
  };
  if (typeof doc.caretRangeFromPoint === 'function') {
    const range = doc.caretRangeFromPoint(x, y);
    if (range) return range.startOffset;
  }
  return null;
}

/**
 * Dbl-click handler: capture caret offset at click point (against read-only
 * text element) BEFORE entering edit mode. Offset consumed by `focusInlineInput`
 * once the &lt;input&gt; mounts.
 */
function handleInlineDblClick<T extends { id: string }>(
  item: T,
  event: MouseEvent,
  controller: { startEdit: (item: T) => void }
): void {
  pendingCaretOffset.value = caretOffsetAtPoint(event.clientX, event.clientY);
  controller.startEdit(item);
}

/**
 * Plain template ref for whichever inline-edit &lt;textarea&gt; is mounted.
 * At most one is visible at a time. NOT a function ref: it would re-run on
 * every render (including `v-model` keystrokes) and jump the caret to 0.
 */
const inlineInputEl = ref<HTMLTextAreaElement | null>(null);

/**
 * Auto-grow the editing textarea to fit its content (min 2 rows, max ~6 rows
 * before it scrolls). Called once on edit-start and on every `@input` so the
 * box always matches the text height. Idempotent and cheap.
 */
function autoResizeTextarea(): void {
  const el = inlineInputEl.value;
  if (!(el instanceof HTMLTextAreaElement)) return;
  el.style.height = 'auto';
  // scrollHeight is the content height; clamp to a 6-row cap (~144px at 24px
  // line-height) so very long text scrolls instead of growing unbounded.
  el.style.height = Math.min(el.scrollHeight, 144) + 'px';
}

/**
 * Focus the inline-edit textarea and place the edit caret (NOT a selection) at
 * the captured click offset, falling back to position 0. Runs ONCE per
 * edit-session start, triggered by the `watch` on the editing id - never during
 * typing. Also auto-resizes the textarea so multi-line text fits on open.
 */
function placeCaretOnce(): void {
  const el = inlineInputEl.value;
  if (!(el instanceof HTMLTextAreaElement)) return;
  autoResizeTextarea();
  el.focus();
  const value = el.value;
  const requested = pendingCaretOffset.value;
  pendingCaretOffset.value = null;
  // Clamp to [0, value.length]; default to 0 when no offset was captured.
  const pos = requested === null ? 0 : Math.max(0, Math.min(requested, value.length));
  el.setSelectionRange(pos, pos);
}

/**
 * Keydown handler for the inline-edit <textarea>. Chat-dialog convention:
 *   - Enter (no shift)  -> commit (save)
 *   - Shift+Enter       -> newline (default textarea behavior, no preventDefault)
 * Only listens for Enter; Escape and blur are handled by their own bindings.
 */
function onTextareaEnter<T extends { id: string }>(
  item: T,
  event: KeyboardEvent,
  controller: { commitEdit: (item: T) => Promise<void> }
): void {
  if (event.shiftKey) return; // let the newline through
  event.preventDefault();
  void controller.commitEdit(item);
}

/**
 * Watch the aim editing id: when it transitions to a real id (entering edit
 * mode), wait one tick for the <input> to mount, then place the caret once.
 * Subsequent keystrokes update `draftText` via `v-model` but do NOT re-fire
 * this watcher (the id does not change while typing), so the caret is never
 * disturbed mid-edit.
 */
watch(aimEdit.editingId, (next, prev) => {
  if (prev === null && next !== null) {
    nextTick(placeCaretOnce);
  }
});

/**
 * Same watch for criteria (covers both inclusion + exclusion; only one
 * criterion can be edited at a time so a single watcher suffices).
 */
watch(criterionEdit.editingId, (next, prev) => {
  if (prev === null && next !== null) {
    nextTick(placeCaretOnce);
  }
});

function priorityBorderClass(priority: Priority): string {
  const map: Record<Priority, string> = {
    critical: 'border-l-4 border-red-500 bg-red-50/30',
    high: 'border-l-4 border-orange-500 bg-orange-50/30',
    standard: 'border-l-4 border-indigo-500 bg-indigo-50/30',
    low: 'border-l-4 border-slate-400 bg-slate-50/50',
    optional: 'border-l-4 border-slate-300 border-dashed bg-white',
  };
  return map[priority];
}

function priorityLabelClass(priority: Priority): string {
  const map: Record<Priority, string> = {
    critical: 'text-red-700',
    high: 'text-orange-700',
    standard: 'text-indigo-700',
    low: 'text-slate-600',
    optional: 'text-slate-400',
  };
  return map[priority];
}

function priorityLabel(priority: Priority): string {
  const map: Record<Priority, string> = {
    critical: 'Critical Criterion',
    high: 'High Priority',
    standard: 'Standard Criterion',
    low: 'Low Priority',
    optional: 'Optional/Draft',
  };
  return map[priority];
}

// ── AI assistant logic ──────────────────────────────────────────────

const hasAims = computed(() => aims.value.length > 0);
/* Use canonical gate so local providers (LM Studio / Ollama / llama.cpp)
 * enable AI buttons. Re-deriving from `apiKeyEncrypted` would disable them. */
const canUseAi = computed(() => hasAims.value && llmConfigured.value);

const canGenerateStrategy = computed(() => hasAims.value && llmConfigured.value);
const strategyButtonTitle = computed(() => {
  if (!hasAims.value) return 'Add at least one research aim first';
  if (!llmConfigured.value) return 'Configure an LLM in Settings first';
  return 'Generate database-ready Boolean search strings from your aims';
});

const inclusionButtonLabel = computed(() =>
  inclusionCriteria.value.length === 0 ? 'Generate with AI' : 'Critique with AI'
);
const exclusionButtonLabel = computed(() =>
  exclusionCriteria.value.length === 0 ? 'Generate with AI' : 'Critique with AI'
);

async function handleInclusionAi(): Promise<void> {
  if (!canUseAi.value || generatingInclusion.value) return;
  criteriaStore.generatingInclusion = true;
  criteriaStore.inclusionCritique = '';
  criteriaStore.inclusionError = null;
  try {
    if (inclusionCriteria.value.length === 0) {
      await tauriCommand('generate_criteria', {
        request: { criterionType: 'inclusion' },
      });
      await refetch();
    } else {
      const result = await tauriCommand<{ critique: string }>('critique_criteria', {
        request: { criterionType: 'inclusion' },
      });
      criteriaStore.inclusionCritique = result.critique;
      // Auto-expand so a freshly-generated critique shows its body.
      criteriaStore.inclusionCritiqueExpanded = true;
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    criteriaStore.inclusionError = msg;
  } finally {
    criteriaStore.generatingInclusion = false;
  }
}

/** Dismiss the inclusion critique card: clear the text and reset the collapse
 * state so the next generation starts expanded. */
function dismissInclusionCritique(): void {
  criteriaStore.inclusionCritique = '';
  criteriaStore.inclusionCritiqueExpanded = true;
}

/** Render LLM Markdown critique to safe HTML. Matches the pattern in
 * `summary-view.vue`, `chat-view.vue`, and `wiki-page-editor.vue`:
 * `marked.parse(text) as string` fed to `v-html`. Content is LLM-generated
 * critique prose (no user-controlled wikilinks/footnotes). */
function renderCritiqueMarkdown(text: string): string {
  return marked.parse(text) as string;
}

async function handleExclusionAi(): Promise<void> {
  if (!canUseAi.value || generatingExclusion.value) return;
  criteriaStore.generatingExclusion = true;
  criteriaStore.exclusionCritique = '';
  criteriaStore.exclusionError = null;
  try {
    if (exclusionCriteria.value.length === 0) {
      await tauriCommand('generate_criteria', {
        request: { criterionType: 'exclusion' },
      });
      await refetch();
    } else {
      const result = await tauriCommand<{ critique: string }>('critique_criteria', {
        request: { criterionType: 'exclusion' },
      });
      criteriaStore.exclusionCritique = result.critique;
      // Auto-expand so a freshly-generated critique shows its body.
      criteriaStore.exclusionCritiqueExpanded = true;
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    criteriaStore.exclusionError = msg;
  } finally {
    criteriaStore.generatingExclusion = false;
  }
}

/** Dismiss the exclusion critique card: clear the text and reset the collapse
 * state so the next generation starts expanded. */
function dismissExclusionCritique(): void {
  criteriaStore.exclusionCritique = '';
  criteriaStore.exclusionCritiqueExpanded = true;
}

/* Custom Screening Instructions + Check Rules. Local draft mirrors persisted
 * store value while user edits. Load on mount from store so navigation
 * away-and-back preserves unsaved session edits. */
const customLogicDraft = ref('');
const showHelpPopover = ref(false);
let helpPopoverTimeout: ReturnType<typeof setTimeout> | null = null;

function showHelp(): void {
  if (helpPopoverTimeout) {
    clearTimeout(helpPopoverTimeout);
    helpPopoverTimeout = null;
  }
  showHelpPopover.value = true;
}

function scheduleHideHelp(): void {
  if (helpPopoverTimeout) clearTimeout(helpPopoverTimeout);
  helpPopoverTimeout = setTimeout(() => {
    showHelpPopover.value = false;
  }, 200);
}

async function autoSaveCustomLogic(): Promise<void> {
  const draft = customLogicDraft.value;
  if (draft === criteriaStore.customLogic) return;
  try {
    await criteriaStore.saveCustomLogic(draft);
    criteriaStore.customLogic = draft;
  } catch (e: unknown) {
    toast.show(
      `Failed to save instructions: ${e instanceof Error ? e.message : String(e)}`,
      'error'
    );
  }
}

async function handleCheckRules(): Promise<void> {
  if (!canUseAi.value || criteriaStore.generatingRulesCheck) return;
  await criteriaStore.runRulesCheck();
}

/** Dismiss the rules-check critique card: clear the text and reset collapse. */
function dismissRulesCritique(): void {
  criteriaStore.rulesCritique = '';
  criteriaStore.rulesCritiqueExpanded = true;
}

onBeforeRouteLeave(() => {
  void autoSaveCustomLogic();
});

onMounted(() => {
  void criteriaStore.loadCustomLogic().then(() => {
    customLogicDraft.value = criteriaStore.customLogic;
  });
});

// ── Search Strategy Builder ────────────────────────────────────────────

async function handleSearchStrategy(): Promise<void> {
  if (!canGenerateStrategy.value || generatingSearchStrategy.value) return;
  criteriaStore.generatingSearchStrategy = true;
  criteriaStore.searchStrategyError = null;
  try {
    const result = await tauriCommand<SearchStrategyResult>('suggest_search_strategy');
    criteriaStore.searchStrategyResult = result;
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    criteriaStore.searchStrategyError = msg;
  } finally {
    criteriaStore.generatingSearchStrategy = false;
  }
}

// ── OpenAlex Search Integration (Tier 3) ───────────────────────────────

const router = useRouter();
const openalexStore = useOpenAlexStore();
const openalexLoading = ref(false);

/** Navigate to the Articles Search tab with an empty query. */
function handleOpenAlexSearchNow(): void {
  void router.push({ path: '/articles', query: { status: 'search' } });
}

/** Call the LLM Smart Search, land the query in the OpenAlex store, then
 *  navigate to the Articles Search tab so the user can review and execute. */
async function handleOpenAlexSmartSearch(): Promise<void> {
  if (!canUseAi.value || openalexLoading.value) return;
  openalexLoading.value = true;
  try {
    const result = await tauriCommand<SmartSearchQuery>('smart_search_openalex');
    openalexStore.setQuery(result.searchQuery);
    // Auto-populate filters from suggested filters.
    if (result.suggestedFilters.publicationYear) {
      const [fromStr, toStr] = result.suggestedFilters.publicationYear.split('-');
      openalexStore.filters = {
        ...openalexStore.filters,
        yearFrom: fromStr ? Number(fromStr) : null,
        yearTo: toStr ? Number(toStr) : null,
        workTypes:
          result.suggestedFilters.type.length > 0
            ? [...result.suggestedFilters.type]
            : openalexStore.filters.workTypes,
      };
    }
    void router.push({ path: '/articles', query: { status: 'search' } });
  } catch (err: unknown) {
    toast.show(`Smart Search failed: ${err instanceof Error ? err.message : String(err)}`, 'error');
  } finally {
    openalexLoading.value = false;
  }
}
</script>

<template>
  <div class="criteria-editor">
    <div class="criteria-editor__header">
      <h1 class="page-title">Criteria</h1>
      <p class="page-subtitle">Define research aims and inclusion/exclusion criteria</p>
    </div>

    <!-- Section 1: Research Aims -->
    <section class="section-panel">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-primary">target</span>
          <h2 class="section-panel__title">Research Aims</h2>
        </div>
        <div v-if="generatingSearchStrategy" class="ai-loading">
          <span class="material-symbols-outlined animate-spin">progress_activity</span>
          <span>Generating…</span>
        </div>
        <button
          v-else
          class="ai-btn"
          :disabled="!canGenerateStrategy"
          :title="strategyButtonTitle"
          @click="handleSearchStrategy"
        >
          <span class="material-symbols-outlined">manage_search</span>
          Suggest Search Strategy
        </button>
      </div>
      <div class="space-y-3">
        <div v-for="(aim, index) in aims" :key="aim.id" class="aim-row group">
          <span class="aim-row__number">{{ index + 1 }}</span>
          <div v-if="aimEdit.isEditing(aim.id)" class="inline-edit-wrap">
            <textarea
              ref="inlineInputEl"
              v-model="aimEdit.draftText.value"
              rows="2"
              class="inline-edit-textarea"
              :class="{ 'inline-edit-textarea--saving': aimEdit.saving.value }"
              @keydown.enter="onTextareaEnter(aim, $event, aimEdit)"
              @keydown.escape.prevent="aimEdit.cancelEdit()"
              @input="autoResizeTextarea"
              @blur="aimEdit.commitEdit(aim)"
            />
            <p class="inline-edit-hint">Hit enter to save and SHIFT-ENTER for new line.</p>
          </div>
          <span
            v-else
            class="aim-row__text aim-row__text--editable"
            title="Double-click to edit"
            @dblclick="handleInlineDblClick(aim, $event, aimEdit)"
            >{{ aim.text }}</span
          >
          <button class="aim-row__delete" @click="deleteAim(aim.id)">
            <span class="material-symbols-outlined">delete</span>
          </button>
        </div>
        <!-- Add new aim -->
        <div class="aim-row">
          <span class="aim-row__number">{{ aims.length + 1 }}</span>
          <input
            v-model="newAimText"
            type="text"
            class="aim-row__input"
            placeholder="Add new research aim..."
            @keyup.enter="addAim"
          />
        </div>
      </div>
    </section>

    <!-- Search Strategy error card -->
    <div v-if="searchStrategyError" class="ai-error-card">
      <div class="ai-error-card__header">
        <div class="ai-error-card__title-group">
          <span class="material-symbols-outlined">error</span>
          <span class="ai-error-card__title">Search Strategy Generation Failed</span>
        </div>
        <button class="ai-error-card__dismiss" @click="criteriaStore.searchStrategyError = null">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-error-card__body">
        <p class="ai-error-card__prefix">{{ formatLlmError(searchStrategyError).prefix }}</p>
        <p v-if="formatLlmError(searchStrategyError).cause" class="ai-error-card__cause">
          <strong>Cause:</strong> {{ formatLlmError(searchStrategyError).cause }}
        </p>
        <p v-if="formatLlmError(searchStrategyError).solution" class="ai-error-card__solution">
          <strong>Solution:</strong> {{ formatLlmError(searchStrategyError).solution }}
        </p>
        <details class="ai-error-card__details">
          <summary>Technical details</summary>
          <pre>{{ searchStrategyError }}</pre>
        </details>
        <a
          :href="formatLlmError(searchStrategyError).helpLink"
          class="ai-error-card__help-link"
          target="_blank"
        >
          <span class="material-symbols-outlined" style="font-size: 14px">help</span>
          Troubleshooting guide
        </a>
      </div>
    </div>

    <!-- Search Strategy result card (session-scoped) -->
    <SearchStrategyCard
      v-if="searchStrategyResult"
      :result="searchStrategyResult"
      @dismiss="criteriaStore.searchStrategyResult = null"
    />

    <!-- OpenAlex Search card (Tier 3: bridges criteria to executed search) -->
    <section class="section-panel openalex-card">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-indigo-600">travel_explore</span>
          <h2 class="section-panel__title">OpenAlex Search</h2>
        </div>
        <div v-if="openalexLoading" class="ai-loading">
          <span class="material-symbols-outlined animate-spin">progress_activity</span>
          <span>Generating...</span>
        </div>
      </div>
      <p class="openalex-card__desc">
        Search the OpenAlex catalog of 300M+ scholarly works directly from Bango. Import results
        into your Working list with one click.
      </p>
      <div class="openalex-card__actions">
        <button class="ai-btn" @click="handleOpenAlexSearchNow">
          <span class="material-symbols-outlined">search</span>
          Search OpenAlex Now
        </button>
        <button
          class="ai-btn"
          :disabled="!canUseAi || openalexLoading"
          :title="
            !canUseAi
              ? 'Configure an LLM in Settings first'
              : 'Generate an OpenAlex Boolean query from your aims + criteria'
          "
          @click="handleOpenAlexSmartSearch"
        >
          <span class="material-symbols-outlined">auto_awesome</span>
          Smart Search OpenAlex
        </button>
      </div>
    </section>

    <!-- Section 2: Inclusion Criteria -->
    <section class="section-panel">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-green-600">check_circle</span>
          <h2 class="section-panel__title">Inclusion Criteria</h2>
        </div>
        <div v-if="generatingInclusion" class="ai-loading">
          <span class="material-symbols-outlined animate-spin">progress_activity</span>
          <span>Generating…</span>
        </div>
        <button v-else class="ai-btn" :disabled="!canUseAi" @click="handleInclusionAi">
          <span class="material-symbols-outlined">auto_awesome</span>
          {{ inclusionButtonLabel }}
        </button>
      </div>

      <div class="space-y-4 mt-4">
        <div
          v-for="c in inclusionCriteria"
          :key="c.id"
          class="criterion-card group"
          :class="priorityBorderClass(c.priority)"
        >
          <span class="criterion-card__index criterion-card__index--inc">
            {{ criterionIndexMap.get(c.id) }}
          </span>
          <div class="flex-1">
            <label class="criterion-card__label" :class="priorityLabelClass(c.priority)">
              {{ priorityLabel(c.priority) }}
            </label>
            <div v-if="criterionEdit.isEditing(c.id)" class="inline-edit-wrap">
              <textarea
                ref="inlineInputEl"
                v-model="criterionEdit.draftText.value"
                rows="2"
                class="inline-edit-textarea"
                :class="{ 'inline-edit-textarea--saving': criterionEdit.saving.value }"
                @keydown.enter="onTextareaEnter(c, $event, criterionEdit)"
                @keydown.escape.prevent="criterionEdit.cancelEdit()"
                @input="autoResizeTextarea"
                @blur="criterionEdit.commitEdit(c)"
              />
              <p class="inline-edit-hint">Hit enter to save and SHIFT-ENTER for new line.</p>
            </div>
            <p
              v-else
              class="criterion-card__text criterion-card__text--editable"
              title="Double-click to edit"
              @dblclick="handleInlineDblClick(c, $event, criterionEdit)"
            >
              {{ c.text }}
            </p>
          </div>
          <div class="criterion-card__actions">
            <select
              :value="c.priority"
              class="priority-select"
              @change="
                updateCriterionPriority(
                  c.id,
                  c.text,
                  ($event.target as HTMLSelectElement).value as Priority
                )
              "
            >
              <option value="critical">Critical</option>
              <option value="high">High</option>
              <option value="standard">Standard</option>
              <option value="low">Low</option>
              <option value="optional">Optional</option>
            </select>
            <button class="criterion-card__delete" @click="deleteCriterion(c.id)">
              <span class="material-symbols-outlined">delete</span>
            </button>
          </div>
        </div>

        <!-- Add new inclusion criterion (at the END, like Research Aims) -->
        <div class="criterion-add-row">
          <span class="aim-row__number">{{ inclusionCriteria.length + 1 }}</span>
          <select
            v-model="newInclusionPriority"
            class="priority-select criterion-add-row__priority"
          >
            <option value="critical">Critical</option>
            <option value="high">High</option>
            <option value="standard">Standard</option>
            <option value="low">Low</option>
            <option value="optional">Optional</option>
          </select>

          <input
            v-model="newInclusionText"
            type="text"
            class="criterion-add-row__input"
            placeholder="Add new inclusion criterion..."
            @keyup.enter="addInclusion"
          />
          <button class="btn-primary-sm criterion-add-row__add-btn" @click="addInclusion">
            Add
          </button>
        </div>
        <p class="criterion-add-row__hint">Hit enter or click Add button to save criterion</p>
      </div>
    </section>

    <!-- AI Error: Inclusion -->
    <div v-if="inclusionError" class="ai-error-card">
      <div class="ai-error-card__header">
        <div class="ai-error-card__title-group">
          <span class="material-symbols-outlined">error</span>
          <span class="ai-error-card__title">AI Generation Failed - Inclusion Criteria</span>
        </div>
        <button class="ai-error-card__dismiss" @click="criteriaStore.inclusionError = null">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-error-card__body">
        <p class="ai-error-card__prefix">{{ formatLlmError(inclusionError).prefix }}</p>
        <p v-if="formatLlmError(inclusionError).cause" class="ai-error-card__cause">
          <strong>Cause:</strong> {{ formatLlmError(inclusionError).cause }}
        </p>
        <p v-if="formatLlmError(inclusionError).solution" class="ai-error-card__solution">
          <strong>Solution:</strong> {{ formatLlmError(inclusionError).solution }}
        </p>
        <details class="ai-error-card__details">
          <summary>Technical details</summary>
          <pre>{{ inclusionError }}</pre>
        </details>
        <a
          :href="formatLlmError(inclusionError).helpLink"
          class="ai-error-card__help-link"
          target="_blank"
        >
          <span class="material-symbols-outlined" style="font-size: 14px">help</span>
          Troubleshooting guide
        </a>
      </div>
    </div>

    <!-- AI Critique: Inclusion -->
    <div v-if="inclusionCritiqueText" class="ai-critique-card">
      <div class="ai-critique-card__header">
        <div class="ai-critique-card__title-group">
          <span class="material-symbols-outlined">auto_awesome</span>
          <span class="ai-critique-card__title">AI Critique - Inclusion Criteria</span>
        </div>
        <div class="ai-critique-card__header-actions">
          <button
            class="ai-critique-card__toggle"
            :title="criteriaStore.inclusionCritiqueExpanded ? 'Collapse' : 'Expand'"
            @click="
              criteriaStore.inclusionCritiqueExpanded = !criteriaStore.inclusionCritiqueExpanded
            "
          >
            <span class="material-symbols-outlined">{{
              criteriaStore.inclusionCritiqueExpanded ? 'expand_less' : 'expand_more'
            }}</span>
          </button>
          <button class="ai-critique-card__dismiss" @click="dismissInclusionCritique">
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>
      </div>
      <div
        v-if="criteriaStore.inclusionCritiqueExpanded"
        class="markdown-content ai-critique-card__body"
        v-html="renderCritiqueMarkdown(inclusionCritiqueText)"
      />
    </div>

    <!-- Section 3: Exclusion Criteria -->
    <section class="section-panel">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-error">cancel</span>
          <h2 class="section-panel__title">Exclusion Criteria</h2>
        </div>
        <div v-if="generatingExclusion" class="ai-loading">
          <span class="material-symbols-outlined animate-spin">progress_activity</span>
          <span>Generating…</span>
        </div>
        <button v-else class="ai-btn" :disabled="!canUseAi" @click="handleExclusionAi">
          <span class="material-symbols-outlined">auto_awesome</span>
          {{ exclusionButtonLabel }}
        </button>
      </div>

      <div class="space-y-4 mt-4">
        <div
          v-for="c in exclusionCriteria"
          :key="c.id"
          class="criterion-card group"
          :class="priorityBorderClass(c.priority)"
        >
          <span class="criterion-card__index criterion-card__index--exc">
            {{ criterionIndexMap.get(c.id) }}
          </span>
          <div class="flex-1">
            <label class="criterion-card__label" :class="priorityLabelClass(c.priority)">
              {{ priorityLabel(c.priority) }}
            </label>
            <div v-if="criterionEdit.isEditing(c.id)" class="inline-edit-wrap">
              <textarea
                ref="inlineInputEl"
                v-model="criterionEdit.draftText.value"
                rows="2"
                class="inline-edit-textarea"
                :class="{ 'inline-edit-textarea--saving': criterionEdit.saving.value }"
                @keydown.enter="onTextareaEnter(c, $event, criterionEdit)"
                @keydown.escape.prevent="criterionEdit.cancelEdit()"
                @input="autoResizeTextarea"
                @blur="criterionEdit.commitEdit(c)"
              />
              <p class="inline-edit-hint">Hit enter to save and SHIFT-ENTER for new line.</p>
            </div>
            <p
              v-else
              class="criterion-card__text criterion-card__text--editable"
              title="Double-click to edit"
              @dblclick="handleInlineDblClick(c, $event, criterionEdit)"
            >
              {{ c.text }}
            </p>
          </div>
          <div class="criterion-card__actions">
            <select
              :value="c.priority"
              class="priority-select"
              @change="
                updateCriterionPriority(
                  c.id,
                  c.text,
                  ($event.target as HTMLSelectElement).value as Priority
                )
              "
            >
              <option value="critical">Critical</option>
              <option value="high">High</option>
              <option value="standard">Standard</option>
              <option value="low">Low</option>
              <option value="optional">Optional</option>
            </select>
            <button class="criterion-card__delete" @click="deleteCriterion(c.id)">
              <span class="material-symbols-outlined">delete</span>
            </button>
          </div>
        </div>

        <!-- Add new exclusion criterion (at the END, like Research Aims) -->
        <div class="criterion-add-row">
          <span class="aim-row__number">{{ exclusionCriteria.length + 1 }}</span>
          <select
            v-model="newExclusionPriority"
            class="priority-select criterion-add-row__priority"
          >
            <option value="critical">Critical</option>
            <option value="high">High</option>
            <option value="standard">Standard</option>
            <option value="low">Low</option>
            <option value="optional">Optional</option>
          </select>

          <input
            v-model="newExclusionText"
            type="text"
            class="criterion-add-row__input"
            placeholder="Add new exclusion criterion..."
            @keyup.enter="addExclusion"
          />
          <button class="btn-primary-sm criterion-add-row__add-btn" @click="addExclusion">
            Add
          </button>
        </div>
        <p class="criterion-add-row__hint">Hit enter or click Add button to save criterion</p>
      </div>
    </section>

    <!-- AI Error: Exclusion -->
    <div v-if="exclusionError" class="ai-error-card">
      <div class="ai-error-card__header">
        <div class="ai-error-card__title-group">
          <span class="material-symbols-outlined">error</span>
          <span class="ai-error-card__title">AI Generation Failed - Exclusion Criteria</span>
        </div>
        <button class="ai-error-card__dismiss" @click="criteriaStore.exclusionError = null">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-error-card__body">
        <p class="ai-error-card__prefix">{{ formatLlmError(exclusionError).prefix }}</p>
        <p v-if="formatLlmError(exclusionError).cause" class="ai-error-card__cause">
          <strong>Cause:</strong> {{ formatLlmError(exclusionError).cause }}
        </p>
        <p v-if="formatLlmError(exclusionError).solution" class="ai-error-card__solution">
          <strong>Solution:</strong> {{ formatLlmError(exclusionError).solution }}
        </p>
        <details class="ai-error-card__details">
          <summary>Technical details</summary>
          <pre>{{ exclusionError }}</pre>
        </details>
        <a
          :href="formatLlmError(exclusionError).helpLink"
          class="ai-error-card__help-link"
          target="_blank"
        >
          <span class="material-symbols-outlined" style="font-size: 14px">help</span>
          Troubleshooting guide
        </a>
      </div>
    </div>

    <!-- AI Critique: Exclusion -->
    <div v-if="exclusionCritiqueText" class="ai-critique-card">
      <div class="ai-critique-card__header">
        <div class="ai-critique-card__title-group">
          <span class="material-symbols-outlined">auto_awesome</span>
          <span class="ai-critique-card__title">AI Critique - Exclusion Criteria</span>
        </div>
        <div class="ai-critique-card__header-actions">
          <button
            class="ai-critique-card__toggle"
            :title="criteriaStore.exclusionCritiqueExpanded ? 'Collapse' : 'Expand'"
            @click="
              criteriaStore.exclusionCritiqueExpanded = !criteriaStore.exclusionCritiqueExpanded
            "
          >
            <span class="material-symbols-outlined">{{
              criteriaStore.exclusionCritiqueExpanded ? 'expand_less' : 'expand_more'
            }}</span>
          </button>
          <button class="ai-critique-card__dismiss" @click="dismissExclusionCritique">
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>
      </div>
      <div
        v-if="criteriaStore.exclusionCritiqueExpanded"
        class="markdown-content ai-critique-card__body"
        v-html="renderCritiqueMarkdown(exclusionCritiqueText)"
      />
    </div>

    <!-- Section 4: Custom Screening Instructions -->
    <section class="section-panel">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-indigo-600">rule</span>
          <h2 class="section-panel__title">Custom Screening Instructions</h2>
          <!-- Help popover: hover/focus the question-mark icon to see the AND/OR
               syntax guide. Uses a delayed hide so the user can move the pointer
               from the icon into the popover body without dismissing it. -->
          <div class="help-popover" @mouseenter="showHelp" @mouseleave="scheduleHideHelp">
            <button
              type="button"
              class="help-popover__icon"
              title="How to use Custom Screening Instructions"
              aria-label="How to use Custom Screening Instructions"
              @focus="showHelp"
              @blur="scheduleHideHelp"
              @click="showHelpPopover = !showHelpPopover"
            >
              <span class="material-symbols-outlined">help</span>
            </button>
            <Transition name="help-popover">
              <div
                v-if="showHelpPopover"
                class="help-popover__panel"
                role="tooltip"
                @mouseenter="showHelp"
                @mouseleave="scheduleHideHelp"
              >
                <p class="help-popover__lead">
                  Optional rules your AI applies when deciding include/exclude. Reference criteria
                  by their numbered position (inclusion is
                  <code>1..N</code>, exclusion continues <code>N+1..N+M</code>, matching the numbers
                  shown on this screen).
                </p>
                <p class="help-popover__examples-label">Examples:</p>
                <ul class="help-popover__examples">
                  <li>
                    "Inclusion criteria 2, 3, and 4 are mandatory AND gates - all three must match
                    for inclusion."
                  </li>
                  <li>
                    "Only if 2-4 are all satisfied, consider inclusion criterion 5 OR 6 as the final
                    inclusion signal."
                  </li>
                  <li>
                    "Exclusion criterion 9 is a hard gate; if it matches, ignore inclusion criteria
                    11-14."
                  </li>
                  <li>
                    "If inclusion 3 and 7 both match, exclusion 5 OR 6 must NOT match for
                    inclusion."
                  </li>
                </ul>
                <p class="help-popover__footer">Leave blank for default priority-only behavior.</p>
              </div>
            </Transition>
          </div>
        </div>
        <div v-if="criteriaStore.generatingRulesCheck" class="ai-loading">
          <span class="material-symbols-outlined animate-spin">progress_activity</span>
          <span>Checking…</span>
        </div>
        <button
          v-else
          class="ai-btn"
          :disabled="!canUseAi"
          :title="
            !canUseAi
              ? 'Add at least one research aim and configure an LLM first'
              : 'Run an AI consistency review of the whole ruleset'
          "
          @click="handleCheckRules"
        >
          <span class="material-symbols-outlined">auto_awesome</span>
          Check Rules
        </button>
      </div>

      <div class="custom-logic">
        <textarea
          v-model="customLogicDraft"
          class="custom-logic__textarea"
          rows="5"
          placeholder="e.g. Inclusion criteria 2, 3, and 4 are mandatory AND gates - all three must match for inclusion. Only then consider inclusion criterion 5 OR 6."
          @blur="autoSaveCustomLogic"
        />
        <p class="custom-logic__hint">
          Saved automatically on navigation. Use the criterion numbers shown above.
        </p>
      </div>
    </section>

    <!-- AI Error: Check Rules -->
    <div v-if="criteriaStore.rulesError" class="ai-error-card">
      <div class="ai-error-card__header">
        <div class="ai-error-card__title-group">
          <span class="material-symbols-outlined">error</span>
          <span class="ai-error-card__title">Check Rules Failed</span>
        </div>
        <button class="ai-error-card__dismiss" @click="criteriaStore.rulesError = null">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-error-card__body">
        <p class="ai-error-card__prefix">{{ formatLlmError(criteriaStore.rulesError).prefix }}</p>
        <p v-if="formatLlmError(criteriaStore.rulesError).cause" class="ai-error-card__cause">
          <strong>Cause:</strong> {{ formatLlmError(criteriaStore.rulesError).cause }}
        </p>
        <p v-if="formatLlmError(criteriaStore.rulesError).solution" class="ai-error-card__solution">
          <strong>Solution:</strong> {{ formatLlmError(criteriaStore.rulesError).solution }}
        </p>
        <details class="ai-error-card__details">
          <summary>Technical details</summary>
          <pre>{{ criteriaStore.rulesError }}</pre>
        </details>
      </div>
    </div>

    <!-- AI Critique: Check Rules -->
    <div v-if="criteriaStore.rulesCritique" class="ai-critique-card">
      <div class="ai-critique-card__header">
        <div class="ai-critique-card__title-group">
          <span class="material-symbols-outlined">auto_awesome</span>
          <span class="ai-critique-card__title">Rules Consistency Review</span>
        </div>
        <div class="ai-critique-card__header-actions">
          <button
            class="ai-critique-card__toggle"
            :title="criteriaStore.rulesCritiqueExpanded ? 'Collapse' : 'Expand'"
            @click="criteriaStore.rulesCritiqueExpanded = !criteriaStore.rulesCritiqueExpanded"
          >
            <span class="material-symbols-outlined">{{
              criteriaStore.rulesCritiqueExpanded ? 'expand_less' : 'expand_more'
            }}</span>
          </button>
          <button class="ai-critique-card__dismiss" @click="dismissRulesCritique">
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>
      </div>
      <div
        v-if="criteriaStore.rulesCritiqueExpanded"
        class="markdown-content ai-critique-card__body"
        v-html="renderCritiqueMarkdown(criteriaStore.rulesCritique)"
      />
    </div>
  </div>
</template>

<style scoped>
.criteria-editor {
  padding: var(--container-padding);
  max-width: 64rem;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

@media (max-width: 767px) {
  .criteria-editor {
    padding: var(--container-padding-sm);
    gap: 1.5rem;
  }
}

.section-panel {
  background-color: #ffffff;
  border-radius: 0.75rem;
  padding: 1.5rem;
  border: 1px solid #e2e8f0;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
}

@media (max-width: 767px) {
  .section-panel {
    padding: 1rem;
  }

  .section-panel__header {
    flex-wrap: wrap;
    gap: 0.5rem;
  }
}

.section-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 1.5rem;
}

.section-panel__title-group {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.section-panel__title {
  font-size: 20px;
  line-height: 28px;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: #1b1b24;
}

.section-panel__count {
  font-size: 12px;
  color: #94a3b8;
  font-family: ui-monospace, SFMono-Regular, monospace;
}

/* OpenAlex card (Tier 3) */
.openalex-card__desc {
  font-size: 14px;
  line-height: 22px;
  color: #475569;
  margin-bottom: 1rem;
}

.openalex-card__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}

/* Aim rows */
.aim-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.aim-row__number {
  width: 1.5rem;
  height: 1.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: #f1f5f9;
  color: #94a3b8;
  font-size: 12px;
  font-weight: 700;
  border-radius: 9999px;
  flex-shrink: 0;
}

.aim-row__text {
  flex: 1;
  font-size: 14px;
  line-height: 20px;
  color: #1b1b24;
}

/* Inline-edit affordances: read-only text shows a text caret on hover + a
 * tooltip hint; the editing <input> reuses the dashed-underline aesthetic of
 * the "add new" row so inline editing looks native to the section. */
.aim-row__text--editable {
  cursor: text;
  border-radius: 0.25rem;
  padding: 0.125rem 0.25rem;
  margin: -0.125rem -0.25rem;
  transition: background-color 0.15s;
}

.aim-row__text--editable:hover {
  background-color: #f8fafc;
}

/* Shared inline-edit textarea styles - one consistent multi-line editor
 * across Research Aims, Inclusion, and Exclusion sections. Mirrors the
 * focused-border + soft-ring aesthetic of the former criterion-card input,
 * extended to a 2-row auto-growing textarea so multi-line text is easy to
 * edit. A small hint line under the box matches the "add new" row pattern. */
.inline-edit-wrap {
  flex: 1;
  min-width: 0;
}

.inline-edit-textarea {
  width: 100%;
  background: #ffffff;
  border: 1px solid #4f46e5;
  border-radius: 0.375rem;
  padding: 0.375rem 0.5rem;
  font-size: 14px;
  line-height: 20px;
  color: #1b1b24;
  font-family: inherit;
  resize: none;
  outline: none;
  box-shadow: 0 0 0 2px rgba(79, 70, 229, 0.15);
  /* Min 2 rows, max ~6 rows (auto-grow via autoResizeTextarea). */
  min-height: 48px;
  max-height: 144px;
  overflow-y: auto;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
}

.inline-edit-textarea:focus {
  border-color: #3525cd;
  box-shadow: 0 0 0 3px rgba(79, 70, 229, 0.2);
}

.inline-edit-textarea--saving {
  opacity: 0.6;
  pointer-events: none;
}

.inline-edit-hint {
  font-size: 11px;
  color: #94a3b8;
  margin: 0.25rem 0 0 0;
}

.aim-row__input {
  flex: 1;
  background: transparent;
  border: none;
  border-bottom: 1px dashed #e2e8f0;
  padding: 0.5rem 0;
  font-size: 14px;
  line-height: 20px;
  color: #94a3b8;
  font-style: italic;
  outline: none;
  transition: border-color 0.15s;
}

.aim-row__input:focus {
  border-bottom-color: #4f46e5;
  color: #1b1b24;
  font-style: normal;
}

.aim-row__delete {
  opacity: 0;
  transition: opacity 0.15s;
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
}

.aim-row__delete:hover {
  color: #ba1a1a;
  background-color: #fef2f2;
}

.aim-row:hover .aim-row__delete,
.group:hover .aim-row__delete {
  opacity: 1;
}

/* Criterion add-row (mirrors the Research Aims dashed-input pattern, extended
 * with a priority dropdown + Add button on the left of the input). Sits at the
 * END of the criteria list, with a number prefix matching the existing cards. */
.criterion-add-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 0.5rem;
}

.criterion-add-row__priority {
  width: auto;
  flex-shrink: 0;
}

.criterion-add-row__add-btn {
  flex-shrink: 0;
}

.criterion-add-row__input {
  flex: 1;
  background: transparent;
  border: none;
  border-bottom: 1px dashed #e2e8f0;
  padding: 0.5rem 0;
  font-size: 14px;
  line-height: 20px;
  color: #94a3b8;
  font-style: italic;
  outline: none;
  transition: border-color 0.15s;
}

.criterion-add-row__input::placeholder {
  color: #94a3b8;
  font-style: italic;
}

.criterion-add-row__input:focus {
  border-bottom-color: #4f46e5;
  color: #1b1b24;
  font-style: normal;
}

.criterion-add-row__hint {
  font-size: 11px;
  color: #94a3b8;
  margin: 0.25rem 0 0 0;
  padding-left: 2.75rem;
}

@media (max-width: 767px) {
  .criterion-add-row {
    flex-wrap: wrap;
  }

  .criterion-add-row__input {
    flex: 1 1 100%;
    order: 99;
  }
}

/* Priority select */
.priority-select {
  font-size: 12px;
  background-color: #ffffff;
  border: 1px solid #e2e8f0;
  border-radius: 0.25rem;
  padding: 0.25rem 0.5rem;
  outline: none;
  cursor: pointer;
}

.priority-select:focus {
  border-color: #3525cd;
}

/* Criterion card */
.criterion-card {
  display: flex;
  align-items: flex-start;
  gap: 1rem;
  padding: 1rem;
  border-radius: 0 0.5rem 0.5rem 0;
}

@media (max-width: 767px) {
  .criterion-card {
    flex-direction: column;
    gap: 0.75rem;
  }

  .criterion-card__actions {
    flex-direction: row;
    align-items: center;
    width: 100%;
  }
}

.criterion-card__index {
  width: 1.5rem;
  height: 1.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 700;
  border-radius: 0.375rem;
  flex-shrink: 0;
  margin-top: 0.125rem;
}

.criterion-card__index--inc {
  background-color: #ecfdf5;
  color: #059669;
}

.criterion-card__index--exc {
  background-color: #fff1f2;
  color: #e11d48;
}

.criterion-card__label {
  font-size: 10px;
  text-transform: uppercase;
  font-weight: 700;
  letter-spacing: 0.05em;
  margin-bottom: 0.25rem;
  display: block;
}

.criterion-card__text {
  font-size: 14px;
  line-height: 20px;
  color: #1b1b24;
}

.criterion-card__text--editable {
  cursor: text;
  border-radius: 0.25rem;
  padding: 0.125rem 0.25rem;
  margin: -0.125rem -0.25rem;
  transition: background-color 0.15s;
}

.criterion-card__text--editable:hover {
  background-color: #f8fafc;
}

.criterion-card__actions {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.5rem;
}

.criterion-card__delete {
  opacity: 0;
  transition: opacity 0.15s;
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
}

.criterion-card__delete:hover {
  color: #ba1a1a;
  background-color: #fef2f2;
}

.criterion-card.group:hover .criterion-card__delete {
  opacity: 1;
}

/* Buttons */
.add-btn {
  color: #3525cd;
  font-weight: 600;
  font-size: 14px;
  display: flex;
  align-items: center;
  gap: 0.25rem;
  background: none;
  border: none;
  cursor: pointer;
  transition: color 0.15s;
}

.add-btn:hover {
  color: #4f46e5;
}

.add-btn__icon {
  font-size: 16px;
}

.btn-primary-sm {
  background-color: #3525cd;
  color: #ffffff;
  font-size: 12px;
  font-weight: 600;
  padding: 0.375rem 0.75rem;
  border-radius: 0.375rem;
  border: none;
  cursor: pointer;
  transition: background-color 0.15s;
}

.btn-primary-sm:hover {
  background-color: #4f46e5;
}

/* AI button - matches Tags & Labels pattern */
.ai-btn {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 1rem;
  background-color: #e8def8;
  color: #4a1564;
  font-size: 14px;
  font-weight: 500;
  border: 1px solid #c8aee6;
  border-radius: 0.5rem;
  cursor: pointer;
  white-space: nowrap;
  transition:
    background-color 0.15s,
    opacity 0.15s;
}

.ai-btn:hover:not(:disabled) {
  background-color: #d8c8f0;
}

.ai-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ai-btn .material-symbols-outlined {
  font-size: 18px;
}

/* AI loading indicator */
.ai-loading {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 1rem;
  background-color: #f3e8ff;
  color: #7c3aed;
  font-size: 14px;
  font-weight: 500;
  border: 1px solid #ddd6fe;
  border-radius: 0.5rem;
  white-space: nowrap;
  animation: pulse-subtle 1.5s ease-in-out infinite;
}

.ai-loading .material-symbols-outlined {
  font-size: 18px;
}

@keyframes pulse-subtle {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.7;
  }
}

/* AI Critique card */
.ai-critique-card {
  background-color: #f5f0ff;
  border: 1px solid #d8c8f0;
  border-radius: 0.75rem;
  padding: 1rem 1.25rem;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
}

.ai-critique-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}

/* Collapse/expand + close actions wrapper for the critique card header
 * (same shape as the search-strategy-card header actions). */
.ai-critique-card__header-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

/* Collapse/expand chevron toggle for the critique cards. Values mirror
 * `.search-strategy-card__toggle` so the two card families stay visually
 * consistent. */
.ai-critique-card__toggle {
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
  display: flex;
  align-items: center;
  transition:
    color 0.15s,
    background-color 0.15s;
}

.ai-critique-card__toggle:hover {
  color: #6b21a8;
  background-color: #ede9fe;
}

.ai-critique-card__toggle .material-symbols-outlined {
  font-size: 20px;
}

.ai-critique-card__title-group {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: #6b21a8;
}

.ai-critique-card__title-group .material-symbols-outlined {
  font-size: 20px;
}

.ai-critique-card__title {
  font-size: 14px;
  font-weight: 600;
}

.ai-critique-card__dismiss {
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
  transition:
    color 0.15s,
    background-color 0.15s;
}

.ai-critique-card__dismiss:hover {
  color: #ba1a1a;
  background-color: #fef2f2;
}

.ai-critique-card__dismiss .material-symbols-outlined {
  font-size: 18px;
}

.ai-critique-card__body {
  font-size: 14px;
  line-height: 22px;
  color: #1b1b24;
}

.ai-critique-card__body :deep(p) {
  margin-bottom: 0.75rem;
}

.ai-critique-card__body :deep(p:last-child) {
  margin-bottom: 0;
}

/* AI Error card */
.ai-error-card {
  background-color: #fef2f2;
  border: 1px solid #fca5a5;
  border-left: 4px solid #dc2626;
  border-radius: 0.75rem;
  padding: 1rem 1.25rem;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
}

.ai-error-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}

.ai-error-card__title-group {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: #991b1b;
}

.ai-error-card__title-group .material-symbols-outlined {
  font-size: 20px;
  color: #dc2626;
}

.ai-error-card__title {
  font-size: 14px;
  font-weight: 600;
}

.ai-error-card__dismiss {
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
  transition:
    color 0.15s,
    background-color 0.15s;
}

.ai-error-card__dismiss:hover {
  color: #ba1a1a;
  background-color: #fef2f2;
}

.ai-error-card__dismiss .material-symbols-outlined {
  font-size: 18px;
}

.ai-error-card__body {
  font-size: 14px;
  line-height: 22px;
  color: #1b1b24;
}

.ai-error-card__prefix {
  color: #7f1d1d;
  font-size: 13px;
  margin-bottom: 0.75rem;
}

.ai-error-card__cause {
  margin-bottom: 0.5rem;
  color: #374151;
}

.ai-error-card__solution {
  margin-bottom: 0.75rem;
  color: #374151;
}

.ai-error-card__details {
  margin-top: 0.75rem;
  margin-bottom: 0.5rem;
}

.ai-error-card__details summary {
  cursor: pointer;
  font-size: 12px;
  color: #6b7280;
  user-select: none;
}

.ai-error-card__details pre {
  margin-top: 0.5rem;
  padding: 0.75rem;
  background-color: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 0.375rem;
  font-size: 12px;
  line-height: 18px;
  color: #374151;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 10rem;
  overflow-y: auto;
}

.ai-error-card__help-link {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 13px;
  color: #3525cd;
  text-decoration: none;
  margin-top: 0.5rem;
}

.ai-error-card__help-link:hover {
  text-decoration: underline;
}

/* ── Section 4: Custom Screening Instructions ────────────────────────── */

.custom-logic {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.custom-logic__textarea {
  width: 100%;
  min-height: 120px;
  padding: 0.75rem;
  border: 1px solid #e2e8f0;
  border-radius: 0.5rem;
  background-color: #ffffff;
  font-family: inherit;
  font-size: 14px;
  line-height: 22px;
  color: #1b1b24;
  resize: vertical;
  outline: none;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
}

.custom-logic__textarea:focus {
  border-color: #4f46e5;
  box-shadow: 0 0 0 2px rgba(79, 70, 229, 0.15);
}

.custom-logic__textarea::placeholder {
  color: #94a3b8;
  font-style: italic;
}

.custom-logic__hint {
  font-size: 12px;
  color: #94a3b8;
  margin: 0;
}

/* Help popover (rich hover/focus tooltip for the question-mark icon) */
.help-popover {
  position: relative;
  display: inline-flex;
}

.help-popover__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 9999px;
  border: none;
  background-color: #f1f5f9;
  color: #64748b;
  cursor: help;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.help-popover__icon:hover,
.help-popover__icon:focus-visible {
  background-color: #e0e7ff;
  color: #4338ca;
}

.help-popover__icon .material-symbols-outlined {
  font-size: 18px;
}

.help-popover__panel {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  z-index: 30;
  width: 380px;
  max-width: calc(100vw - 2rem);
  background-color: #ffffff;
  border: 1px solid #e2e8f0;
  border-radius: 0.5rem;
  box-shadow:
    0 4px 6px -1px rgba(0, 0, 0, 0.1),
    0 2px 4px -2px rgba(0, 0, 0, 0.1);
  padding: 0.875rem 1rem;
  font-size: 12px;
  line-height: 18px;
  color: #475569;
}

.help-popover__panel code {
  background-color: #f1f5f9;
  color: #4338ca;
  padding: 0.0625rem 0.25rem;
  border-radius: 0.1875rem;
  font-size: 11px;
  font-family: ui-monospace, SFMono-Regular, monospace;
}

.help-popover__lead {
  margin: 0 0 0.5rem;
}

.help-popover__examples-label {
  margin: 0 0 0.25rem;
  font-weight: 600;
  color: #1b1b24;
}

.help-popover__examples {
  margin: 0 0 0.5rem;
  padding-left: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.help-popover__examples li {
  color: #64748b;
}

.help-popover__footer {
  margin: 0;
  color: #94a3b8;
  font-style: italic;
}

/* Popover enter/leave transition */
.help-popover-enter-active,
.help-popover-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}

.help-popover-enter-from,
.help-popover-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
