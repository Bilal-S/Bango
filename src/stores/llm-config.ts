import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
import type { LlmConfig, LlmProvider } from '@/types';

export interface TestResult {
  success: boolean;
  message: string;
  /** Embedding capability outcome from the synchronous probe. Undefined when
   * the connection test failed (no probe ran). `"enabled"` / `"disabled"` when
   * the probe ran. Surfaced in the Test Connection message so the user knows
   * whether semantic search is available. */
  embeddingStatus?: string;
  /** The working embedding model name (only set when embeddingStatus is
   * `"enabled"`). */
  embeddingModel?: string;
}

/**
 * The minimum selectable context window, in tokens. The Settings UI slider
 * floor and the load-time clamp both use this so a legacy config below the
 * floor is transparently bumped up on load (keeping the badge and slider
 * consistent with the UI floor).
 */
export const MIN_CONTEXT_WINDOW_TOKENS = 16_000;

/**
 * Local LLM providers that do not require an API key. Mirrors the backend
 * `is_local` set in `llm_config_repo::has_config` (`ollama` | `lm_studio` |
 * `llama_cpp`). Cloud providers require an API key; these do not. Kept as a
 * shared constant so any frontend gate (e.g. `isConfigured`, future readiness
 * checks) stays in sync.
 *
 * Exported so every caller that needs the local-provider distinction (the
 * `useLlmConfig` composable, the Settings card's required-key halo, etc.)
 * reads from ONE copy instead of re-deriving it. The backend Rust match in
 * `llm_config_repo::has_config` is the ultimate source of truth; this Set
 * must stay in sync with it.
 */
export const LOCAL_PROVIDERS: ReadonlySet<LlmProvider> = new Set([
  'ollama',
  'lmStudio',
  'llamaCpp',
]);

/**
 * Canonical "is this provider local (no API key required)?" predicate.
 * Mirrors the backend `is_local` match in `llm_config_repo::has_config`.
 * Use this instead of re-deriving the provider list at every call site so
 * the local-provider contract has exactly one frontend definition.
 *
 * @param provider - The LLM provider identifier to test.
 * @returns `true` for `ollama`, `lmStudio`, `llamaCpp`; `false` otherwise.
 */
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
   * Whether an LLM provider is fully configured and ready to serve requests.
   * Mirrors the backend `llm_config_repo::has_config` contract: the store must
   * be initialized, endpoint + model must be non-empty, and either the provider
   * is local (no key required) or an API key is present. Use this getter
   * everywhere a feature gates on "LLM configured" instead of re-deriving it
   * from `apiKeyEncrypted` (which incorrectly disables local providers like
   * LM Studio / Ollama / llama.cpp).
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
