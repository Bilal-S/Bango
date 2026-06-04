<script setup lang="ts">
import { useToast } from '@/composables/use-toast';

const { toasts, dismiss } = useToast();

const bgClass = (type: string): string => {
  switch (type) {
    case 'success':
      return 'bg-green-500 text-white';
    case 'info':
      return 'bg-blue-500 text-white';
    case 'warning':
      return 'bg-amber-500 text-white';
    case 'error':
      return 'bg-red-500 text-white';
    default:
      return 'bg-slate-700 text-white';
  }
};
</script>

<template>
  <Teleport to="body">
    <div class="fixed top-4 right-4 z-[9999] flex flex-col gap-2 pointer-events-none">
      <TransitionGroup name="toast">
        <div
          v-for="toast in toasts"
          :key="toast.id"
          class="pointer-events-auto flex items-center gap-2 px-4 py-2 rounded-lg shadow-lg text-sm font-medium min-w-[200px] max-w-[360px]"
          :class="bgClass(toast.type)"
        >
          <span class="flex-1">{{ toast.message }}</span>
          <button
            class="opacity-70 hover:opacity-100 transition-opacity text-lg leading-none"
            @click="dismiss(toast.id)"
          >
            ×
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-enter-active {
  transition: all 0.2s ease-out;
}
.toast-leave-active {
  transition: all 0.15s ease-in;
}
.toast-enter-from {
  opacity: 0;
  transform: translateY(-40px);
}
.toast-leave-to {
  opacity: 0;
  transform: translateY(-10px);
}
</style>
