import { ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { tauriCommand } from './use-tauri-command';
import { useLlmConfigStore, isLocalProvider as storeIsLocalProvider } from '@/stores/llm-config';
import type { TestResult } from '@/stores/llm-config';

const providerDisplayNames: Record<string, string> = {
  openai: 'OpenAI',
  anthropic: 'Anthropic',
  google: 'Google Gemini',
  mistralAi: 'Mistral AI',
  zAi: 'Z.AI',
  ollama: 'Ollama',
  lmStudio: 'LM Studio',
  llamaCpp: 'llama.cpp',
  custom: 'Custom',
};

export function useLlmConfig() {
  const store = useLlmConfigStore();

  // storeToRefs gives us config and testResult as reactive Refs backed by
  // the Pinia store, so the connection status persists across route changes.
  const { config, testResult } = storeToRefs(store);
  const loading = ref(false);
  const saving = ref(false);
  const testing = ref(false);
  const showApiKey = ref(false);
  const fetchingModels = ref(false);
  const fetchedModels = ref<string[] | null>(null);

  /* Clear connection status whenever any LLM setting changes. MUST use an
     array of getter sources (not a getter returning a fresh object/array):
     the latter always yields a new reference so Vue reports "changed" on
     every reactive touch, causing an infinite save -> fetch -> clearTestResult
     -> save loop after every edit (status pill flickered between "Saving..."
     and "Not Tested"). Array-of-getters compares each value via Object.is,
     so it fires only on real value changes - including the identical-value
     reassignment `store.fetch()` performs after every save. */
  watch(
    [
      () => config.value.provider,
      () => config.value.endpointUrl,
      () => config.value.apiKeyEncrypted,
      () => config.value.modelName,
      () => config.value.temperature,
      () => config.value.maxConcurrentRequests,
      () => config.value.requestDelayMs,
      () => config.value.contextWindowTokens,
    ],
    () => {
      store.clearTestResult();
    }
  );

  async function loadConfig(): Promise<void> {
    await store.fetch();
  }

  async function save(): Promise<void> {
    saving.value = true;
    try {
      await tauriCommand('save_llm_config', { config: store.config });
      /* Re-fetch so the in-memory store reflects the post-save DB state
      (the backend encrypts `api_key_encrypted`, replacing the user's
      plaintext). Keeps `isConfigured` accurate after every save. */
      await store.fetch();
      lastSavedAt.value = Date.now();
    } finally {
      saving.value = false;
    }
  }

  /* Debounced auto-save for Parameters fields (concurrency/context tokens/
  request delay/temperature). Without it, edits never reach `save_llm_config`
  so the orchestrator uses stale values, and navigation loses edits.
  `scheduleParamSave` debounces `save()` by `delayMs` so slider drags only
  produce one save per pause. `cancelScheduledParamSave` drops pending saves
  (Revert, Test Connection). `lastSavedAt` bumps on save for UI reactivity. */
  const paramSaveTimer = ref<ReturnType<typeof setTimeout> | null>(null);
  const lastSavedAt = ref(0);
  const PARAM_SAVE_DELAY_MS = 600;

  function cancelScheduledParamSave(): void {
    if (paramSaveTimer.value !== null) {
      clearTimeout(paramSaveTimer.value);
      paramSaveTimer.value = null;
    }
  }

  function scheduleParamSave(delayMs: number = PARAM_SAVE_DELAY_MS): void {
    cancelScheduledParamSave();
    paramSaveTimer.value = setTimeout(() => {
      paramSaveTimer.value = null;
      void save();
    }, delayMs);
  }

  async function testConnection(): Promise<void> {
    testing.value = true;
    store.clearTestResult();
    // Use double requestAnimationFrame (macrotask) to guarantee the browser paints the spinner
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
    );
    try {
      // Drop any pending debounced save so it doesn't race with this explicit
      // save (the test path saves then immediately tests).
      cancelScheduledParamSave();
      await save();
      const result = await tauriCommand<TestResult>('test_llm_connection');
      // Refresh config BEFORE setting testResult so the watch on config
      // doesn't wipe the "Connected" status we are about to set.
      if (result.success) {
        await store.fetch();
      }
      store.testResult = result;
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      store.testResult = { success: false, message };
    } finally {
      testing.value = false;
    }
  }

  function revert(): void {
    // Drop any pending debounced save so it can't overwrite the reverted
    // (re-fetched) config a moment later.
    cancelScheduledParamSave();
    store.invalidate();
    void store.fetch();
    store.clearTestResult();
    fetchedModels.value = null;
  }

  function isLocalProvider(): boolean {
    /* Delegate to the store's canonical predicate so the local-provider set
       has exactly one frontend definition (mirrors the backend `is_local`
       match); see the root AGENTS.md single-definition contract. */
    return storeIsLocalProvider(store.config.provider);
  }

  async function fetchModels(): Promise<void> {
    fetchingModels.value = true;
    try {
      fetchedModels.value = await tauriCommand<string[]>('list_llm_models', {
        request: {
          provider: store.config.provider,
          endpointUrl: store.config.endpointUrl,
          apiKey: store.config.apiKeyEncrypted,
        },
      });
      const providerName = providerDisplayNames[store.config.provider] ?? store.config.provider;
      store.testResult = {
        success: true,
        message: `Updated models list for ${providerName}`,
      };
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      store.testResult = { success: false, message: `Failed to fetch models: ${message}` };
    } finally {
      fetchingModels.value = false;
    }
  }

  function resetFetchedModels(): void {
    fetchedModels.value = null;
  }

  return {
    config,
    loading,
    saving,
    testing,
    testResult,
    showApiKey,
    fetchingModels,
    fetchedModels,
    lastSavedAt,
    loadConfig,
    save,
    scheduleParamSave,
    cancelScheduledParamSave,
    testConnection,
    revert,
    isLocalProvider,
    fetchModels,
    resetFetchedModels,
  };
}
