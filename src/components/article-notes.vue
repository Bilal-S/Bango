<script setup lang="ts">
import { ref, watch, nextTick } from 'vue';
import type { Article } from '@/types';

const props = defineProps<{
  article: Article;
}>();

const emit = defineEmits<{
  updateNotes: [id: string, notes: string];
}>();

// Imported Notes expand/collapse state (persisted, collapsed by default)
const importedNotesExpanded = ref(localStorage.getItem('bango-imported-notes-expanded') === 'true');
function toggleImportedNotes(): void {
  importedNotesExpanded.value = !importedNotesExpanded.value;
  localStorage.setItem('bango-imported-notes-expanded', String(importedNotesExpanded.value));
}

// User notes editing
const editingNotes = ref(false);
const noteDraft = ref('');
const noteTextareaRef = ref<HTMLTextAreaElement | null>(null);

watch(editingNotes, (val) => {
  if (val) noteDraft.value = props.article.userNotes ?? '';
});

/**
 * Open the note editor from a double-click on the display/placeholder text.
 * Clears any browser word-selection caused by the dblclick gesture and places
 * the textarea caret at the end of the existing note text (no selection).
 */
function startEditingNotes(): void {
  window.getSelection()?.removeAllRanges();
  editingNotes.value = true;
  void nextTick(() => {
    const el = noteTextareaRef.value;
    if (!el) return;
    el.focus();
    const end = el.value.length;
    el.setSelectionRange(end, end);
  });
}

function saveNotes(): void {
  emit('updateNotes', props.article.id, noteDraft.value);
  editingNotes.value = false;
}

function cancelNotes(): void {
  editingNotes.value = false;
}
</script>

<template>
  <!-- Imported Notes (read-only, collapsible, shown only when present) -->
  <section v-if="article.notes">
    <button
      class="w-full flex items-center justify-between text-xs font-label-caps text-slate-500 uppercase tracking-wider hover:text-slate-700 cursor-pointer transition-colors py-1"
      @click="toggleImportedNotes"
    >
      <span>Imported Notes</span>
      <span
        class="material-symbols-outlined text-[16px] transition-transform duration-200 shrink-0"
        :class="{ 'rotate-180': importedNotesExpanded }"
      >
        expand_more
      </span>
    </button>
    <div v-show="importedNotesExpanded" class="mt-3">
      <p
        class="text-body-main font-body-main text-on-surface-variant leading-relaxed bg-amber-50 border border-amber-200 p-3 rounded-lg whitespace-pre-line"
      >
        {{ article.notes }}
      </p>
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
        ref="noteTextareaRef"
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
      class="text-body-main font-body-main text-on-surface-variant leading-relaxed bg-slate-50 p-3 rounded-lg cursor-pointer hover:bg-slate-100 transition-colors whitespace-pre-line"
      title="Double-click to edit"
      @dblclick.prevent="startEditingNotes"
    >
      {{ article.userNotes }}
    </p>
    <p
      v-else
      class="text-xs text-slate-400 italic cursor-pointer hover:text-slate-500 transition-colors select-none"
      title="Double-click to add notes"
      @dblclick.prevent="startEditingNotes"
    >
      No notes yet. Double-click to add.
    </p>
  </section>
</template>
