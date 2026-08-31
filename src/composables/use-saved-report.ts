import { ref } from 'vue';
import type { Ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

/** Saved report row shape shared by the summary + gap-analysis backends. */
interface SavedReportRow {
  generatedAt: string;
}

/** Optional premium generation extras forwarded with every report command. */
export interface SavedReportGenerateOptions {
  /** Citation style sent as `citationStyle` (defaults to `'APA'`). */
  style?: string;
  /** Free-form LLM instructions sent as `additionalInstructions` when non-blank. */
  additionalInstructions?: string;
  /** Target length in words sent as `targetWordCount` when a positive integer. */
  targetWordCount?: number | null;
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
  generate: (gen?: SavedReportGenerateOptions) => Promise<void>;
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

  /** Generate a fresh report. Builds the IPC payload from the options:
   * `citationStyle` always, plus the optional premium guidance extras
   * (`additionalInstructions` trimmed-non-blank, `targetWordCount` floored
   * positive-integer) only when provided. */
  async function generate(gen: SavedReportGenerateOptions = {}): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const payload: Record<string, string | number> = { citationStyle: gen.style ?? 'APA' };
      const trimmedInstructions = gen.additionalInstructions?.trim() ?? '';
      if (trimmedInstructions) {
        payload.additionalInstructions = trimmedInstructions;
      }
      const words = gen.targetWordCount;
      if (typeof words === 'number' && Number.isFinite(words) && words > 0) {
        payload.targetWordCount = Math.floor(words);
      }
      const result = await tauriCommand<string>(options.generateCommand, payload);
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
