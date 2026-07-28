<script setup lang="ts">
import { watch, ref, computed, nextTick } from 'vue';
import { useRouter } from 'vue-router';
import { useLlmConfig } from '@/composables/use-llm-config';
import { useScreeningStore } from '@/stores/screening';
import { formatLlmError } from '@/utils/llm-error';

const router = useRouter();
const screeningStore = useScreeningStore();

const {
  config,
  saving,
  testing,
  testResult,
  showApiKey,
  fetchingModels,
  fetchedModels,
  lastSavedAt,
  testConnection,
  revert,
  isLocalProvider,
  fetchModels,
  resetFetchedModels,
  scheduleParamSave,
} = useLlmConfig();

const showRawError = ref(false);

/**
 * Deep-link to the relevant section of the Local & Free AI help tab.
 * Cloud/network providers -> Free AI section; local providers -> Local AI Setup Guide.
 */
function goToProviderHelp(): void {
  router.push({
    path: '/help',
    query: { tab: 'local-ai' },
    hash: isLocalProvider() ? '#local-ai-setup' : '#free-ai',
  });
}

// Max Context Tokens inline editing
const editingContextTokens = ref(false);
const contextTokensInput = ref('');
const contextTokensInputRef = ref<HTMLInputElement | null>(null);

function startEditingContextTokens(): void {
  editingContextTokens.value = true;
  contextTokensInput.value = String(config.value.contextWindowTokens);
  nextTick(() => {
    contextTokensInputRef.value?.focus();
    contextTokensInputRef.value?.select();
  });
}

const CONTEXT_MAX_CEILING = 1_000_000;
/** The minimum selectable context window, in tokens. */
const CONTEXT_MIN_FLOOR = 16_000;

function commitContextTokens(): void {
  const raw = contextTokensInput.value.replace(/[^0-9]/g, '');
  let parsed = parseInt(raw, 10);
  if (isNaN(parsed) || parsed < CONTEXT_MIN_FLOOR) {
    parsed = CONTEXT_MIN_FLOOR;
  } else if (parsed > CONTEXT_MAX_CEILING) {
    parsed = CONTEXT_MAX_CEILING;
  }
  // Round to nearest 1000 to keep in sync with slider step
  config.value.contextWindowTokens = Math.round(parsed / 1000) * 1000;
  editingContextTokens.value = false;
}

const contextSliderMax = computed(() => Math.max(config.value.contextWindowTokens, 50000));

function formatCompact(n: number): string {
  if (n >= 1_000_000) return `${n / 1_000_000}M`;
  if (n >= 1_000) return `${n / 1_000}k`;
  return String(n);
}

const providerDefaults: Record<string, { url: string; models: string[] }> = {
  openai: {
    url: 'https://api.openai.com/v1',
    models: [
      'gpt-5-mini',
      'chat-latest',
      'gpt-5.1-chat-latest',
      'gpt-5.4',
      'gpt-5.4-pro',
      'gpt-5.4-mini',
      'gpt-5.4-nano',
      'gpt-5.5',
    ],
  },
  anthropic: {
    url: 'https://api.anthropic.com/v1',
    models: [
      'claude-haiku-4-5',
      'claude-opus-4-5',
      'claude-opus-4-6',
      'claude-opus-4-7',
      'claude-sonnet-4-5',
      'claude-sonnet-4-6',
    ],
  },
  google: {
    url: 'https://generativelanguage.googleapis.com/v1beta',
    models: [
      'gemini-flash-latest',
      'gemini-flash-lite-latest',
      'gemini-pro-latest',
      'gemini-3.1-pro-preview"',
    ],
  },
  mistralAi: {
    url: 'https://api.mistral.ai/v1',
    models: [
      'mistral-small-latest',
      'magistral-medium-latest',
      'mistral-medium-latest',
      'mistral-large-latest',
      'codestral-latest',
      'codestral-mamba-latest',
    ],
  },
  zAi: {
    url: 'https://api.z.ai/api/paas/v4',
    models: ['glm-5-turbo', 'glm-4.7-flash', 'glm-5', 'glm-5.1'],
  },
  ollama: {
    url: 'http://localhost:11434/v1',
    models: [],
  },
  lmStudio: {
    url: 'http://localhost:1234/v1',
    models: [],
  },
  llamaCpp: {
    url: 'http://localhost:8080/v1',
    models: [],
  },
};

const isOtherModel = ref(false);

const llmErrorInfo = computed(() => {
  if (!testResult.value || testResult.value.success) {
    return {
      prefix: '',
      details: '',
      helpLink: '',
      matched: false,
      anchorId: null,
      solution: null,
      cause: null,
    };
  }
  return formatLlmError(testResult.value.message);
});

const availableModels = computed(() => {
  if (fetchedModels.value && fetchedModels.value.length > 0) {
    return fetchedModels.value;
  }
  return providerDefaults[config.value.provider]?.models || [];
});

const canFetchModels = computed(() => {
  if (!config.value.endpointUrl.trim()) return false;
  if (!config.value.modelName.trim()) return false;
  if (isLocalProvider()) return true;
  return !!config.value.apiKeyEncrypted;
});

const canTestConnection = computed(() => {
  if (!config.value.endpointUrl.trim()) return false;
  if (!config.value.modelName.trim()) return false;
  if (isLocalProvider()) return true;
  return !!config.value.apiKeyEncrypted;
});

const needsApiKey = computed(() => !isLocalProvider());

watch(
  () => config.value.provider,
  (newProvider, oldProvider) => {
    if (oldProvider && newProvider !== oldProvider) {
      resetFetchedModels();
      config.value.skipTemperature = false;
      const defaults = providerDefaults[newProvider];
      if (defaults) {
        config.value.endpointUrl = defaults.url;
        config.value.modelName = defaults.models[0] || '';
        isOtherModel.value = defaults.models.length === 0;
      } else {
        config.value.endpointUrl = '';
        config.value.modelName = '';
        isOtherModel.value = true;
      }
    }
  }
);

// Detect imported model names that don't match available select options.
// When the loaded model isn't in the provider's default list, switch to
// the custom text input so the user can see and edit their imported model name.
watch(
  () => config.value.modelName,
  (name) => {
    if (name && availableModels.value.length > 0 && !availableModels.value.includes(name)) {
      isOtherModel.value = true;
    }
  },
  { immediate: true }
);

// Clear temperature skip flag when model changes (within same provider)
watch(
  () => config.value.modelName,
  (newModel, oldModel) => {
    if (oldModel && newModel !== oldModel) {
      config.value.skipTemperature = false;
    }
  }
);

// ── Debounced auto-save for the Parameters fields ─────────────────────────
//
// The Parameters card has no Save button. Without this watcher, editing
// Concurrency / Context Tokens / Request Delay / Temperature mutates only the
// in-memory Pinia store: the change never reaches `save_llm_config`, so the
// orchestrator's `update_settings` is never called and the next LLM call uses
// stale concurrency/delay. The watcher debounces `save()` (600ms trailing) so
// dragging the Context Tokens slider fires one save per pause, not one per
// tick. It is gated on `!testing` (Test Connection already saves) and skips
// the very first run so loading the config from the DB doesn't trigger a
// spurious re-save. After a successful save the cached screening-readiness
// estimate (which depends on `contextWindowTokens`) is invalidated so the
// progress bar reflects the new value on the next screening-view visit.
let paramSaveStarted = false;
watch(
  () => [
    config.value.maxConcurrentRequests,
    config.value.requestDelayMs,
    config.value.contextWindowTokens,
    config.value.temperature,
  ],
  () => {
    // Skip the initial propagation that fires when the config is first loaded
    // from the DB into the store (avoid an immediate no-op save round-trip).
    if (!paramSaveStarted) {
      paramSaveStarted = true;
      return;
    }
    // Don't schedule a debounced save while Test Connection is running: that
    // path saves explicitly + re-fetches, and a concurrent debounced save
    // could race with the re-fetch.
    if (testing.value) return;
    scheduleParamSave();
  }
);

// Invalidate the cached screening-readiness estimate whenever a save lands.
// `readiness` includes a token estimate derived from `contextWindowTokens`
// (via `worst_case_per_article_tokens`); without this invalidation the
// Articles-view progress bar keeps showing the old context window until the
// user navigates away and back (which triggers `fetchIfNeeded`). Reactive on
// `lastSavedAt` so it only fires after a successful save, not on every edit.
watch(lastSavedAt, (ts) => {
  if (ts > 0) {
    screeningStore.invalidate();
  }
});
</script>

<template>
  <!-- ONE consolidated box for all AI Provider settings -->
  <section class="provider-card">
    <!-- Section header -->
    <header class="provider-card__header">
      <h2 class="provider-card__title">
        <span class="material-symbols-outlined text-primary">smart_toy</span>
        AI Provider
      </h2>
      <p class="provider-card__subtitle">
        Configure the LLM endpoint, credentials, and inference parameters. AI models can make
        mistakes - always verify decisions.
      </p>
    </header>

    <!-- Local hardware warning (inside the box) -->
    <div v-if="isLocalProvider()" class="provider-card__warning">
      <span class="material-symbols-outlined provider-card__warning-icon">warning</span>
      <p class="provider-card__warning-text">
        <strong>Hardware Requirement Notice:</strong> Local providers require 16+ GB VRAM for 50k
        token context to maintain stable inference speeds.
      </p>
    </div>

    <!-- Inputs + parameters side by side -->
    <div class="provider-card__grid">
      <!-- Connection Details -->
      <div class="provider-card__sub">
        <h3 class="provider-card__sub-title">
          <span class="material-symbols-outlined text-primary">dns</span>
          Connection Details
        </h3>
        <div class="field-group">
          <!-- Provider -->
          <div class="field">
            <label class="field__label">Provider</label>
            <div class="field__select-wrapper">
              <select v-model="config.provider" class="field__select">
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
                <option value="google">Google Gemini</option>
                <option value="mistralAi">Mistral AI</option>
                <option value="zAi">z.ai</option>
                <option value="llamaCpp">llama.cpp</option>
                <option value="ollama">Ollama</option>
                <option value="lmStudio">LM Studio</option>
                <option value="custom">Custom</option>
              </select>
              <span class="material-symbols-outlined field__select-arrow">expand_more</span>
            </div>
          </div>

          <!-- Endpoint URL -->
          <div class="field">
            <label class="field__label">Endpoint Base URL</label>
            <input
              v-model="config.endpointUrl"
              type="url"
              class="field__input field__input--mono"
              :class="{ 'field__input--required-halo': !config.endpointUrl.trim() }"
              placeholder="https://api.openai.com/v1/chat/completions"
            />
          </div>

          <!-- Model Name & API Key -->
          <div class="field-row">
            <div class="field">
              <label class="field__label">Model Name</label>
              <div v-if="!isOtherModel && availableModels.length > 0" class="field__select-wrapper">
                <select
                  v-model="config.modelName"
                  class="field__select field__select--mono"
                  :class="{ 'field__select--required-halo': !config.modelName.trim() }"
                  @change="
                    (e) => {
                      if ((e.target as HTMLSelectElement).value === 'other') {
                        isOtherModel = true;
                        config.modelName = '';
                      }
                    }
                  "
                >
                  <option v-for="model in availableModels" :key="model" :value="model">
                    {{ model }}
                  </option>
                  <option value="other">Other...</option>
                </select>
                <span class="material-symbols-outlined field__select-arrow">expand_more</span>
              </div>
              <div v-else style="display: flex; gap: 0.5rem">
                <input
                  v-model="config.modelName"
                  type="text"
                  class="field__input field__input--mono"
                  :class="{ 'field__input--required-halo': !config.modelName.trim() }"
                  placeholder="e.g. custom-model-v1"
                  style="flex: 1"
                />
                <button
                  v-if="availableModels.length > 0"
                  class="btn btn--secondary"
                  title="Back to list"
                  style="padding: 0 0.75rem"
                  @click="
                    isOtherModel = false;
                    config.modelName = availableModels[0] || '';
                  "
                >
                  <span class="material-symbols-outlined">list</span>
                </button>
              </div>
            </div>
            <div class="field">
              <label class="field__label">API Key</label>
              <div class="field__password-wrapper">
                <input
                  v-model="config.apiKeyEncrypted"
                  :type="showApiKey ? 'text' : 'password'"
                  class="field__input field__input--mono field__input--password"
                  :class="{ 'field__input--required-halo': needsApiKey && !config.apiKeyEncrypted }"
                  placeholder="sk-..."
                />
                <button class="field__toggle-visibility" @click="showApiKey = !showApiKey">
                  <span class="material-symbols-outlined">
                    {{ showApiKey ? 'visibility_off' : 'visibility' }}
                  </span>
                </button>
              </div>
            </div>
          </div>

          <!-- Test result / error feedback (directly under Model/API Key inputs) -->
          <div
            v-if="testResult"
            class="provider-card__test-result"
            :class="{ 'provider-card__test-result--success': testResult.success }"
          >
            <template v-if="testResult.success">
              {{ testResult.message }}
            </template>
            <template v-else>
              <!-- Matched error: show inline solution with collapsible raw response -->
              <div v-if="llmErrorInfo.matched" class="provider-card__error-block">
                <div class="provider-card__error-solution">
                  <div class="provider-card__solution-header">
                    <span class="material-symbols-outlined provider-card__solution-icon"
                      >checklist</span
                    >
                    <strong>AI Configuration Problem (this is generally not a bug)</strong>
                  </div>
                  <p class="provider-card__solution-cause">
                    <span class="provider-card__solution-label">Cause:</span>
                    {{ llmErrorInfo.cause }}
                  </p>
                  <p class="provider-card__solution-text">
                    <span class="provider-card__solution-label">Solution:</span>
                    {{ llmErrorInfo.solution }}
                  </p>
                </div>
                <button class="provider-card__raw-toggle" @click="showRawError = !showRawError">
                  <span class="material-symbols-outlined" style="font-size: 16px">
                    {{ showRawError ? 'expand_less' : 'expand_more' }}
                  </span>
                  {{ showRawError ? 'Hide raw response' : 'Show raw LLM response' }}
                </button>
                <div v-if="showRawError" class="provider-card__error-details">
                  {{ llmErrorInfo.details }}
                </div>
                <a class="provider-card__error-link" :href="llmErrorInfo.helpLink">
                  <span class="material-symbols-outlined" style="font-size: 14px; margin-right: 4px"
                    >open_in_new</span
                  >
                  View in Troubleshooting Guide
                </a>
              </div>
              <!-- Unmatched error: show raw response directly -->
              <div v-else class="provider-card__error-block">
                <p class="provider-card__error-prefix">{{ llmErrorInfo.prefix }}</p>
                <p class="provider-card__error-details">{{ llmErrorInfo.details }}</p>
                <a class="provider-card__error-link" :href="llmErrorInfo.helpLink">
                  <span class="material-symbols-outlined" style="font-size: 14px; margin-right: 4px"
                    >open_in_new</span
                  >
                  View Troubleshooting Guide
                </a>
              </div>
            </template>
          </div>

          <!-- Provider-specific help link -->
          <div class="provider-card__help-link-row">
            <a class="provider-card__help-link" @click.prevent="goToProviderHelp">
              <span class="material-symbols-outlined" style="font-size: 14px; margin-right: 4px"
                >help_outline</span
              >
              {{ isLocalProvider() ? 'Setting up local AI' : 'How to get free limited API access' }}
            </a>
          </div>
        </div>
      </div>

      <!-- Parameters -->
      <div class="provider-card__sub provider-card__sub--params">
        <h3 class="provider-card__sub-title">
          <span class="material-symbols-outlined text-primary">tune</span>
          Parameters
        </h3>
        <div class="params-group">
          <!-- Max Context Tokens -->
          <div class="field">
            <div class="field__header">
              <label class="field__label">Max Context Tokens</label>
              <span
                v-if="!editingContextTokens"
                class="field__value-badge field__value-badge--editable"
                @click="startEditingContextTokens"
                >{{ config.contextWindowTokens.toLocaleString() }}</span
              >
              <input
                v-else
                ref="contextTokensInputRef"
                v-model="contextTokensInput"
                type="text"
                class="field__value-input"
                @blur="commitContextTokens"
                @keydown.enter="commitContextTokens"
                @keydown.escape="editingContextTokens = false"
              />
            </div>
            <input
              v-model.number="config.contextWindowTokens"
              type="range"
              class="field__range"
              :min="CONTEXT_MIN_FLOOR"
              :max="contextSliderMax"
              step="1000"
            />
            <div class="field__range-labels">
              <span>{{ formatCompact(CONTEXT_MIN_FLOOR) }}</span>
              <span>{{ formatCompact(contextSliderMax) }}</span>
            </div>
          </div>

          <!-- Concurrency -->
          <div class="field">
            <label class="field__label">Concurrency Threads</label>
            <div class="field__inline">
              <input
                v-model.number="config.maxConcurrentRequests"
                type="number"
                class="field__input field__input--small field__input--mono"
                min="1"
                max="16"
              />
              <span class="field__hint">Parallel requests</span>
            </div>
          </div>

          <!-- Request Delay -->
          <div class="field">
            <label class="field__label">Request Delay (ms)</label>
            <div class="field__inline">
              <input
                v-model.number="config.requestDelayMs"
                type="number"
                class="field__input field__input--medium field__input--mono"
                min="0"
                max="5000"
                step="100"
              />
              <span class="field__hint">Rate limiting</span>
            </div>
          </div>

          <!-- Temperature -->
          <div class="field">
            <label class="field__label">Temperature ({{ config.temperature.toFixed(1) }})</label>
            <input
              v-model.number="config.temperature"
              type="range"
              class="field__range"
              min="0"
              max="1"
              step="0.1"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- Action row: NO horizontal divider line above (spacing-only separation) -->
    <div class="provider-card__actions-row">
      <div class="provider-card__status">
        <span v-if="saving" class="material-symbols-outlined provider-card__status-spinner"
          >progress_activity</span
        >
        <span
          v-else
          class="provider-card__status-dot"
          :class="testResult?.success ? 'provider-card__status-dot--ok' : ''"
        ></span>
        <span class="provider-card__status-label">
          {{ saving ? 'Saving…' : testResult?.success ? 'Connection Succeeded' : 'Not Tested' }}
        </span>
      </div>
      <div class="provider-card__actions">
        <button class="btn btn--secondary" @click="revert">Revert</button>
        <button
          class="btn btn--secondary"
          :disabled="fetchingModels || !canFetchModels"
          @click="fetchModels"
        >
          <span v-if="fetchingModels" class="material-symbols-outlined btn__icon spinner"
            >progress_activity</span
          >
          <span v-else class="material-symbols-outlined btn__icon">cloud_download</span>
          {{ fetchingModels ? 'Fetching...' : 'Get Models' }}
        </button>
        <button
          class="btn btn--primary"
          :disabled="testing || !canTestConnection"
          @click="testConnection"
        >
          <span v-if="testing" class="material-symbols-outlined btn__icon spinner"
            >progress_activity</span
          >
          <span v-else class="material-symbols-outlined btn__icon">cable</span>
          {{ testing ? 'Testing...' : 'Test Connection' }}
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.provider-card {
  background-color: var(--color-surface-container-lowest, #ffffff);
  border-radius: var(--radius-xl, 0.75rem);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  padding: 1.5rem;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}

.provider-card__header {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid var(--color-surface-variant, #e4e1ee);
  margin-bottom: 1.5rem;
}

.provider-card__title {
  font-size: 18px;
  line-height: 24px;
  font-weight: 600;
  color: var(--color-on-surface, #1b1b24);
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.provider-card__subtitle {
  font-size: 13px;
  line-height: 18px;
  color: var(--color-on-surface-variant, #464555);
}

/* Warning (inside box) */
.provider-card__warning {
  background-color: #fefce8;
  border: 1px solid #fde68a;
  border-radius: var(--radius-lg, 0.5rem);
  padding: 0.75rem 1rem;
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  margin-bottom: 1.5rem;
}

.provider-card__warning-icon {
  color: #ca8a04;
  margin-top: 2px;
}

.provider-card__warning-text {
  font-size: 13px;
  line-height: 18px;
  color: #854d0e;
}

/* Two-column layout: connection | parameters */
.provider-card__grid {
  display: grid;
  grid-template-columns: 2fr 1fr;
  gap: 1.5rem;
}

@media (max-width: 768px) {
  .provider-card__grid {
    grid-template-columns: 1fr;
  }
}

/* Borderless sub-sections (no card-in-card chrome) */
.provider-card__sub {
  display: flex;
  flex-direction: column;
}

.provider-card__sub--params {
  background-color: var(--color-surface-container-low, #f5f2ff);
  border-radius: var(--radius-lg, 0.5rem);
  padding: 1.25rem;
}

.provider-card__sub-title {
  font-size: 14px;
  line-height: 20px;
  font-weight: 600;
  color: var(--color-on-surface, #1b1b24);
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding-bottom: 0.625rem;
  border-bottom: 1px solid var(--color-surface-variant, #e4e1ee);
  margin-bottom: 1.25rem;
}

/* Action row - NO border-top (spacing only, per request) */
.provider-card__actions-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 1.5rem;
  padding-top: 1.5rem;
}

.provider-card__status {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.375rem 0.75rem;
  background-color: rgba(228, 225, 238, 0.5);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: var(--radius-full, 9999px);
}

.provider-card__status-dot {
  width: 8px;
  height: 8px;
  border-radius: 9999px;
  background-color: var(--color-outline, #777587);
}

.provider-card__status-dot--ok {
  background-color: #16a34a;
}

.provider-card__status-spinner {
  font-size: 14px;
  color: var(--color-primary, #4f46e5);
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.provider-card__status-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--color-on-surface-variant, #464555);
}

.provider-card__actions {
  display: flex;
  gap: 0.75rem;
}

@media (max-width: 767px) {
  .provider-card__actions-row {
    flex-direction: column;
    gap: var(--space-4, 16px);
    align-items: stretch;
  }

  .provider-card__actions {
    justify-content: flex-end;
  }
}

/* Test Result */
.provider-card__test-result {
  padding: 0.75rem 1rem;
  background-color: #fef2f2;
  color: #991b1b;
  border-radius: var(--radius-lg, 0.5rem);
  font-size: 14px;
  line-height: 20px;
}

.provider-card__test-result--success {
  background-color: #f0fdf4;
  color: #166534;
}

/* LLM Error Block */
.provider-card__error-block {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.provider-card__error-prefix {
  font-weight: 500;
  margin: 0;
}

.provider-card__error-details {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  font-size: 12px;
  background-color: rgba(153, 27, 27, 0.08);
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
  margin: 0;
  word-break: break-word;
}

.provider-card__error-link {
  display: inline-flex;
  align-items: center;
  color: var(--color-primary-container, #4f46e5);
  font-weight: 500;
  text-decoration: none;
  font-size: 13px;
}

.provider-card__error-link:hover {
  text-decoration: underline;
}

/* Inline solution for matched errors */
.provider-card__error-solution {
  background-color: #fffbeb;
  border: 1px solid #fde68a;
  border-radius: 0.375rem;
  padding: 0.625rem 0.75rem;
}

.provider-card__solution-header {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  margin-bottom: 0.375rem;
  font-size: 13px;
  color: #92400e;
}

.provider-card__solution-icon {
  font-size: 18px;
  color: #d97706;
}

.provider-card__solution-cause,
.provider-card__solution-text {
  font-size: 13px;
  line-height: 18px;
  color: #78350f;
  margin: 0 0 0.25rem 0;
}

.provider-card__solution-label {
  font-weight: 600;
  color: #92400e;
}

.provider-card__raw-toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  background: none;
  border: none;
  color: #991b1b;
  font-size: 12px;
  cursor: pointer;
  padding: 0;
  font-family: inherit;
  opacity: 0.8;
}

.provider-card__raw-toggle:hover {
  opacity: 1;
  text-decoration: underline;
}

/* Provider-specific help link (below test-result/error feedback) */
.provider-card__help-link-row {
  display: flex;
  justify-content: center;
  margin-top: 0.75rem;
}

.provider-card__help-link {
  display: inline-flex;
  align-items: center;
  color: var(--color-primary, #4f46e5);
  font-weight: 500;
  font-size: 13px;
  text-decoration: none;
  cursor: pointer;
}

.provider-card__help-link:hover {
  text-decoration: underline;
}
</style>
