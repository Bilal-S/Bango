import { ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { tauriCommand } from './use-tauri-command';
import { useLlmConfigStore } from '@/stores/llm-config';
import type { TestResult } from '@/stores/llm-config';

export type { TestResult };

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
    } finally {
      saving.value = false;
    }
  }

  async function testConnection(): Promise<void> {
    testing.value = true;
    store.clearTestResult();
    // Use requestAnimationFrame (macrotask) to guarantee the browser paints the spinner
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    try {
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
    loadConfig,
    save,
    testConnection,
    revert,
    isLocalProvider,
    fetchModels,
    resetFetchedModels,
  };
}
