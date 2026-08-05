<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import TagChip from '@/components/tag-chip.vue';
import LabelChip from '@/components/label-chip.vue';
import ClearableInput from '@/components/clearable-input.vue';
import { getColorScheme } from '@/utils/color';
import { useTagLabelFilter, type FilterableItem } from '@/composables/use-tag-label-filter';
import type { TagWithCount, LabelWithCount } from '@/types';

/** Shared panel for Tags & Labels management. Renders header ("Suggest" action),
 *  add-input row, sticky filter/sort sub-bar, and chip list with inline edit /
 *  color / delete. Both tag and label panels driven from one implementation;
 *  `kind` selects chip component, accent, copy, tooltips.
 *
 *  Filter/sort bar: thin sticky sub-bar at chip area top with filter toggle,
 *  sort buttons (alpha/frequency), expandable filter input. Each panel owns
 *  its own state (not shared; not persisted). Default sort: alpha A-Z. */

type Kind = 'tag' | 'label';

const props = defineProps<{
  kind: Kind;
  items: TagWithCount[] | LabelWithCount[];
  suggesting?: boolean;
}>();

const emit = defineEmits<{
  create: [name: string];
  rename: [id: string, newName: string];
  delete: [id: string];
  updateColor: [id: string, color: string | null];
  filter: [id: string];
  suggest: [];
  mergeRequest: [payload: { id: string; name: string; articleCount: number }];
}>();

// ── Per-kind config ────────────────────────────────────────────────────
const config = computed(() => {
  if (props.kind === 'tag') {
    return {
      icon: 'sell',
      title: 'Tags',
      subtitle: 'Content labels for grouping related research.',
      noun: 'tag',
      placeholder: 'Add new tag...',
      addAria: 'Add tag',
      empty: 'No tags yet.',
      noMatch: 'No matching tags.',
      suggestTooltip: 'Suggest tags from your article corpus using AI',
      accentText: 'text-primary',
      accentFocus: 'focus:border-primary focus:ring-primary',
      accentBorder: 'border-primary',
      accentHover: 'hover:text-primary',
    };
  }
  return {
    icon: 'bookmark_manager',
    title: 'Labels',
    subtitle: 'Workflow markers indicating state or priority.',
    noun: 'label',
    placeholder: 'Add new label...',
    addAria: 'Add label',
    empty: 'No labels yet.',
    noMatch: 'No matching labels.',
    suggestTooltip: 'Suggest labels from your articles using AI',
    accentText: 'text-secondary',
    accentFocus: 'focus:border-secondary focus:ring-secondary',
    accentBorder: 'border-secondary',
    accentHover: 'hover:text-secondary',
  };
});

/* Filter + sort bar: each panel owns its own instance; state not persisted.
   `items` flows reactively so displayItems stays in sync with store mutations
   (create/rename/delete/merge/suggest). `FilterableItem` unifies the union
   type for the structural supertype both TagWithCount and LabelWithCount
   satisfy. Template only reads id/name/color/articleCount. */
const filter = useTagLabelFilter<FilterableItem>(() => props.items);

// ── Add input ──────────────────────────────────────────────────────────
const newName = ref('');

function commitCreate(): void {
  const name = newName.value.trim();
  if (!name) return;
  emit('create', name);
  newName.value = '';
}

/* Inline edit: one item at a time. Query input by class within panel root
   (not `ref` in `v-for`, which Vue 3 collects into an array breaking `.focus()`). */
const rootEl = ref<HTMLElement | null>(null);
const editingId = ref<string | null>(null);
const editingName = ref('');

function startEdit(id: string, currentName: string): void {
  editingId.value = id;
  editingName.value = currentName;
  void nextTick(() => {
    const el = rootEl.value?.querySelector<HTMLInputElement>('.tlp-edit-input');
    el?.focus();
    el?.select();
  });
}

function commitEdit(): void {
  if (!editingId.value) return;
  const name = editingName.value.trim();
  if (!name) {
    cancelEdit();
    return;
  }
  emit('rename', editingId.value, name);
  editingId.value = null;
  editingName.value = '';
}

function cancelEdit(): void {
  editingId.value = null;
  editingName.value = '';
}

// ── Color picker ───────────────────────────────────────────────────────
function onColorChange(id: string, event: Event): void {
  const input = event.target as HTMLInputElement;
  emit('updateColor', id, input.value);
}

/* Delete confirmation: dialog state owned here (mirrors article-detail-panel).
   Delete button sets `pendingDelete` instead of emitting immediately; on
   confirm, emit `delete` + clear state. Parent handlers stay unchanged. */
interface PendingDelete {
  id: string;
  name: string;
  articleCount: number;
}
const pendingDelete = ref<PendingDelete | null>(null);

function requestDelete(item: { id: string; name: string; articleCount: number }): void {
  // Skip the confirmation dialog when the tag/label has no articles - there's
  // nothing to warn about, so the delete is frictionless.
  if (item.articleCount === 0) {
    emit('delete', item.id);
    return;
  }
  pendingDelete.value = item;
}

function cancelDelete(): void {
  pendingDelete.value = null;
}

function confirmDelete(): void {
  if (!pendingDelete.value) return;
  const id = pendingDelete.value.id;
  pendingDelete.value = null;
  emit('delete', id);
}
</script>

<template>
  <section
    ref="rootEl"
    class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm overflow-hidden flex flex-col min-h-[400px]"
  >
    <!-- Panel header: title + count badge + Suggest action -->
    <div class="p-4 lg:p-5 border-b border-surface-variant bg-surface-bright flex-shrink-0">
      <div class="flex items-center justify-between gap-3 mb-4">
        <div class="min-w-0">
          <h2 class="panel-title text-on-surface flex items-center gap-2">
            <span class="material-symbols-outlined text-[22px]" :class="config.accentText">{{
              config.icon
            }}</span>
            {{ config.title }}
          </h2>
          <p class="font-body-sm text-body-sm text-on-surface-variant mt-0.5">
            {{ config.subtitle }}
          </p>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <span
            class="bg-surface-variant text-on-surface-variant px-2 py-0.5 rounded-full font-label-caps text-label-caps"
          >
            {{ items.length }} Total
          </span>
          <button
            class="ai-btn"
            :disabled="suggesting"
            :title="config.suggestTooltip"
            :aria-label="config.suggestTooltip"
            @click="emit('suggest')"
          >
            <span class="material-symbols-outlined" :class="{ 'animate-spin': suggesting }"
              >auto_awesome</span
            >
            <span v-if="suggesting">Suggesting…</span>
            <span v-else>Suggest with AI</span>
          </button>
        </div>
      </div>

      <!-- Add input row: input + explicit Add button -->
      <div class="flex gap-2">
        <div class="relative flex-1">
          <span
            class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-outline text-[18px]"
            >add</span
          >
          <input
            v-model="newName"
            class="w-full pl-9 pr-3 py-2 bg-surface-container-lowest border border-outline-variant rounded-lg font-body-main text-body-main text-on-surface transition-all"
            :class="config.accentFocus"
            :placeholder="config.placeholder"
            :aria-label="config.placeholder"
            type="text"
            @keyup.enter="commitCreate"
          />
        </div>
        <button
          class="btn-primary-sm"
          :disabled="!newName.trim()"
          :aria-label="config.addAria"
          @click="commitCreate"
        >
          Add
        </button>
      </div>
    </div>

    <!-- Chip list. `TransitionGroup` wraps the rows so a delete animates
         (shrink + fade) instead of vanishing instantly. Mirrors the AI-card
         transition in `article-detail-panel.vue`. The sticky filter/sort
         sub-bar (Option A) pins to the top of this scroll area. -->
    <div class="overflow-y-auto flex-1">
      <!-- Sticky filter/sort sub-bar. Collapsed by default to a single thin
           row (filter icon + two sort buttons + caret). Clicking the row or
           the caret expands a ClearableInput below it. `sticky top-0 z-10`
           keeps the bar visible while chips scroll under it; the surface
           background + bottom border prevent bleed-through. -->
      <div
        class="sticky top-0 z-10 bg-surface-bright border-b border-surface-variant px-4 lg:px-5 py-2"
      >
        <div class="flex items-center gap-2">
          <!-- Filter toggle (icon-only; the row itself is also clickable) -->
          <button
            type="button"
            class="p-1 rounded transition-colors hover:bg-surface-variant"
            :class="filter.filterOpen.value ? config.accentText : 'text-outline'"
            :title="`Filter ${config.noun}s`"
            :aria-label="`Filter ${config.noun}s`"
            :aria-expanded="filter.filterOpen.value"
            aria-controls="tlp-filter-row"
            @click="filter.toggleFilterOpen()"
          >
            <span class="material-symbols-outlined text-[20px]">filter_list</span>
          </button>

          <!-- Sort by alpha (A-Z). Active = accent; arrow_downward for asc,
               arrow_upward for desc. Clicking the active sort flips direction;
               clicking the inactive sort switches active + resets to asc. -->
          <button
            type="button"
            class="tlp-sort-btn flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[12px] font-medium transition-colors"
            :class="
              filter.sortMode.value === 'alpha'
                ? config.accentText + ' bg-surface-container'
                : 'text-outline hover:bg-surface-variant'
            "
            :title="
              filter.sortMode.value === 'alpha'
                ? filter.sortDir.value === 'asc'
                  ? 'Sorted A-Z. Click to reverse.'
                  : 'Sorted Z-A. Click to reverse.'
                : 'Sort A-Z'
            "
            :aria-pressed="filter.sortMode.value === 'alpha'"
            @click="filter.toggleSort('alpha')"
          >
            <span class="material-symbols-outlined text-[16px]">sort</span>
            <span>A-Z</span>
            <span
              v-if="filter.sortMode.value === 'alpha'"
              class="material-symbols-outlined text-[14px]"
              >{{ filter.sortDir.value === 'asc' ? 'arrow_downward' : 'arrow_upward' }}</span
            >
          </button>

          <!-- Sort by frequency (1-100). Same toggle semantics as alpha. -->
          <button
            type="button"
            class="tlp-sort-btn flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[12px] font-medium transition-colors"
            :class="
              filter.sortMode.value === 'frequency'
                ? config.accentText + ' bg-surface-container'
                : 'text-outline hover:bg-surface-variant'
            "
            :title="
              filter.sortMode.value === 'frequency'
                ? filter.sortDir.value === 'asc'
                  ? 'Sorted 1-100 (smallest first). Click to reverse.'
                  : 'Sorted 100-1 (largest first). Click to reverse.'
                : 'Sort by frequency (1-100)'
            "
            :aria-pressed="filter.sortMode.value === 'frequency'"
            @click="filter.toggleSort('frequency')"
          >
            <span class="material-symbols-outlined text-[16px]">sort</span>
            <span>1-100</span>
            <span
              v-if="filter.sortMode.value === 'frequency'"
              class="material-symbols-outlined text-[14px]"
              >{{ filter.sortDir.value === 'asc' ? 'arrow_downward' : 'arrow_upward' }}</span
            >
          </button>

          <div class="flex-1" />

          <!-- Collapsed-state summary + caret. The count surfaces ONLY here
               (next to the caret) so the expanded filter row stays clean. -->
          <span
            v-if="filter.isFiltering.value"
            class="text-[11px] text-on-surface-variant font-label-caps text-label-caps"
          >
            Showing {{ filter.shownCount.value }} of {{ filter.totalCount.value }}
          </span>
          <button
            type="button"
            class="p-1 rounded transition-colors hover:bg-surface-variant text-outline"
            :title="filter.filterOpen.value ? 'Collapse filter' : 'Expand filter'"
            :aria-label="filter.filterOpen.value ? 'Collapse filter' : 'Expand filter'"
            :aria-expanded="filter.filterOpen.value"
            aria-controls="tlp-filter-row"
            @click="filter.toggleFilterOpen()"
          >
            <span
              class="material-symbols-outlined text-[18px] transition-transform"
              :class="{ 'rotate-180': filter.filterOpen.value }"
              >expand_more</span
            >
          </button>
        </div>

        <!-- Expanded filter input row. Slides in under the bar when open.
             The count lives next to the caret (above), so this row holds
             only the ClearableInput. -->
        <div v-if="filter.filterOpen.value" id="tlp-filter-row" class="mt-2">
          <ClearableInput
            v-model="filter.query.value"
            :placeholder="`Filter ${config.noun}s...`"
            :input-class="
              'bg-surface-container-lowest border-outline-variant text-on-surface py-1.5 text-sm ' +
              config.accentFocus
            "
            @clear="filter.clearFilter()"
          />
        </div>
      </div>

      <!-- Chip rows. Note: the previous `p-4 lg:p-5` padding moved to this
           wrapper so the sticky bar spans the full width of the scroll area. -->
      <div class="p-4 lg:p-5">
        <TransitionGroup name="tlp-row" tag="div" class="space-y-3">
          <div
            v-for="item in filter.displayItems.value"
            :key="item.id"
            class="flex items-center justify-between group p-2 hover:bg-surface-container rounded-lg transition-colors"
          >
            <div class="flex items-center gap-3 flex-1 min-w-0">
              <template v-if="editingId === item.id">
                <input
                  v-model="editingName"
                  class="tlp-edit-input px-2 py-1 bg-surface-container-lowest border rounded-lg font-mono text-mono text-on-surface transition-all w-full min-w-0"
                  :class="config.accentBorder + ' focus:ring-1 ' + config.accentFocus"
                  @keyup.enter="commitEdit"
                  @keyup.escape="cancelEdit"
                  @blur="commitEdit"
                />
              </template>
              <template v-else>
                <span
                  class="cursor-pointer"
                  title="Double-click to edit"
                  @dblclick="startEdit(item.id, item.name)"
                >
                  <TagChip
                    v-if="kind === 'tag'"
                    :name="item.name"
                    :color="item.color"
                    :count="item.articleCount"
                  />
                  <LabelChip
                    v-else
                    :name="item.name"
                    :color="item.color"
                    :count="item.articleCount"
                  />
                </span>
              </template>
            </div>
            <div class="flex items-center flex-shrink-0">
              <div
                class="flex items-center gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity"
              >
                <template v-if="editingId === item.id">
                  <button
                    class="p-1 rounded transition-colors"
                    :class="config.accentText + ' hover:bg-surface-variant'"
                    :title="`Save ${config.noun}`"
                    :aria-label="`Save ${config.noun}`"
                    @click="commitEdit"
                  >
                    <span class="material-symbols-outlined text-[20px]">check</span>
                  </button>
                  <button
                    class="p-1 text-outline hover:bg-surface-variant rounded transition-colors"
                    :title="`Cancel edit`"
                    aria-label="Cancel edit"
                    @click="cancelEdit"
                  >
                    <span class="material-symbols-outlined text-[20px]">close</span>
                  </button>
                </template>
                <template v-else>
                  <button
                    class="p-1 text-outline rounded transition-colors"
                    :class="config.accentHover + ' hover:bg-surface-variant'"
                    :title="`Edit ${config.noun}`"
                    :aria-label="`Edit ${config.noun}`"
                    @click="startEdit(item.id, item.name)"
                  >
                    <span class="material-symbols-outlined text-[20px]">edit</span>
                  </button>
                  <button
                    class="p-1 text-outline rounded transition-colors"
                    :class="config.accentHover + ' hover:bg-surface-variant'"
                    :title="`Replace ${config.noun} with...`"
                    :aria-label="`Replace ${config.noun} with...`"
                    @click.stop="
                      emit('mergeRequest', {
                        id: item.id,
                        name: item.name,
                        articleCount: item.articleCount,
                      })
                    "
                  >
                    <span class="material-symbols-outlined text-[20px]">cell_merge</span>
                  </button>
                  <button
                    class="p-1 text-outline rounded hover:bg-surface-variant transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                    :class="config.accentHover"
                    :disabled="item.articleCount === 0"
                    :title="item.articleCount > 0 ? 'see assigned' : 'not assigned'"
                    @click="emit('filter', item.id)"
                  >
                    <span class="material-symbols-outlined text-[20px]">filter_arrow_right</span>
                  </button>
                  <label
                    class="relative cursor-pointer p-1 rounded hover:bg-surface-variant transition-colors"
                    :style="{ color: item.color || getColorScheme(item.name, null).base }"
                    :title="`Set ${config.noun} color`"
                  >
                    <span class="material-symbols-outlined text-[20px]">palette</span>
                    <!-- `w-full h-full` is required: <input type="color"> is a
                         replaced element with an intrinsic native width (~44px
                         in Chromium). With only `inset-0` + `width: auto`, the
                         intrinsic size wins (CSS 2.1 §10.3.8) and the overlay
                         overflows ~16px to the right, silently covering the
                         left portion of the adjacent delete button. Explicit
                         width/height 100% constrains the overlay to the label
                         so the delete icon stays fully clickable. -->
                    <input
                      type="color"
                      class="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
                      :value="item.color || getColorScheme(item.name, null).base"
                      :aria-label="`Set ${config.noun} color`"
                      @input="onColorChange(item.id, $event)"
                    />
                  </label>
                  <button
                    class="p-1 text-outline hover:text-error rounded hover:bg-error-container transition-colors"
                    :title="`Delete ${config.noun}`"
                    :aria-label="`Delete ${config.noun}`"
                    @click="
                      requestDelete({
                        id: item.id,
                        name: item.name,
                        articleCount: item.articleCount,
                      })
                    "
                  >
                    <span class="material-symbols-outlined text-[20px]">delete</span>
                  </button>
                </template>
              </div>
            </div>
          </div>
        </TransitionGroup>
        <!-- Two empty states: (1) the taxonomy is genuinely empty
           (items.length === 0) -> "No tags yet." / "No labels yet.";
           (2) items exist but the filter matched none -> "No matching tags."
           / "No matching labels." (only when the filter is actively narrowing). -->
        <p
          v-if="items.length === 0"
          class="text-on-surface-variant font-body-sm text-body-sm text-center py-8"
        >
          {{ config.empty }}
        </p>
        <p
          v-else-if="filter.isFiltering.value && filter.shownCount.value === 0"
          class="text-on-surface-variant font-body-sm text-body-sm text-center py-8"
        >
          {{ config.noMatch }}
        </p>
      </div>
    </div>

    <!-- Delete confirmation dialog (Teleported to body, mirrors
         article-detail-panel.vue's delete-article pattern). Owned here so
         the parent's `onDeleteTag`/`onDeleteLabel` handlers stay unchanged. -->
    <Teleport to="body">
      <div v-if="pendingDelete" class="dialog-overlay" @click.self="cancelDelete">
        <div class="dialog">
          <button class="dialog__close" aria-label="Close" @click="cancelDelete">
            <span class="material-symbols-outlined">close</span>
          </button>
          <h2>Delete {{ config.noun }}</h2>
          <div class="dialog__danger-box">
            <span class="material-symbols-outlined">warning</span>
            <p>
              This will <strong>permanently delete</strong> the {{ config.noun }}
              <code>{{ pendingDelete.name }}</code>
              <span v-if="pendingDelete.articleCount > 0">
                and remove it from
                <strong>{{ pendingDelete.articleCount }}</strong> article(s)</span
              >. The {{ config.noun }} is removed from your taxonomy; the articles themselves are
              not deleted. This action <strong>cannot be undone</strong>.
            </p>
          </div>
          <div class="dialog__actions">
            <button class="btn btn--outline" @click="cancelDelete">Cancel</button>
            <button class="btn btn--danger" @click="confirmDelete">Delete {{ config.noun }}</button>
          </div>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<style scoped>
/* Section title uses --font-size-h1 (20px) rather than the default h2
   token (16px), so it reads as a clear panel heading while still sitting
   below the 24px page title (--font-size-display). Stays on the <h2>
   element for correct document semantics; only the visual size changes. */
.panel-title {
  font-size: var(--font-size-h1, 20px);
  font-weight: var(--font-weight-semibold, 600);
  line-height: var(--line-height-h1, 28px);
  letter-spacing: var(--letter-spacing-h1, -0.01em);
  margin: 0;
}

/* Row leave transition: shrink + fade out over 250ms. Mirrors the AI-card
   transition in `article-detail-panel.vue`. `max-height` (with a generous
   cap) collapses the row, `opacity` fades it, and `margin` collapses the
   `space-y-3` gap so siblings slide together smoothly. The `!important`
   on `margin` overrides the parent `.space-y-3 > * + *` rule. */
.tlp-row-leave-active {
  transition:
    max-height 0.25s ease-in,
    opacity 0.25s ease-in,
    margin 0.25s ease-in,
    padding 0.25s ease-in;
  overflow: hidden;
}
.tlp-row-leave-from {
  max-height: 80px;
  opacity: 1;
}
.tlp-row-leave-to {
  max-height: 0;
  opacity: 0;
  margin-top: 0 !important;
  margin-bottom: 0 !important;
  padding-top: 0;
  padding-bottom: 0;
}
</style>
