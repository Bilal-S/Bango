<template>
  <div class="relative">
    <button
      class="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
      @click="showExportMenu = !showExportMenu"
    >
      <span class="material-symbols-outlined text-sm">download</span>
      Export
      <span class="material-symbols-outlined text-sm">expand_more</span>
    </button>
    <ul
      v-if="showExportMenu"
      class="absolute left-0 bottom-full mb-1 w-36 bg-white border border-slate-200 rounded-lg shadow-lg z-30 overflow-hidden"
    >
      <li
        class="px-3 py-2 text-xs text-slate-700 hover:bg-indigo-50 cursor-pointer flex items-center gap-2"
        @click="onExport('png')"
      >
        <span class="material-symbols-outlined text-sm">image</span>
        PNG Image
      </li>
      <li
        class="px-3 py-2 text-xs text-slate-700 hover:bg-indigo-50 cursor-pointer flex items-center gap-2"
        @click="onExport('gexf')"
      >
        <span class="material-symbols-outlined text-sm">share</span>
        GEXF Network
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import type { NetworkExportFormat } from '../utils/network-export';

/**
 * Shared Export dropdown (PNG / GEXF) for the bibliometric controls sidebars.
 * Owns its own open/close state; emits `select` with the chosen format.
 */
const emit = defineEmits<{ (e: 'select', format: NetworkExportFormat): void }>();

const showExportMenu = ref(false);

function onExport(format: NetworkExportFormat) {
  showExportMenu.value = false;
  emit('select', format);
}
</script>
