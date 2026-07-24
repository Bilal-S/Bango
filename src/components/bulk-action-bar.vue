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
  clearSelection: [];
}>();
</script>

<template>
  <div
    v-if="selectedCount > 0"
    class="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 flex items-center gap-3 bg-[var(--color-sidebar)] text-[var(--color-sidebar-text)] rounded-xl shadow-lg px-5 py-3"
  >
    <span class="text-sm font-medium">
      {{ selectedCount }} article{{ selectedCount !== 1 ? 's' : '' }} selected
    </span>
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
      Move to Working
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
