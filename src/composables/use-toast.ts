import { ref } from 'vue';

export interface Toast {
  id: number;
  message: string;
  type: 'success' | 'info' | 'warning' | 'error';
  duration: number; // ms, 0 = persistent
}

const toasts = ref<Toast[]>([]);
let nextId = 0;

export function useToast() {
  function show(message: string, type: Toast['type'] = 'info', duration = 3000): void {
    const id = nextId++;
    toasts.value.push({ id, message, type, duration });
    if (duration > 0) {
      setTimeout(() => dismiss(id), duration);
    }
  }

  function dismiss(id: number): void {
    const idx = toasts.value.findIndex((t) => t.id === id);
    if (idx >= 0) toasts.value.splice(idx, 1);
  }

  return { toasts, show, dismiss };
}
