import { ref, onMounted } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { LlmConfig } from '@/types';

const DEFAULT_CONFIG: LlmConfig = {
  provider: 'openai',
  endpointUrl: '',
  apiKeyEncrypted: null,
  modelName: '',
  temperature: 0.2,
  maxConcurrentRequests: 3,
  requestDelayMs: 500,
  contextWindowTokens: 50000,
};

interface TestResult {
  success: boolean;
  message: string;
}

export function useLlmConfig() {
  const config = ref<LlmConfig>({ ...DEFAULT_CONFIG });
  const loading = ref(false);
  const saving = ref(false);
  const testing = ref(false);
  const testResult = ref<TestResult | null>(null);
  const showApiKey = ref(false);
  const fetchingModels = ref(false);
  const fetchedModels = ref<string[] | null>(null);

  onMounted(loadConfig);

  async function loadConfig(): Promise<void> {
    loading.value = true;
    try {
      const saved = await tauriCommand<LlmConfig | null>('get_llm_config');
      if (saved) {
        config.value = saved;
      }
    } finally {
      loading.value = false;
    }
  }

  async function save(): Promise<void> {
    saving.value = true;
    try {
      await tauriCommand('save_llm_config', { config: config.value });
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
    config.value = { ...DEFAULT_CONFIG };
    testResult.value = null;
    fetchedModels.value = null;
  }

  function isLocalProvider(): boolean {
    return ['llama_cpp', 'ollama', 'lm_studio'].includes(config.value.provider);
  }

  async function fetchModels(): Promise<void> {
    fetchingModels.value = true;
    try {
      fetchedModels.value = await tauriCommand<string[]>('list_llm_models', {
        request: {
          provider: config.value.provider,
          endpointUrl: config.value.endpointUrl,
          apiKey: config.value.apiKeyEncrypted,
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
