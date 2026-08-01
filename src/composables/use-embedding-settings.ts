import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

/**
 * The shape returned by the `get_embedding_status` backend command.
 *
 * `modelOverride` is the premium user's pinned embedding-model name
 * (`app_settings.embedding_model_override`). `undefined` when the key is absent
 * or empty (auto-detection active) - serialized as `null` by serde but kept
 * `undefined` here so the input can distinguish "not loaded yet" from
 * "explicitly cleared".
 */
export interface EmbeddingStatusInfo {
  status: string;
  model: string;
  dimensions: number;
  modelOverride?: string | null;
}

/**
 * Composable for loading + persisting the embedding-model override (premium).
 *
 * The override is a machine-local `app_settings` key that, when set, makes the
 * embedding probe try the user's pinned model first (ahead of the
 * provider-default + the configured chat model). See `.worktrees/setmodel.md`.
 *
 * Loads via `get_embedding_status` (which returns the full triple-state +
 * `modelOverride`) and saves via the premium-gated `set_embedding_model_override`
 * command. Saving resets the embedding capability to `unknown` so the next
 * probe (next embedding call or `Test Connection`) re-evaluates against the new
 * override.
 *
 * @returns `modelOverride` (reactive ref), `load()`, `save(value)`, and a
 * `saving` flag.
 */
export function useEmbeddingSettings() {
  const modelOverride = ref<string>('');
  const saving = ref(false);

  /**
   * Load the current embedding-model override from the backend.
   *
   * Sets `modelOverride` to the stored value (empty string when cleared). Safe
   * to call outside Tauri (no-op when `isTauri()` is false).
   */
  async function load(): Promise<void> {
    try {
      const info = await tauriCommand<EmbeddingStatusInfo>('get_embedding_status');
      modelOverride.value = info.modelOverride ?? '';
    } catch {
      // Best-effort: a read failure leaves the input empty. The Settings UI
      // degrades gracefully (the user can still type + save).
      modelOverride.value = '';
    }
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
      modelOverride.value = trimmed;
    } finally {
      saving.value = false;
    }
  }

  return { modelOverride, saving, load, save };
}
