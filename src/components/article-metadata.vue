<script setup lang="ts">
import { ref, nextTick, computed, watch } from 'vue';
import type { Article } from '@/types';
import { useToast } from '@/composables/use-toast';

const props = defineProps<{
  article: Article;
}>();

const emit = defineEmits<{
  updateField: [field: string, value: string | string[]];
}>();

const toast = useToast();

// Metadata expand/collapse state (persisted)
const metadataExpanded = ref(localStorage.getItem('bango-metadata-expanded') !== 'false');
function toggleMetadata(): void {
  metadataExpanded.value = !metadataExpanded.value;
  localStorage.setItem('bango-metadata-expanded', String(metadataExpanded.value));
}

function copyDoi(): void {
  if (!props.article.doi) return;
  navigator.clipboard.writeText(props.article.doi).then(() => {
    toast.show('DOI copied to clipboard', 'success', 2000);
  });
}

/**
 * Copy a normalized version of the DOI suitable for use as a filename.
 * Strips any `https://doi.org/` prefix, lowercases, and replaces characters
 * that are unsafe in filenames (`/`, spaces, etc.) with underscores. Matches
 * the batch-import `clean_doi_filename` convention so the copied value can be
 * used directly to name `{clean_doi}_references.ris` files.
 *
 * Example: `10.1108/MRR-12-2021-0866` → `10.1108_mrr-12-2021-0866`
 */
function copyCleanDoi(): void {
  if (!props.article.doi) return;
  const raw = props.article.doi.replace(/^https?:\/\/doi\.org\//i, '');
  const cleaned = raw
    .toLowerCase()
    .replace(/[^a-z0-9.-]/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_|_$/g, '');
  navigator.clipboard.writeText(cleaned).then(() => {
    toast.show(`Clean DOI copied: ${cleaned}`, 'success', 2000);
  });
}

// ── Inline edit ────────────────────────────────────────────────────────
// At most one field is edited at a time. We query the edit input by class
// within the section root rather than using a `ref` inside `v-for` / template
// branches (which Vue 3 collects into an array, breaking `.focus()`).
// Mirrors the proven pattern in `tag-label-panel.vue` (v6.9).
const rootEl = ref<HTMLElement | null>(null);
const editingField = ref<string | null>(null);
const editingValue = ref('');
/** Validation error message for the in-flight edit (currently only Year uses
 *  this; null = no error). When set, the input stays open and a red hint shows
 *  below it; the user must fix or Escape-cancel. */
const editError = ref<string | null>(null);
/** Tracks whether the language dropdown is in "Other…" free-text mode. */
const languageOtherMode = ref(false);

/** The 7 fields surfaced in this card. Authors/Keywords use array values. */
type MetaField =
  | 'authors'
  | 'affiliation'
  | 'journal'
  | 'publicationYear'
  | 'language'
  | 'doi'
  | 'keywords';

/**
 * Curated language list for the Lang dropdown. Covers the languages most
 * commonly seen in academic publishing + the languages the translation
 * pipeline supports. "Other…" lets the user type a custom value not in the
 * list. Stored values are free-form `Option<String>` on the backend so any
 * value works with `is_english_language` / `should_skip_translation`.
 */
const LANGUAGE_OPTIONS: readonly string[] = [
  'English',
  'French',
  'German',
  'Spanish',
  'Portuguese',
  'Italian',
  'Dutch',
  'Russian',
  'Chinese',
  'Japanese',
  'Korean',
  'Arabic',
  'Turkish',
  'Polish',
  'Swedish',
  'Norwegian',
  'Danish',
  'Finnish',
  'Czech',
  'Greek',
  'Hebrew',
  'Hindi',
  'Persian',
  'Ukrainian',
] as const;

/** Sentinel value for the "Other…" option in the language dropdown. */
const LANGUAGE_OTHER = '__other__';

/** Read the current DB value as a single editable string. */
function readField(field: MetaField): string {
  const a = props.article;
  switch (field) {
    case 'authors':
      return a.authors.join(', ');
    case 'keywords':
      return a.keywords.join(', ');
    case 'publicationYear':
      return a.publicationYear != null ? String(a.publicationYear) : '';
    case 'doi':
      return a.doi ?? '';
    case 'journal':
      return a.journal ?? '';
    case 'language':
      return a.language ?? '';
    case 'affiliation':
      return a.affiliation ?? '';
  }
}

/** Pure year validator: returns an error message or null when valid/empty. */
function validateYear(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed === '') return null; // empty clears the field (allowed)
  if (!/^\d{4}$/.test(trimmed)) return 'Year must be a 4-digit number';
  const n = Number(trimmed);
  if (!Number.isInteger(n) || n < 1800 || n > 2100) {
    return 'Year must be between 1800 and 2100';
  }
  return null;
}

function startEdit(field: MetaField): void {
  editingField.value = field;
  editingValue.value = readField(field);
  editError.value = null;
  // If the current language value is not in the curated list, start in
  // "Other…" mode so the user sees their custom value in the free-text input.
  languageOtherMode.value =
    field === 'language' &&
    editingValue.value !== '' &&
    !LANGUAGE_OPTIONS.includes(editingValue.value);
  void nextTick(() => {
    const el = rootEl.value?.querySelector<HTMLElement>('.meta-edit-input');
    el?.focus();
    // `.select()` only exists on HTMLInputElement, not HTMLSelectElement.
    if (el instanceof HTMLInputElement) el.select();
  });
}

/**
 * Commit the in-flight edit. Emits the typed value to the parent, which routes
 * it through the `update_article_metadata` IPC and re-fetches the article so
 * the chip flips live. Empty/whitespace-only values are forwarded so the
 * backend clears the field to NULL (or `[]` for array fields).
 *
 * For Year: validates first; on error, keeps the input open + shows a red hint
 * instead of committing.
 */
function commitEdit(): void {
  const raw = editingField.value;
  if (raw == null) return;
  const field = raw as MetaField;
  const trimmed = editingValue.value.trim();

  // Year validation gate: block commit on invalid input.
  if (field === 'publicationYear') {
    const err = validateYear(editingValue.value);
    if (err) {
      editError.value = err;
      // Re-focus so the user can fix immediately without re-double-clicking.
      void nextTick(() => {
        const el = rootEl.value?.querySelector<HTMLInputElement>('.meta-edit-input');
        el?.focus();
      });
      return;
    }
  }

  // Skip the IPC round-trip if nothing changed.
  if (trimmed === readField(field)) {
    editingField.value = null;
    editingValue.value = '';
    editError.value = null;
    languageOtherMode.value = false;
    return;
  }

  if (field === 'authors' || field === 'keywords') {
    const arr = trimmed
      ? trimmed
          .split(',')
          .map((s) => s.trim())
          .filter((s) => s.length > 0)
      : [];
    emit('updateField', field, arr);
  } else {
    emit('updateField', field, trimmed);
  }
  editingField.value = null;
  editingValue.value = '';
  editError.value = null;
  languageOtherMode.value = false;
}

function cancelEdit(): void {
  editingField.value = null;
  editingValue.value = '';
  editError.value = null;
  languageOtherMode.value = false;
}

/** Display helper: empty/null -> muted `---` placeholder. */
function displayValue(field: MetaField): string {
  const v = readField(field);
  return v === '' ? '---' : v;
}

/** Whether the current journal is set but NOT linked to the journal_index. */
const journalUnrecognized = computed(
  () =>
    !!props.article.journal && props.article.journal.trim() !== '' && !props.article.journalIndexId
);

/** Live year validation error for the input hint (null when valid/empty). */
const yearInputError = computed<string | null>(() => {
  if (editingField.value !== 'publicationYear') return null;
  return validateYear(editingValue.value);
});

/**
 * Handle the language `<select>` change. When the user picks "Other…", switch
 * to free-text mode (clear the sentinel + focus the text input). Otherwise
 * update `editingValue` so the normal commit-on-blur path persists the choice.
 */
function onLanguageSelectChange(value: string): void {
  if (value === LANGUAGE_OTHER) {
    languageOtherMode.value = true;
    editingValue.value = '';
    void nextTick(() => {
      const el = rootEl.value?.querySelector<HTMLInputElement>('.meta-edit-input');
      el?.focus();
    });
  } else {
    emit('updateField', 'language', value);
  }
}

// When the user switches back from "Other…" to the list via the ← button, the
// dropdown re-renders. No extra watcher needed — the template's
// `v-if="!languageOtherMode"` + the button handler reset `editingValue`.
// Keep `editError` clear when not editing Year (defensive cleanup).
watch(editingField, (f) => {
  if (f !== 'publicationYear') editError.value = null;
});

// When the article changes (user navigates away), close the language select
// so the display reverts to the text span.
watch(
  () => props.article.id,
  () => {
    if (editingField.value === 'language') cancelEdit();
  }
);
</script>

<template>
  <section ref="rootEl">
    <div class="border border-slate-200 rounded overflow-hidden">
      <button
        class="w-full flex items-center justify-between px-3 py-2 text-xs font-label-caps text-slate-500 uppercase tracking-wider hover:bg-slate-50 cursor-pointer transition-colors"
        @click="toggleMetadata"
      >
        <span class="flex items-center gap-1 min-w-0 overflow-hidden">
          <span class="shrink-0">Metadata</span>
          <span
            v-if="!metadataExpanded && article.authors.length > 0"
            class="text-[11px] text-slate-400 font-body-sm normal-case tracking-normal truncate"
          >
            – {{ article.authors.join(', ') }}
          </span>
        </span>
        <span
          class="material-symbols-outlined text-[16px] transition-transform duration-200 shrink-0"
          :class="{ 'rotate-180': metadataExpanded }"
        >
          expand_more
        </span>
      </button>
      <div v-show="metadataExpanded" class="px-3 pb-3 space-y-3">
        <!-- Authors (array, comma-separated edit) -->
        <div class="flex flex-col gap-1 text-body-sm font-body-sm">
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
            >Authors</span
          >
          <input
            v-if="editingField === 'authors'"
            v-model="editingValue"
            class="meta-edit-input px-2 py-1 bg-white border border-primary rounded font-body-sm text-on-surface transition-all focus:ring-1 focus:border-primary focus:ring-primary"
            placeholder="Author One, Author Two"
            @keyup.enter="commitEdit"
            @keyup.escape="cancelEdit"
            @blur="commitEdit"
          />
          <span
            v-else
            class="text-on-surface cursor-text hover:bg-slate-50 rounded px-1 -mx-1"
            :class="{ 'text-slate-300': article.authors.length === 0 }"
            title="Double-click to edit"
            @dblclick="startEdit('authors')"
            >{{ displayValue('authors') }}</span
          >
        </div>

        <!-- Affiliation -->
        <div class="flex flex-col gap-1 text-body-sm font-body-sm">
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
            >Affiliation</span
          >
          <input
            v-if="editingField === 'affiliation'"
            v-model="editingValue"
            class="meta-edit-input px-2 py-1 bg-white border border-primary rounded font-body-sm text-on-surface transition-all focus:ring-1 focus:border-primary focus:ring-primary"
            placeholder="Institution, Department"
            @keyup.enter="commitEdit"
            @keyup.escape="cancelEdit"
            @blur="commitEdit"
          />
          <span
            v-else
            class="text-on-surface cursor-text hover:bg-slate-50 rounded px-1 -mx-1 truncate"
            :class="{ 'text-slate-300': !article.affiliation }"
            title="Double-click to edit"
            @dblclick="startEdit('affiliation')"
            >{{ displayValue('affiliation') }}</span
          >
        </div>

        <div
          class="grid gap-4 text-body-sm font-body-sm"
          style="grid-template-columns: 2.5fr 1fr 1fr"
        >
          <!-- Journal (with "(unrecognized)" indicator when journalIndexId is null) -->
          <div class="flex flex-col gap-1 min-w-0">
            <span
              class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold flex items-center gap-1"
            >
              Journal
              <span
                v-if="journalUnrecognized"
                class="text-amber-600 normal-case tracking-normal font-medium"
                title="This journal is not in the local journal index. The entry is still accepted; run Settings → Rematch Journals later to retry."
              >
                (unrecognized)
              </span>
            </span>
            <input
              v-if="editingField === 'journal'"
              v-model="editingValue"
              class="meta-edit-input px-2 py-1 bg-white border border-primary rounded font-body-sm text-on-surface transition-all focus:ring-1 focus:border-primary focus:ring-primary"
              placeholder="Journal name"
              @keyup.enter="commitEdit"
              @keyup.escape="cancelEdit"
              @blur="commitEdit"
            />
            <span
              v-else
              class="text-on-surface cursor-text hover:bg-slate-50 rounded px-1 -mx-1 truncate"
              :class="{ 'text-slate-300': !article.journal }"
              :title="article.journal ?? ''"
              @dblclick="startEdit('journal')"
              >{{ displayValue('journal') }}</span
            >
          </div>

          <!-- Year (4-digit, 1800-2100) -->
          <div class="flex flex-col gap-1 min-w-0">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >Year</span
            >
            <input
              v-if="editingField === 'publicationYear'"
              v-model="editingValue"
              inputmode="numeric"
              maxlength="4"
              class="meta-edit-input px-2 py-1 bg-white border rounded font-body-sm text-on-surface transition-all focus:ring-1"
              :class="
                yearInputError
                  ? 'border-rose-400 focus:border-rose-500 focus:ring-rose-500'
                  : 'border-primary focus:border-primary focus:ring-primary'
              "
              placeholder="2024"
              @keyup.enter="commitEdit"
              @keyup.escape="cancelEdit"
              @blur="commitEdit"
            />
            <span
              v-if="editingField === 'publicationYear' && yearInputError"
              class="text-[10px] text-rose-600 leading-tight"
              >{{ yearInputError }}</span
            >
            <span
              v-else-if="editingField !== 'publicationYear'"
              class="text-on-surface cursor-text hover:bg-slate-50 rounded px-1 -mx-1"
              :class="{ 'text-slate-300': article.publicationYear == null }"
              title="Double-click to edit"
              @dblclick="startEdit('publicationYear')"
              >{{ displayValue('publicationYear') }}</span
            >
          </div>

          <!-- Language (dropdown + "Other…" free-text fallback) -->
          <div class="flex flex-col gap-1 min-w-0">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >Lang</span
            >
            <template v-if="editingField === 'language'">
              <!-- Dropdown: hidden when "Other…" free-text mode is active -->
              <select
                v-if="!languageOtherMode"
                v-model="editingValue"
                class="meta-edit-input px-2 py-1 bg-white border border-primary rounded font-body-sm text-on-surface transition-all focus:ring-1 focus:border-primary focus:ring-primary"
                @keyup.escape="cancelEdit"
                @change="onLanguageSelectChange(($event.target as HTMLSelectElement).value)"
              >
                <option value="">—</option>
                <option v-for="lang in LANGUAGE_OPTIONS" :key="lang" :value="lang">
                  {{ lang }}
                </option>
                <option :value="LANGUAGE_OTHER">Other…</option>
              </select>
              <!-- Free-text input for "Other…" -->
              <input
                v-if="languageOtherMode"
                v-model="editingValue"
                class="meta-edit-input px-2 py-1 bg-white border border-primary rounded font-body-sm text-on-surface transition-all focus:ring-1 focus:border-primary focus:ring-primary"
                placeholder="Custom language"
                @keyup.enter="commitEdit"
                @keyup.escape="cancelEdit"
                @blur="commitEdit"
              />
              <button
                v-if="languageOtherMode"
                type="button"
                class="text-[10px] text-primary hover:underline self-start mt-0.5"
                @click="
                  languageOtherMode = false;
                  editingValue = '';
                "
              >
                ← back to list
              </button>
            </template>
            <span
              v-else
              class="text-on-surface cursor-text hover:bg-slate-50 rounded px-1 -mx-1 truncate"
              :class="{ 'text-slate-300': !article.language }"
              :title="article.language ?? ''"
              @dblclick="startEdit('language')"
              >{{ displayValue('language') }}</span
            >
          </div>

          <!-- DOI (editable value; external link + copy button shown only when non-empty) -->
          <div class="flex flex-col gap-1 col-span-3">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >DOI</span
            >
            <div class="flex items-center gap-1 min-w-0">
              <input
                v-if="editingField === 'doi'"
                v-model="editingValue"
                class="meta-edit-input flex-1 px-2 py-1 bg-white border border-primary rounded font-body-sm text-on-surface transition-all focus:ring-1 focus:border-primary focus:ring-primary"
                placeholder="10.xxxx/xxxxx"
                @keyup.enter="commitEdit"
                @keyup.escape="cancelEdit"
                @blur="commitEdit"
              />
              <template v-else>
                <a
                  v-if="article.doi"
                  class="text-primary hover:underline cursor-text truncate"
                  :href="'https://doi.org/' + article.doi"
                  target="_blank"
                  rel="noopener noreferrer"
                  :title="article.doi"
                  >{{ article.doi }}</a
                >
                <span
                  class="text-slate-300 cursor-text hover:bg-slate-50 rounded px-1 -mx-1 truncate"
                  title="Double-click to edit"
                  @dblclick="startEdit('doi')"
                  >{{ article.doi ? '' : '---' }}</span
                >
                <!-- Edit button: the DOI renders as a link so double-click
                     navigates instead of editing. This explicit icon button
                     enters edit mode directly. -->
                <button
                  v-if="article.doi"
                  class="material-symbols-outlined text-[14px] text-slate-400 hover:text-primary cursor-pointer transition-colors shrink-0"
                  title="Edit DOI"
                  @click="startEdit('doi')"
                >
                  edit
                </button>
                <button
                  v-if="article.doi"
                  class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer transition-colors shrink-0"
                  title="Copy DOI"
                  @click="copyDoi"
                >
                  content_copy
                </button>
                <!-- Clean-copy button: copies a filename-safe normalized DOI
                     (e.g. `10.1108_mrr-12-2021-0866`) matching the batch-import
                     `clean_doi_filename` convention. -->
                <button
                  v-if="article.doi"
                  class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer transition-colors shrink-0"
                  title="Copy DOI as clean filename (e.g. 10.1108_mrr-12-2021-0866)"
                  @click="copyCleanDoi"
                >
                  cleaning_services
                </button>
              </template>
            </div>
          </div>

          <!-- Keywords (array, comma-separated edit) -->
          <div class="flex flex-col gap-1 col-span-3">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >Keywords</span
            >
            <input
              v-if="editingField === 'keywords'"
              v-model="editingValue"
              class="meta-edit-input px-2 py-1 bg-white border border-primary rounded font-body-sm text-on-surface transition-all focus:ring-1 focus:border-primary focus:ring-primary"
              placeholder="keyword one, keyword two"
              @keyup.enter="commitEdit"
              @keyup.escape="cancelEdit"
              @blur="commitEdit"
            />
            <span
              v-else
              class="text-on-surface cursor-text hover:bg-slate-50 rounded px-1 -mx-1"
              :class="{ 'text-slate-300': article.keywords.length === 0 }"
              title="Double-click to edit"
              @dblclick="startEdit('keywords')"
              >{{ displayValue('keywords') }}</span
            >
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
