import { ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { tauriCommand } from './use-tauri-command';
import { useLlmConfigStore } from '@/stores/llm-config';
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

  // Clear connection status whenever any LLM setting changes
  watch(
    () => ({
      provider: config.value.provider,
      endpointUrl: config.value.endpointUrl,
      apiKeyEncrypted: config.value.apiKeyEncrypted,
      modelName: config.value.modelName,
      temperature: config.value.temperature,
      maxConcurrentRequests: config.value.maxConcurrentRequests,
      requestDelayMs: config.value.requestDelayMs,
      contextWindowTokens: config.value.contextWindowTokens,
    }),
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
      lastSavedAt.value = Date.now();
    } finally {
      saving.value = false;
    }
  }

  // ── Debounced auto-save for the Parameters fields ───────────────────────
  //
  // The Parameters card (concurrency / context tokens / request delay /
  // temperature) has no Save button. Without auto-save, edits live only in
  // the in-memory Pinia store: they never reach `save_llm_config` (so the
  // orchestrator's `update_settings` is never called and the next LLM call
  // uses stale concurrency/delay) and are lost on navigation (the next view
  // triggers a fresh `fetch()` that overwrites them).
  //
  // `scheduleParamSave` debounces `save()` by `delayMs` (default 600ms) so
  // dragging the Context Tokens slider (which fires many intermediate values)
  // only produces one save per pause. `cancelScheduledParamSave` lets callers
  // (Revert, Test Connection) drop a pending save so it doesn't fight with
  // their own save/fetch. `lastSavedAt` bumps on every successful save so the
  // UI can react (e.g. invalidate the cached screening-readiness estimate).
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
    return ['llamaCpp', 'ollama', 'lmStudio'].includes(store.config.provider);
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
