import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

/**
 * Shape returned by `get_embedding_status`. `modelOverride` is the premium
 * user's pinned embedding-model name (`undefined` when absent/empty).
 */
interface EmbeddingStatusInfo {
  status: string;
  model: string;
  dimensions: number;
  modelOverride?: string | null;
}

/**
 * Composable for loading + persisting the embedding-model override (premium).
 * Saves via the premium-gated `set_embedding_model_override` command.
 *
 * Propagation vs. user edits: `isPersisted(value)` reports whether a value is
 * known to match the backend (loaded by `load()` or confirmed by `save()`).
 * Auto-save watchers use it to ignore propagation assignments and react to
 * every other change. A naive "skip the first change" flag is wrong here:
 * when the stored value is empty, `load()` produces no ref change, so the
 * flag survives and swallows the user's first (and possibly only) edit, which
 * is then never saved - the "field is blank after returning to Settings" bug.
 */
export function useEmbeddingSettings() {
  const modelOverride = ref<string>('');
  const saving = ref(false);
  /** Last backend-known override. `null` until the first `load()`/`save()`. */
  let persistedValue: string | null = null;

  /**
   * Whether `value` is known to match the backend state. Watchers use this to
   * skip save scheduling for propagation, never for user edits.
   */
  function isPersisted(value: string): boolean {
    return persistedValue === value;
  }

  /** Load the current embedding-model override from the backend. */
  async function load(): Promise<void> {
    // Snapshot the field so a user edit that lands while the read is in
    // flight wins (the debounced save reconciles the backend to it).
    const valueAtRequest = modelOverride.value;
    let next: string;
    try {
      const info = await tauriCommand<EmbeddingStatusInfo>('get_embedding_status');
      next = info.modelOverride ?? '';
    } catch {
      // Best-effort: a read failure keeps the last known state. The Settings
      // UI degrades gracefully (the user can still type + save).
      return;
    }
    if (modelOverride.value === valueAtRequest) {
      modelOverride.value = next;
    }
    persistedValue = next;
  }

  /**
   * Persist the embedding-model override. Pass an empty/whitespace string to
   * clear the override (restore auto-detection).
   *
   * @param value - the model name to pin, or empty to clear.
   * @throws when the backend rejects the save (e.g. non-premium user).
   */
  async function save(value: string): Promise<void> {
    saving.value = true;
    try {
      const trimmed = value.trim();
      await tauriCommand('set_embedding_model_override', {
        value: trimmed.length > 0 ? trimmed : null,
      });
      // Mark as backend-known BEFORE any ref write, so the watcher treats
      // the normalization below as propagation, not a new user edit.
      persistedValue = trimmed;
      modelOverride.value = trimmed;
    } finally {
      saving.value = false;
    }
  }

  return { modelOverride, saving, isPersisted, load, save };
}
