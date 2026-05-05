<script setup lang="ts">
defineProps<{ name: string }>();
defineEmits<{ remove: [] }>();
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-slate-300 text-slate-700 bg-transparent font-mono text-mono"
  >
    <span class="w-1.5 h-1.5 rounded-full bg-slate-400"></span>
    {{ name }}
    <button
      class="inline-flex items-center justify-center w-4 h-4 rounded-full text-slate-400 hover:text-slate-700 hover:bg-slate-100 transition-colors cursor-pointer p-0 leading-1 text-xs"
      @click="$emit('remove')"
    >
      &times;
    </button>
  </span>
</template>
