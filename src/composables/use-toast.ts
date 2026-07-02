import { ref } from 'vue';

export interface Toast {
  id: number;
  message: string;
  type: 'success' | 'info' | 'warning' | 'error';
  duration: number; // ms, 0 = persistent
  timestamp: number; // epoch ms when shown
}

/** Maximum number of history entries kept in memory. */
const MAX_HISTORY = 100;
/** Default auto-dismiss duration (ms). Doubled from the previous 3000. */
const DEFAULT_DURATION = 6000;

const toasts = ref<Toast[]>([]);
const history = ref<Toast[]>([]);
let nextId = 0;

export function useToast() {
  function show(message: string, type: Toast['type'] = 'info', duration = DEFAULT_DURATION): void {
    const id = nextId++;
    const timestamp = Date.now();
    const toast: Toast = { id, message, type, duration, timestamp };
    toasts.value.push(toast);
    // Record into the in-memory history (newest at the end).
    history.value.push({ ...toast });
    if (history.value.length > MAX_HISTORY) {
      // Trim the oldest entries to stay within the cap.
      history.value.splice(0, history.value.length - MAX_HISTORY);
    }
    if (duration > 0) {
      setTimeout(() => dismiss(id), duration);
    }
  }

  function dismiss(id: number): void {
    const idx = toasts.value.findIndex((t) => t.id === id);
    if (idx >= 0) toasts.value.splice(idx, 1);
  }

  function clearHistory(): void {
    history.value = [];
  }

  return { toasts, show, dismiss, history, clearHistory };
}
