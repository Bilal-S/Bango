import { ref } from 'vue';
import type { Ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

/** Saved report row shape shared by the summary + gap-analysis backends. */
interface SavedReportRow {
  generatedAt: string;
}

/**
 * Factory for the single-row LLM report composables (summary, gap analysis).
 * Owns the shared load/generate/clear/format scaffold over refs the caller
 * holds at module level, so every report keeps its singleton semantics.
 * Per-report extras (e.g. the summary's `citationStyle`) hook in via
 * `onLoaded`/`onClear`. Public composable APIs stay identical.
 *
 * @param options.getCommand IPC command returning the saved row (or null).
 * @param options.generateCommand IPC command generating a fresh report.
 * @param options.readText Maps the saved row to the report text field.
 * @param options.onLoaded Optional hook for non-text saved fields.
 * @param options.onClear Optional hook resetting extras when `clear()` runs.
 * @returns Shared report state + actions.
 */
export function createSavedReport<T extends SavedReportRow>(options: {
  getCommand: string;
  generateCommand: string;
  readText: (saved: T) => string;
  onLoaded?: (saved: T) => void;
  onClear?: () => void;
}): {
  text: Ref<string | null>;
  loading: Ref<boolean>;
  error: Ref<string | null>;
  generatedAt: Ref<string | null>;
  loadSaved: () => Promise<void>;
  generate: (style?: string) => Promise<void>;
  clear: () => void;
  formatGeneratedAt: () => string | null;
} {
  const text = ref<string | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const generatedAt = ref<string | null>(null);

  async function loadSaved(): Promise<void> {
    try {
      const saved = await tauriCommand<T | null>(options.getCommand, {});
      if (saved) {
        text.value = options.readText(saved);
        generatedAt.value = saved.generatedAt;
        options.onLoaded?.(saved);
      }
    } catch {
      // Silently ignore - a saved report is optional.
    }
  }

  /** Reset report state (called on import or project reset). */
  function clear(): void {
    text.value = null;
    generatedAt.value = null;
    error.value = null;
    options.onClear?.();
  }

  async function generate(style = 'APA'): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const result = await tauriCommand<string>(options.generateCommand, {
        citationStyle: style,
      });
      text.value = result;
      // The backend saves with a timestamp; reload to get the exact server timestamp.
      await loadSaved();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  /** Format the ISO timestamp using the user's system locale. */
  function formatGeneratedAt(): string | null {
    if (!generatedAt.value) return null;
    try {
      const date = new Date(generatedAt.value);
      return new Intl.DateTimeFormat(undefined, {
        dateStyle: 'medium',
        timeStyle: 'short',
      }).format(date);
    } catch {
      return generatedAt.value;
    }
  }

  return { text, loading, error, generatedAt, loadSaved, generate, clear, formatGeneratedAt };
}
