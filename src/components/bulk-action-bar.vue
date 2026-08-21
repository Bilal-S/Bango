<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';

withDefaults(
  defineProps<{
    selectedCount: number;
    /** Canonical LLM-configured gate, passed down from the host view (the bar
     * never derives LLM state itself - see `src/AGENTS.md` Local Contracts).
     * Gates the AI Summary submenu item. */
    llmReady?: boolean;
  }>(),
  { llmReady: true }
);

const emit = defineEmits<{
  bulkInclude: [];
  bulkReject: [];
  bulkMoveToWorking: [];
  bulkAddTag: [];
  bulkAddLabel: [];
  bulkAddToChat: [];
  bulkExport: [];
  bulkAiSummary: [];
  clearSelection: [];
}>();

/* ── More (...) submenu ──────────────────────────────────────────────────
 * The vertical three-dot button anchors a submenu that slides open above the
 * bar and holds the overflow actions (Export, AI Summary). The bar owns the
 * open state (same pattern as `network-export-menu.vue`); it closes on item
 * pick, anchor re-click, outside click, and Escape. */
const moreOpen = ref(false);
const rootRef = ref<HTMLElement | null>(null);

function toggleMore(): void {
  moreOpen.value = !moreOpen.value;
}

function closeMore(): void {
  moreOpen.value = false;
}

/** Item picked: close the submenu, then hand the action to the host view. */
function pickMoreAction(action: 'bulkExport' | 'bulkAiSummary'): void {
  closeMore();
  if (action === 'bulkExport') {
    emit('bulkExport');
  } else {
    emit('bulkAiSummary');
  }
}

/** Outside click closes the submenu (suggest-input.vue pattern). */
function handleOutsideClick(event: MouseEvent): void {
  if (rootRef.value && !rootRef.value.contains(event.target as Node)) {
    closeMore();
  }
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') closeMore();
}

onMounted(() => {
  document.addEventListener('click', handleOutsideClick);
  document.addEventListener('keydown', handleKeydown);
});

onUnmounted(() => {
  document.removeEventListener('click', handleOutsideClick);
  document.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <div
    v-if="selectedCount > 0"
    ref="rootRef"
    class="bulk-bar sticky bottom-6 z-50 mx-auto w-max flex flex-wrap items-center justify-center gap-2 max-w-full bg-[var(--color-sidebar)] text-[var(--color-sidebar-text)] rounded-xl shadow-lg px-4 py-3"
  >
    <span class="text-sm font-medium whitespace-nowrap"> {{ selectedCount }} selected </span>
    <div class="w-px h-5 bg-[var(--color-sidebar-hover)]" />
    <button
      class="px-3 py-1.5 text-xs font-semibold rounded-lg bg-emerald-600 hover:bg-emerald-700 transition-colors"
      @click="$emit('bulkInclude')"
    >
      Include
    </button>
    <button
      class="px-3 py-1.5 text-xs font-semibold rounded-lg bg-red-600 hover:bg-red-700 transition-colors"
      @click="$emit('bulkReject')"
    >
      Reject
    </button>
    <button
      class="px-3 py-1.5 text-xs font-semibold rounded-lg bg-amber-600 hover:bg-amber-700 transition-colors"
      @click="$emit('bulkMoveToWorking')"
    >
      Working
    </button>
    <button
      class="px-3 py-1.5 text-xs font-semibold rounded-lg bg-indigo-600 hover:bg-indigo-700 transition-colors"
      @click="$emit('bulkAddTag')"
    >
      Change Tag
    </button>
    <button
      class="px-3 py-1.5 text-xs font-semibold rounded-lg bg-purple-600 hover:bg-purple-700 transition-colors"
      @click="$emit('bulkAddLabel')"
    >
      Change Label
    </button>
    <button
      class="px-3 py-1.5 text-xs font-semibold rounded-lg bg-sky-600 hover:bg-sky-700 transition-colors"
      @click="$emit('bulkAddToChat')"
    >
      Add to Chat
    </button>
    <!-- More actions: vertical three-dot anchor. The submenu slides open above
         the bar and holds the overflow actions (Export, AI Summary). -->
    <div class="relative">
      <button
        class="p-1.5 rounded-lg hover:bg-[var(--color-sidebar-hover)] transition-colors"
        title="More actions"
        aria-label="More actions"
        aria-haspopup="menu"
        :aria-expanded="moreOpen"
        @click="toggleMore"
      >
        <span class="material-symbols-outlined text-[18px]">more_vert</span>
      </button>
      <Transition
        enter-active-class="transition duration-150 ease-out"
        enter-from-class="opacity-0 translate-y-1 scale-95"
        enter-to-class="opacity-100 translate-y-0 scale-100"
        leave-active-class="transition duration-100 ease-in"
        leave-from-class="opacity-100 translate-y-0 scale-100"
        leave-to-class="opacity-0 translate-y-1 scale-95"
      >
        <div
          v-if="moreOpen"
          role="menu"
          class="absolute bottom-full right-0 mb-2 w-44 origin-bottom-right rounded-lg bg-white border border-slate-200 shadow-xl overflow-hidden z-10"
        >
          <button
            role="menuitem"
            class="w-full flex items-center gap-2 px-3 py-2 text-xs font-semibold text-slate-700 text-left hover:bg-slate-100 transition-colors"
            title="Export selected articles to RIS"
            @click="pickMoreAction('bulkExport')"
          >
            <span class="material-symbols-outlined text-[16px]">download</span>
            Export
          </button>
          <button
            role="menuitem"
            class="w-full flex items-center gap-2 px-3 py-2 text-xs font-semibold text-slate-700 text-left hover:bg-slate-100 transition-colors disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-transparent"
            :disabled="!llmReady"
            :title="
              llmReady
                ? 'Generate AI summaries for selected articles that do not have one'
                : 'Configure an LLM provider in Settings to use AI Summary'
            "
            @click="pickMoreAction('bulkAiSummary')"
          >
            <span class="material-symbols-outlined text-[16px]">auto_awesome</span>
            AI Summary
          </button>
        </div>
      </Transition>
    </div>
    <div class="w-px h-5 bg-[var(--color-sidebar-hover)]" />
    <button
      class="text-slate-400 hover:text-white transition-colors"
      title="Clear selection"
      @click="$emit('clearSelection')"
    >
      <span class="material-symbols-outlined text-[18px]">close</span>
    </button>
  </div>
</template>
