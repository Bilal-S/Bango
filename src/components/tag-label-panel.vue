<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import TagChip from '@/components/tag-chip.vue';
import LabelChip from '@/components/label-chip.vue';
import { formatArticleCount } from '@/utils/formatters';
import { getColorScheme } from '@/utils/color';
import type { TagWithCount, LabelWithCount } from '@/types';

/**
 * Shared panel for the Tags & Labels screen. Renders the header (with the
 * panel-level "Suggest with AI" action), the add-input row, and the chip
 * list with inline edit / color / filter / delete affordances.
 *
 * Extracted from `tag-label-management.vue` where the tag and label panels
 * were ~140 lines of near-identical markup. Both kinds are driven from one
 * implementation; `kind` selects the chip component, accent color, copy,
 * and tooltips.
 */

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
    suggestTooltip: 'Suggest labels from your articles using AI',
    accentText: 'text-secondary',
    accentFocus: 'focus:border-secondary focus:ring-secondary',
    accentBorder: 'border-secondary',
    accentHover: 'hover:text-secondary',
  };
});

// ── Add input ──────────────────────────────────────────────────────────
const newName = ref('');

function commitCreate(): void {
  const name = newName.value.trim();
  if (!name) return;
  emit('create', name);
  newName.value = '';
}

// ── Inline edit ────────────────────────────────────────────────────────
// At most one item is edited at a time. We query the edit input by class
// within the panel root rather than using a `ref` inside `v-for` (which
// Vue 3 collects into an array, breaking `.focus()`).
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
</script>

<template>
  <section
    ref="rootEl"
    class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm overflow-hidden flex flex-col min-h-[400px] lg:h-[700px]"
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

    <!-- Chip list -->
    <div class="p-4 lg:p-5 overflow-y-auto flex-1 space-y-3">
      <div
        v-for="item in items"
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
              <TagChip v-if="kind === 'tag'" :name="item.name" :color="item.color" />
              <LabelChip v-else :name="item.name" :color="item.color" />
            </span>
          </template>
        </div>
        <div class="flex items-center gap-4 flex-shrink-0">
          <span class="font-body-sm text-body-sm text-on-surface-variant">{{
            formatArticleCount(item.articleCount)
          }}</span>
          <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
            <button
              class="p-1 text-outline rounded hover:bg-surface-variant transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
              :class="config.accentHover"
              :disabled="item.articleCount === 0"
              :title="item.articleCount > 0 ? 'see assigned' : 'not assigned'"
              @click="emit('filter', item.id)"
            >
              <span class="material-symbols-outlined text-[16px]">filter_arrow_right</span>
            </button>
            <label
              class="relative cursor-pointer p-1 rounded hover:bg-surface-variant transition-colors"
              :style="{ color: item.color || getColorScheme(item.name, null).base }"
              :title="`Set ${config.noun} color`"
            >
              <span class="material-symbols-outlined text-[16px]">palette</span>
              <input
                type="color"
                class="absolute inset-0 opacity-0 cursor-pointer"
                :value="item.color || getColorScheme(item.name, null).base"
                :aria-label="`Set ${config.noun} color`"
                @input="onColorChange(item.id, $event)"
              />
            </label>
            <template v-if="editingId === item.id">
              <button
                class="p-1 rounded transition-colors"
                :class="config.accentText + ' hover:bg-surface-variant'"
                :title="`Save ${config.noun}`"
                :aria-label="`Save ${config.noun}`"
                @click="commitEdit"
              >
                <span class="material-symbols-outlined text-[16px]">check</span>
              </button>
              <button
                class="p-1 text-outline hover:bg-surface-variant rounded transition-colors"
                :title="`Cancel edit`"
                aria-label="Cancel edit"
                @click="cancelEdit"
              >
                <span class="material-symbols-outlined text-[16px]">close</span>
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
                <span class="material-symbols-outlined text-[16px]">edit</span>
              </button>
              <button
                class="p-1 text-outline hover:text-error rounded hover:bg-error-container transition-colors"
                :title="`Delete ${config.noun}`"
                :aria-label="`Delete ${config.noun}`"
                @click="emit('delete', item.id)"
              >
                <span class="material-symbols-outlined text-[16px]">close</span>
              </button>
            </template>
          </div>
        </div>
      </div>
      <p
        v-if="items.length === 0"
        class="text-on-surface-variant font-body-sm text-body-sm text-center py-8"
      >
        {{ config.empty }}
      </p>
    </div>
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
</style>
