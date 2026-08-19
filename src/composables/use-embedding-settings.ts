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
 */
export function useEmbeddingSettings() {
  const modelOverride = ref<string>('');
  const saving = ref(false);

  /** Load the current embedding-model override from the backend. */
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
