import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
import type { LlmConfig, LlmProvider } from '@/types';

export interface TestResult {
  success: boolean;
  message: string;
  /** Embedding capability from sync probe. Undefined when test failed. */
  embeddingStatus?: string;
  /** Working embedding model name (set when embeddingStatus is `"enabled"`). */
  embeddingModel?: string;
}

/** Minimum context window in tokens for the Settings slider + load-time clamp. */
export const MIN_CONTEXT_WINDOW_TOKENS = 16_000;

/** Local LLM providers that do not require an API key. Must stay in sync with
 *  the backend Rust `is_local` match in `llm_config_repo::has_config`. */
export const LOCAL_PROVIDERS: ReadonlySet<LlmProvider> = new Set([
  'ollama',
  'lmStudio',
  'llamaCpp',
]);

/** Canonical "is this provider local (no API key)?" predicate. Mirrors the
 *  backend `is_local` match. */
export function isLocalProvider(provider: LlmProvider): boolean {
  return LOCAL_PROVIDERS.has(provider);
}

const DEFAULT_CONFIG: LlmConfig = {
  provider: 'openai',
  endpointUrl: 'https://api.openai.com/v1',
  apiKeyEncrypted: null,
  modelName: 'gpt-5-mini',
  temperature: 0.2,
  skipTemperature: false,
  maxConcurrentRequests: 3,
  requestDelayMs: 500,
  contextWindowTokens: 50000,
};

export const useLlmConfigStore = defineStore('llm-config', () => {
  const config = ref<LlmConfig>({ ...DEFAULT_CONFIG });
  const loading = ref(false);
  const initialized = ref(false);
  const testResult = ref<TestResult | null>(null);

  /**
   * Whether an LLM provider is fully configured. Mirrors the backend
   * `llm_config_repo::has_config` contract: initialized, endpoint+model
   * non-empty, and either local or an API key is present.
   */
  const isConfigured = computed(() => {
    if (!initialized.value) return false;
    const c = config.value;
    if (!c.endpointUrl.trim() || !c.modelName.trim()) return false;
    return LOCAL_PROVIDERS.has(c.provider) || !!c.apiKeyEncrypted;
  });

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetch();
  }

  async function fetch(): Promise<void> {
    loading.value = true;
    try {
      const saved = await tauriCommand<LlmConfig | null>('get_llm_config');
      if (saved) {
        // Clamp a legacy sub-floor context window up to the minimum so the
        // badge and slider (whose min is now MIN_CONTEXT_WINDOW_TOKENS) stay
        // consistent with the persisted value. A config at/above the floor
        // is left untouched.
        if (saved.contextWindowTokens < MIN_CONTEXT_WINDOW_TOKENS) {
          saved.contextWindowTokens = MIN_CONTEXT_WINDOW_TOKENS;
        }
        config.value = saved;
      }
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  function invalidate(): void {
    config.value = { ...DEFAULT_CONFIG };
    initialized.value = false;
    testResult.value = null;
  }

  function clearTestResult(): void {
    testResult.value = null;
  }

  return {
    config,
    loading,
    initialized,
    testResult,
    isConfigured,
    fetchIfNeeded,
    fetch,
    invalidate,
    clearTestResult,
  };
});
