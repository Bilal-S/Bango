import { ref } from 'vue';
import { storeToRefs } from 'pinia';
import { tauriCommand } from './use-tauri-command';
import { useLlmConfigStore } from '@/stores/llm-config';

interface TestResult {
  success: boolean;
  message: string;
}

export function useLlmConfig() {
  const store = useLlmConfigStore();

  // storeToRefs gives us config as a writable Ref<LlmConfig> so
  // config.value.xxx reads and writes work unchanged in the view.
  const { config } = storeToRefs(store);
  const loading = ref(false);
  const saving = ref(false);
  const testing = ref(false);
  const testResult = ref<TestResult | null>(null);
  const showApiKey = ref(false);
  const fetchingModels = ref(false);
  const fetchedModels = ref<string[] | null>(null);

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
    testResult.value = null;
    try {
      await save();
      testResult.value = await tauriCommand<TestResult>('test_llm_connection');
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      testResult.value = { success: false, message };
    } finally {
      testing.value = false;
    }
  }

  function revert(): void {
    store.invalidate();
    void store.fetch();
    testResult.value = null;
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
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      testResult.value = { success: false, message: `Failed to fetch models: ${message}` };
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
