import { defineStore } from 'pinia';
import { ref } from 'vue';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
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

export const useLlmConfigStore = defineStore('llm-config', () => {
  const config = ref<LlmConfig>({ ...DEFAULT_CONFIG });
  const loading = ref(false);
  const initialized = ref(false);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetch();
  }

  async function fetch(): Promise<void> {
    loading.value = true;
    try {
      const saved = await tauriCommand<LlmConfig | null>('get_llm_config');
      if (saved) {
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
  }

  return { config, loading, initialized, fetchIfNeeded, fetch, invalidate };
});
