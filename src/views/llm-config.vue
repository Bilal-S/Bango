<script setup lang="ts">
import { watch, ref, computed } from 'vue';
import { useRouter } from 'vue-router';
import { useLlmConfig } from '@/composables/use-llm-config';
import { useExport } from '@/composables/use-export';
import { formatLlmError } from '@/utils/llm-error';
import { invoke } from '@tauri-apps/api/core';

const {
  config,
  testing,
  testResult,
  showApiKey,
  fetchingModels,
  fetchedModels,
  testConnection,
  revert,
  isLocalProvider,
  fetchModels,
  resetFetchedModels,
} = useLlmConfig();

const { error, exportProject, importProject, resetProject } = useExport();
const router = useRouter();

// Project management state
const showImportDialog = ref(false);
const showExportDialog = ref(false);
const showDeleteDialog = ref(false);
const deleteConfirmText = ref('');
const importFile = ref<File | null>(null);
const clearLogsStatus = ref<string | null>(null);
const showErrorLog = ref(false);
const errorLogEntries = ref<Array<{ id: string; timestamp: string; details: string | null }>>([]);
const errorLogLoading = ref(false);

function handleImportFile(event: Event): void {
  const target = event.target as HTMLInputElement;
  if (target.files?.length) {
    importFile.value = target.files[0] ?? null;
  }
}

async function doImportProject(): Promise<void> {
  if (!importFile.value) return;
  await importProject(importFile.value);
  showImportDialog.value = false;
  importFile.value = null;
  // Navigate to dashboard so all views refresh with newly imported data
  if (!error.value) {
    router.push('/');
  }
}

async function doExportProject(): Promise<void> {
  await exportProject();
  showExportDialog.value = false;
}

async function doShowErrorLog(): Promise<void> {
  if (showErrorLog.value) {
    showErrorLog.value = false;
    return;
  }
  errorLogLoading.value = true;
  try {
    const entries = await invoke<Array<{ id: string; timestamp: string; details: string | null }>>(
      'get_generic_audit_entries',
      { limit: 10 }
    );
    errorLogEntries.value = entries;
    showErrorLog.value = true;
  } catch (e) {
    errorLogEntries.value = [];
  } finally {
    errorLogLoading.value = false;
  }
}

async function doClearSystemLogs(): Promise<void> {
  try {
    const count = await invoke<number>('clear_generic_audit');
    clearLogsStatus.value = `Cleared ${count} system log entry(ies).`;
    setTimeout(() => {
      clearLogsStatus.value = null;
    }, 3000);
  } catch (e) {
    clearLogsStatus.value = `Failed to clear logs: ${e}`;
  }
}

async function doDeleteProject(): Promise<void> {
  if (deleteConfirmText.value.toUpperCase() !== 'DELETE') return;
  const success = await resetProject();
  if (success) {
    showDeleteDialog.value = false;
    deleteConfirmText.value = '';
    router.push('/');
  }
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
    return { prefix: '', details: '', helpLink: '', matched: false, anchorId: null };
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
</script>

<template>
  <div class="llm-config">
    <!-- Header -->
    <div class="llm-config__header">
      <h1 class="page-title">LLM Configuration</h1>
      <p class="llm-config__subtitle">
        Configure the Large Language Model endpoint and parameters for text generation tasks.
      </p>
    </div>

    <!-- Warning Banner -->
    <div v-if="isLocalProvider()" class="llm-config__warning">
      <span class="material-symbols-outlined llm-config__warning-icon">warning</span>
      <p class="llm-config__warning-text">
        <strong>Hardware Requirement Notice:</strong> Local providers require 16+ GB VRAM for 50k
        token context to maintain stable inference speeds.
      </p>
    </div>

    <!-- Main Form: Bento Layout -->
    <div class="llm-config__grid">
      <!-- Connection Details (spans 2 columns) -->
      <div class="llm-config__card llm-config__card--wide">
        <h2 class="llm-config__card-title">
          <span class="material-symbols-outlined text-primary">dns</span>
          Connection Details
        </h2>
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
        </div>
      </div>

      <!-- Parameters (spans 1 column) -->
      <div class="llm-config__card llm-config__card--narrow">
        <h2 class="llm-config__card-title">
          <span class="material-symbols-outlined text-primary">tune</span>
          Parameters
        </h2>
        <div class="params-group">
          <!-- Max Context Tokens -->
          <div class="field">
            <div class="field__header">
              <label class="field__label">Max Context Tokens</label>
              <span class="field__value-badge">{{
                config.contextWindowTokens.toLocaleString()
              }}</span>
            </div>
            <input
              v-model.number="config.contextWindowTokens"
              type="range"
              class="field__range"
              min="1000"
              max="50000"
              step="1000"
            />
            <div class="field__range-labels">
              <span>1k</span>
              <span>50k</span>
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

    <!-- Footer Actions -->
    <div class="llm-config__footer">
      <div class="llm-config__status">
        <span
          class="llm-config__status-dot"
          :class="testResult?.success ? 'llm-config__status-dot--ok' : ''"
        ></span>
        <span class="llm-config__status-label">
          {{ testResult?.success ? 'Connection Succeeded' : 'Not Tested' }}
        </span>
      </div>
      <div class="llm-config__actions">
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

    <!-- Test Result -->
    <div
      v-if="testResult"
      class="llm-config__test-result"
      :class="{ 'llm-config__test-result--success': testResult.success }"
    >
      <template v-if="testResult.success">
        {{ testResult.message }}
      </template>
      <template v-else>
        <div class="llm-config__error-block">
          <p class="llm-config__error-prefix">{{ llmErrorInfo.prefix }}</p>
          <p class="llm-config__error-details">{{ llmErrorInfo.details }}</p>
          <a class="llm-config__error-link" :href="llmErrorInfo.helpLink">
            <span class="material-symbols-outlined" style="font-size: 14px; margin-right: 4px"
              >open_in_new</span
            >
            View Troubleshooting Guide
          </a>
        </div>
      </template>
    </div>

    <!-- Project Management -->
    <div class="llm-config__card pm-card" style="margin-top: 2rem">
      <h2 class="llm-config__card-title">
        <span class="material-symbols-outlined text-primary">settings_backup_restore</span>
        Project Management
      </h2>
      <p class="pm-card__desc">Import, export, or reset your project data.</p>
      <div class="pm-card__actions">
        <button class="btn btn--secondary" @click="showImportDialog = true">
          <span class="material-symbols-outlined btn__icon">upload_file</span>
          Import Backup
        </button>
        <button class="btn btn--secondary" @click="showExportDialog = true">
          <span class="material-symbols-outlined btn__icon">download</span>
          Export Backup
        </button>
        <button class="btn btn--danger" @click="showDeleteDialog = true">
          <span class="material-symbols-outlined btn__icon">delete_forever</span>
          Delete All Data
        </button>
      </div>
    </div>

    <!-- Diagnostics -->
    <div class="llm-config__card pm-card" style="margin-top: 1rem">
      <h2 class="llm-config__card-title">
        <span class="material-symbols-outlined text-primary">troubleshoot</span>
        Diagnostics
      </h2>
      <p class="pm-card__desc">View recent system errors and diagnostic information.</p>
      <p v-if="clearLogsStatus" class="pm-card__status">{{ clearLogsStatus }}</p>
      <div class="pm-card__actions">
        <button class="btn btn--secondary" :disabled="errorLogLoading" @click="doShowErrorLog">
          <span v-if="errorLogLoading" class="material-symbols-outlined btn__icon spinner"
            >progress_activity</span
          >
          <span v-else class="material-symbols-outlined btn__icon">{{
            showErrorLog ? 'visibility_off' : 'bug_report'
          }}</span>
          {{ showErrorLog ? 'Hide Errors' : 'Show Last 10 Errors' }}
        </button>
        <button class="btn btn--secondary" @click="doClearSystemLogs">
          <span class="material-symbols-outlined btn__icon">mop</span>
          Clear System Logs
        </button>
      </div>

      <!-- Error log entries -->
      <div v-if="showErrorLog" class="error-log">
        <p v-if="errorLogEntries.length === 0" class="error-log__empty">
          No system errors recorded.
        </p>
        <div v-for="entry in errorLogEntries" :key="entry.id" class="error-log__entry">
          <span class="error-log__time">{{ entry.timestamp }}</span>
          <span class="error-log__details">{{ entry.details }}</span>
        </div>
      </div>
    </div>

    <!-- Import Dialog -->
    <div v-if="showImportDialog" class="dialog-overlay" @click.self="showImportDialog = false">
      <div class="dialog">
        <h2>Import Project Backup</h2>
        <p class="dialog__desc">Select a <code>.bango.json</code> file to restore your project.</p>
        <div class="field">
          <label class="field__label">Backup File</label>
          <input
            type="file"
            accept=".bango.json,.json"
            class="field__input"
            @change="handleImportFile"
          />
        </div>
        <div class="dialog__actions">
          <button class="btn btn--outline" @click="showImportDialog = false">Cancel</button>
          <button class="btn btn--primary" :disabled="!importFile" @click="doImportProject">
            Import
          </button>
        </div>
      </div>
    </div>

    <!-- Export Dialog -->
    <div v-if="showExportDialog" class="dialog-overlay" @click.self="showExportDialog = false">
      <div class="dialog">
        <h2>Export Project Backup</h2>
        <p class="dialog__desc">
          Export your project data to a <code>.bango.json</code> file. Note: API keys are NOT
          included in the backup.
        </p>
        <div class="dialog__actions">
          <button class="btn btn--outline" @click="showExportDialog = false">Cancel</button>
          <button class="btn btn--primary" @click="doExportProject">Export Backup</button>
        </div>
      </div>
    </div>

    <!-- Delete Confirmation Dialog -->
    <div v-if="showDeleteDialog" class="dialog-overlay" @click.self="showDeleteDialog = false">
      <div class="dialog dialog--danger">
        <h2>Delete All Project Data</h2>
        <div class="dialog__danger-box">
          <span class="material-symbols-outlined">warning</span>
          <p>
            This will permanently delete
            <strong>all articles, criteria, tags, labels, and settings</strong>. This action cannot
            be undone.
          </p>
        </div>
        <div class="field">
          <label class="field__label">Type DELETE to confirm</label>
          <input
            v-model="deleteConfirmText"
            type="text"
            class="field__input"
            placeholder="DELETE"
          />
        </div>
        <div class="dialog__actions">
          <button
            class="btn btn--outline"
            @click="
              showDeleteDialog = false;
              deleteConfirmText = '';
            "
          >
            Cancel
          </button>
          <button
            class="btn btn--danger"
            :disabled="deleteConfirmText.toUpperCase() !== 'DELETE'"
            @click="doDeleteProject"
          >
            Delete Everything
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.llm-config {
  padding: var(--container-padding);
  max-width: 56rem;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .llm-config {
    padding: var(--container-padding-sm);
  }

  .field-row {
    grid-template-columns: 1fr;
  }

  .llm-config__footer {
    flex-direction: column;
    gap: var(--space-4);
    align-items: stretch;
  }

  .llm-config__actions {
    justify-content: flex-end;
  }
}

.llm-config__header {
  margin-bottom: 1.5rem;
}

.llm-config__title {
  font-size: 24px;
  line-height: 32px;
  font-weight: 600;
  letter-spacing: -0.02em;
  color: #1b1b24;
  margin-bottom: 0.5rem;
}

.llm-config__subtitle {
  font-size: 14px;
  line-height: 20px;
  color: #464555;
}

/* Warning */
.llm-config__warning {
  background-color: #fefce8;
  border: 1px solid #fde68a;
  border-radius: 0.5rem;
  padding: 1rem;
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  margin-bottom: 1.5rem;
}

.llm-config__warning-icon {
  color: #ca8a04;
  margin-top: 2px;
}

.llm-config__warning-text {
  font-size: 13px;
  line-height: 18px;
  color: #854d0e;
}

/* Grid */
.llm-config__grid {
  display: grid;
  grid-template-columns: 2fr 1fr;
  gap: 1.5rem;
}

@media (max-width: 768px) {
  .llm-config__grid {
    grid-template-columns: 1fr;
  }
}

/* Cards */
.llm-config__card {
  background-color: #ffffff;
  border-radius: 0.75rem;
  border: 1px solid #e4e1ee;
  padding: 1.5rem;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}

.llm-config__card--narrow {
  background-color: #f5f2ff;
}

.llm-config__card-title {
  font-size: 16px;
  line-height: 24px;
  font-weight: 600;
  color: #1b1b24;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid #e4e1ee;
  margin-bottom: 1.5rem;
}

/* Field */
.field-group {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.params-group {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.field__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.5rem;
}

.field__label {
  font-size: 11px;
  line-height: 16px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: #464555;
}

.field__input {
  width: 100%;
  background-color: #ffffff;
  border: 1px solid #c7c4d8;
  border-radius: 0.5rem;
  padding: 0.625rem 1rem;
  font-size: 14px;
  line-height: 20px;
  color: #1b1b24;
  outline: none;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
}

.field__input:focus {
  border-color: #3525cd;
  box-shadow: 0 0 0 1px #3525cd;
}

.field__input--mono {
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 13px;
}

.field__input--small {
  width: 5rem;
  text-align: center;
}

.field__input--medium {
  width: 6rem;
  text-align: center;
}

.field__input--password {
  padding-right: 2.5rem;
}

/* Required-field halo (pulsing amber glow for empty required fields) */
.field__input--required-halo {
  border-color: #d97706;
  animation: required-halo-pulse 2s ease-in-out infinite;
}

@keyframes required-halo-pulse {
  0%,
  100% {
    box-shadow: 0 0 0 2px rgba(217, 119, 6, 0.25);
  }
  50% {
    box-shadow: 0 0 0 3px rgba(217, 119, 6, 0.45);
  }
}

.field__input--required-halo:focus {
  border-color: #3525cd;
  box-shadow: 0 0 0 1px #3525cd;
  animation: none;
}

/* Required-field halo for select elements */
.field__select--required-halo {
  border-color: #d97706;
  animation: required-halo-pulse 2s ease-in-out infinite;
}

.field__select--required-halo:focus {
  border-color: #3525cd;
  box-shadow: 0 0 0 1px #3525cd;
  animation: none;
}

.field__password-wrapper {
  position: relative;
}

.field__toggle-visibility {
  position: absolute;
  right: 0.75rem;
  top: 50%;
  transform: translateY(-50%);
  background: none;
  border: none;
  cursor: pointer;
  color: #777587;
  padding: 0.125rem;
  transition: color 0.15s;
}

.field__toggle-visibility:hover {
  color: #1b1b24;
}

/* Select */
.field__select-wrapper {
  position: relative;
}

.field__select {
  width: 100%;
  appearance: none;
  background-color: #ffffff;
  border: 1px solid #c7c4d8;
  border-radius: 0.5rem;
  padding: 0.625rem 2.5rem 0.625rem 1rem;
  font-size: 14px;
  line-height: 20px;
  color: #1b1b24;
  outline: none;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
  cursor: pointer;
}

.field__select:focus {
  border-color: #3525cd;
  box-shadow: 0 0 0 1px #3525cd;
}

.field__select-arrow {
  position: absolute;
  right: 0.75rem;
  top: 50%;
  transform: translateY(-50%);
  color: #777587;
  pointer-events: none;
}

/* Range */
.field__range {
  width: 100%;
  height: 6px;
  background-color: #e4e1ee;
  border-radius: 0.5rem;
  appearance: none;
  cursor: pointer;
  accent-color: #3525cd;
}

.field__range-labels {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: #777587;
  margin-top: 0.25rem;
  font-family: ui-monospace, SFMono-Regular, monospace;
}

.field__value-badge {
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 13px;
  color: #0f0069;
  background-color: #e2dfff;
  padding: 0.125rem 0.5rem;
  border-radius: 0.25rem;
  font-weight: 500;
}

/* Inline fields */
.field__inline {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.field__hint {
  font-size: 13px;
  line-height: 18px;
  color: #464555;
}

.field-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1rem;
}

/* Footer */
.llm-config__footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-top: 1.5rem;
  border-top: 1px solid #e4e1ee;
  margin-top: 2rem;
}

.llm-config__status {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.375rem 0.75rem;
  background-color: rgba(228, 225, 238, 0.5);
  border: 1px solid #e4e1ee;
  border-radius: 9999px;
}

.llm-config__status-dot {
  width: 8px;
  height: 8px;
  border-radius: 9999px;
  background-color: #777587;
}

.llm-config__status-dot--ok {
  background-color: #16a34a;
}

.llm-config__status-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: #464555;
}

.llm-config__actions {
  display: flex;
  gap: 0.75rem;
}

/* Buttons */
.btn {
  padding: 0.625rem 1.5rem;
  border-radius: 0.5rem;
  font-size: 14px;
  line-height: 20px;
  font-weight: 500;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  transition:
    background-color 0.15s,
    opacity 0.15s;
  border: none;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn--primary {
  background-color: #3525cd;
  color: #ffffff;
}

.btn--primary:hover:not(:disabled) {
  background-color: #4f46e5;
}

.btn--secondary {
  border: 1px solid #c7c4d8;
  background-color: transparent;
  color: #1b1b24;
}

.btn--secondary:hover:not(:disabled) {
  background-color: #f0ecf9;
}

.btn__icon {
  font-size: 18px;
}

/* Test Result */
.llm-config__test-result {
  margin-top: 1rem;
  padding: 0.75rem 1rem;
  background-color: #fef2f2;
  color: #991b1b;
  border-radius: 0.5rem;
  font-size: 14px;
  line-height: 20px;
}

.llm-config__test-result--success {
  background-color: #f0fdf4;
  color: #166534;
}

/* LLM Error Block */
.llm-config__error-block {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.llm-config__error-prefix {
  font-weight: 500;
  margin: 0;
}

.llm-config__error-details {
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 12px;
  background-color: rgba(153, 27, 27, 0.08);
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
  margin: 0;
  word-break: break-word;
}

.llm-config__error-link {
  display: inline-flex;
  align-items: center;
  color: #4f46e5;
  font-weight: 500;
  text-decoration: none;
  font-size: 13px;
}

.llm-config__error-link:hover {
  text-decoration: underline;
}

.spinner {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* Project Management */
.pm-card__desc {
  font-size: 13px;
  color: #464555;
  margin-bottom: 1rem;
}

.pm-card__status {
  font-size: 13px;
  color: #166534;
  background-color: #f0fdf4;
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
  margin-bottom: 0.75rem;
}

.pm-card__actions {
  display: flex;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.btn--danger {
  background-color: #dc2626;
  color: #ffffff;
}

.btn--danger:hover:not(:disabled) {
  background-color: #b91c1c;
}

.btn--ghost {
  color: #464555;
  background: none;
  border: none;
}

.btn--ghost:hover:not(:disabled) {
  background-color: #f0ecf9;
}

.btn--outline {
  background: transparent;
  color: #464555;
  border: 1px solid #777587;
  border-radius: 0.5rem;
  padding: 0.625rem 1.5rem;
  font-size: 14px;
  line-height: 20px;
  font-weight: 500;
  cursor: pointer;
}

.btn--outline:hover:not(:disabled) {
  background-color: #f0ecf9;
}

/* Dialog */
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.dialog {
  background: white;
  padding: 1.5rem;
  border-radius: 0.75rem;
  width: 420px;
  max-width: 90vw;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.dialog h2 {
  font-size: 18px;
  font-weight: 600;
  color: #1b1b24;
}

.dialog__desc {
  font-size: 13px;
  color: #464555;
}

.dialog__desc code {
  background-color: #e2dfff;
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
  font-size: 12px;
}

.dialog__actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
}

.dialog__danger-box {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 0.75rem;
  background-color: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: 0.5rem;
}

.dialog__danger-box .material-symbols-outlined {
  color: #dc2626;
  margin-top: 2px;
}

.dialog__danger-box p {
  font-size: 13px;
  color: #991b1b;
}

/* Error Log */
.error-log {
  margin-top: 1rem;
  border: 1px solid #fecaca;
  border-radius: 0.5rem;
  overflow: hidden;
}

.error-log__empty {
  padding: 1rem;
  font-size: 13px;
  color: #464555;
  text-align: center;
}

.error-log__entry {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid #fecaca;
  background-color: #fef2f2;
}

.error-log__entry:last-child {
  border-bottom: none;
}

.error-log__time {
  font-size: 11px;
  color: #777587;
  font-family: ui-monospace, SFMono-Regular, monospace;
}

.error-log__details {
  font-size: 13px;
  color: #991b1b;
  line-height: 18px;
  word-break: break-word;
}
</style>
