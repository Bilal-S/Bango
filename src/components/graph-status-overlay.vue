<template>
  <!-- Loading overlay -->
  <div
    v-if="loading || isLayouting"
    class="absolute inset-0 z-20 flex items-center justify-center bg-white/60 backdrop-blur-sm"
  >
    <div class="flex items-center gap-3 text-slate-600">
      <span class="material-symbols-outlined text-xl animate-spin">progress_activity</span>
      <span class="text-sm font-medium">{{ isLayouting ? layoutingLabel : loadingLabel }}</span>
    </div>
  </div>

  <!-- Error overlay -->
  <div v-else-if="error" class="absolute inset-0 z-20 flex items-center justify-center">
    <div class="text-center p-6 max-w-sm">
      <span class="material-symbols-outlined text-3xl text-red-400 mb-2 block">error</span>
      <p class="text-sm text-red-600">{{ error }}</p>
      <button
        class="mt-3 px-3 py-1.5 text-xs font-semibold text-white bg-indigo-600 hover:bg-indigo-700 rounded-lg cursor-pointer transition-colors"
        @click="$emit('retry')"
      >
        Retry
      </button>
    </div>
  </div>

  <!-- Empty state: simple single-line variant -->
  <div v-else-if="empty" class="absolute inset-0 z-20 flex items-center justify-center">
    <div v-if="emptyTitle" class="text-center text-slate-400 max-w-sm">
      <span class="material-symbols-outlined text-4xl mb-2 block">{{ emptyIcon }}</span>
      <p class="text-sm font-medium text-slate-500 mb-1">{{ emptyTitle }}</p>
      <p class="text-xs text-slate-400 leading-relaxed">{{ emptyHint }}</p>
    </div>
    <div v-else class="text-center text-slate-400">
      <span class="material-symbols-outlined text-4xl mb-2 block">{{ emptyIcon }}</span>
      <p class="text-sm">{{ emptyText }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * Shared loading / error / empty overlay for the four bibliometric graph
 * components. Renders exactly one of the three states (mirrors the historical
 * `v-if` / `v-else-if` chain). The hover tooltip stays in each domain
 * component because its content differs per network.
 */
withDefaults(
  defineProps<{
    loading: boolean;
    isLayouting: boolean;
    error: string | null;
    /** Whether the empty state should render (pass `!hasGraph`). */
    empty: boolean;
    /** Text while `loading` is true, e.g. `Loading citation network...`. */
    loadingLabel: string;
    layoutingLabel?: string;
    /** Material symbol name for the empty state. */
    emptyIcon: string;
    /** Simple one-line empty message (used when `emptyTitle` is absent). */
    emptyText?: string;
    /** Rich empty-state title; presence switches to the title + hint layout. */
    emptyTitle?: string;
    /** Rich empty-state hint paragraph below the title. */
    emptyHint?: string;
  }>(),
  {
    layoutingLabel: 'Computing layout…',
    emptyText: undefined,
    emptyTitle: undefined,
    emptyHint: undefined,
  }
);

defineEmits<{ (e: 'retry'): void }>();
</script>
