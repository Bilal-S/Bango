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
 *  the backend Rust `is_local` match in `llm_config_repo::has_config`.
 *  Store-private: `isLocalProvider` is the only public accessor. */
const LOCAL_PROVIDERS: ReadonlySet<LlmProvider> = new Set(['ollama', 'lmStudio', 'llamaCpp']);

/** Canonical "is this provider local (no API key)?" predicate. Mirrors the
 *  backend `is_local` match. */
export function isLocalProvider(provider: LlmProvider): boolean {
  return LOCAL_PROVIDERS.has(provider);
}

/** Generation backend selection (mirrors the Rust `LlmBackend`). */
export type LlmBackendId = 'configured_provider' | 'bango_ai';

/** Minimal shape of the `get_bango_ai_status` payload used here. */
interface BangoAiStatusLike {
  state?: string;
  supportedTarget?: boolean;
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
  // Matches the OpenAI per-provider default in settings-provider-card.vue.
  contextWindowTokens: 128000,
};

export const useLlmConfigStore = defineStore('llm-config', () => {
  const config = ref<LlmConfig>({ ...DEFAULT_CONFIG });
  const loading = ref(false);
  const initialized = ref(false);
  const testResult = ref<TestResult | null>(null);
  const backend = ref<LlmBackendId>('configured_provider');
  const localReady = ref(false);
  const localSupported = ref(true);

  /**
   * Whether an LLM provider is fully configured. Mirrors the backend
   * `llm_config_repo::has_config` contract under `configured_provider`; under
   * `bango_ai` it is the installed-component readiness instead (the cloud row
   * is never consulted).
   */
  const isConfigured = computed(() => {
    if (!initialized.value) return false;
    if (backend.value === 'bango_ai') return localReady.value && localSupported.value;
    const c = config.value;
    if (!c.endpointUrl.trim() || !c.modelName.trim()) return false;
    return isLocalProvider(c.provider) || !!c.apiKeyEncrypted;
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
      /* Backend awareness is best-effort: a failed/absent status call keeps
      the cloud rule so the gate stays conservative. */
      await refreshBackendState();
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  /**
   * Re-read the backend selection + Bango AI readiness. Called whenever
   * Bango AI state legitimately changes (install completion, backend switch)
   * so `isConfigured` unlocks - or re-locks - without an app restart
   * (aifixes1 F16: downloaded-and-ready local AI must unlock all LLM gates).
   */
  async function refreshBackendState(): Promise<void> {
    /* Best-effort: a failed/absent status call keeps the cloud rule so the
    gate stays conservative. */
    try {
      const selected = await tauriCommand<string>('get_llm_backend');
      if (selected === 'bango_ai' || selected === 'configured_provider') {
        backend.value = selected;
      }
      const status = await tauriCommand<BangoAiStatusLike | null>('get_bango_ai_status');
      localReady.value = status?.state === 'ready';
      localSupported.value = status?.supportedTarget !== false;
    } catch {
      localReady.value = false;
    }
  }

  function invalidate(): void {
    config.value = { ...DEFAULT_CONFIG };
    initialized.value = false;
    testResult.value = null;
    backend.value = 'configured_provider';
    localReady.value = false;
    localSupported.value = true;
  }

  function clearTestResult(): void {
    testResult.value = null;
  }

  return {
    config,
    loading,
    initialized,
    testResult,
    backend,
    localReady,
    localSupported,
    isConfigured,
    fetchIfNeeded,
    fetch,
    refreshBackendState,
    invalidate,
    clearTestResult,
  };
});
