<script setup lang="ts">
defineProps<{
  selectedCount: number;
}>();

defineEmits<{
  bulkInclude: [];
  bulkReject: [];
  bulkMoveToWorking: [];
  bulkAddTag: [];
  bulkAddLabel: [];
  bulkAddToChat: [];
  bulkExport: [];
  clearSelection: [];
}>();
</script>

<template>
  <div
    v-if="selectedCount > 0"
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
    <button
      class="px-3 py-1.5 text-xs font-semibold rounded-lg bg-teal-600 hover:bg-teal-700 transition-colors inline-flex items-center gap-1"
      title="Export selected articles to RIS"
      @click="$emit('bulkExport')"
    >
      <span class="material-symbols-outlined text-[16px]">download</span>
      Export
    </button>
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
