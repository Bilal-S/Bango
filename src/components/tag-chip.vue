<script setup lang="ts">
defineProps<{ name: string }>();
defineEmits<{ remove: [] }>();
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-blue-100 text-blue-800 font-mono text-mono border border-blue-200"
  >
    {{ name }}
    <button
      class="inline-flex items-center justify-center w-4 h-4 rounded-full text-blue-400 hover:text-blue-700 hover:bg-blue-200 transition-colors cursor-pointer p-0 leading-1 text-xs"
      @click="$emit('remove')"
    >
      &times;
    </button>
  </span>
</template>
