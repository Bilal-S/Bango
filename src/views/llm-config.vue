<script setup lang="ts">
import { watch, ref, computed } from 'vue';
import { useLlmConfig } from '@/composables/use-llm-config';
import { useExport } from '@/composables/use-export';

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

const { exportProject, importProject, resetProject } = useExport();

// Project management state
const showImportDialog = ref(false);
const showExportDialog = ref(false);
const showDeleteDialog = ref(false);
const deleteConfirmText = ref('');
const importFile = ref<File | null>(null);

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
}

async function doExportProject(): Promise<void> {
  await exportProject();
  showExportDialog.value = false;
}

async function doDeleteProject(): Promise<void> {
  if (deleteConfirmText.value !== 'DELETE') return;
  await resetProject();
  showDeleteDialog.value = false;
  deleteConfirmText.value = '';
}

const providerDefaults: Record<string, { url: string; models: string[] }> = {
  openai: {
    url: 'https://api.openai.com/v1',
    models: [
      'gpt-5-mini',
      'gpt-5',
      'gpt-4.1-latest',
      'gpt-5-chat-latest',
      'gpt-5.3-codex',
      'gpt-5.4',
      'gpt-5.4-pro',
      'gpt-5.4-mini',
      'gpt-5.4-nano',
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
  mistral_ai: {
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
  z_ai: {
    url: 'https://api.z.ai/api/paas/v4',
    models: ['glm-5-turbo', 'glm-4.7-flash', 'glm-5', 'glm-5.1'],
  },
  ollama: {
    url: 'http://localhost:11434/v1',
    models: [],
  },
  lm_studio: {
    url: 'http://localhost:1234/v1',
    models: [],
  },
  llama_cpp: {
    url: 'http://localhost:8080/v1',
    models: [],
  },
};

const isOtherModel = ref(false);

const availableModels = computed(() => {
  if (fetchedModels.value && fetchedModels.value.length > 0) {
    return fetchedModels.value;
  }
  return providerDefaults[config.value.provider]?.models || [];
});

const canFetchModels = computed(() => {
  if (isLocalProvider()) return true;
  return !!config.value.apiKeyEncrypted;
});

watch(
  () => config.value.provider,
  (newProvider, oldProvider) => {
    if (oldProvider && newProvider !== oldProvider) {
      resetFetchedModels();
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
                <option value="mistral_ai">Mistral AI</option>
                <option value="z_ai">z.ai</option>
                <option value="llama_cpp">llama.cpp</option>
                <option value="ollama">Ollama</option>
                <option value="lm_studio">LM Studio</option>
                <option value="custom">Custom</option>
              </select>
              <span class="material-symbols-outlined field__select-arrow">expand_more</span>
            </div>
          </div>

          <!-- Endpoint URL -->
          <div class="field">
            <label class="field__label">Endpoint URL</label>
            <input
              v-model="config.endpointUrl"
              type="url"
              class="field__input field__input--mono"
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
          {{ testResult?.success ? 'Connected' : 'Disconnected' }}
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
        <button class="btn btn--primary" :disabled="testing" @click="testConnection">
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
      {{ testResult.message }}
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
            :disabled="deleteConfirmText !== 'DELETE'"
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
</style>
