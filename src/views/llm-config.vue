<script setup lang="ts">
import { useLlmConfig } from '@/composables/use-llm-config';

const { config, testing, testResult, showApiKey, testConnection, revert, isLocalProvider } =
  useLlmConfig();
</script>

<template>
  <div class="llm-config">
    <!-- Header -->
    <div class="llm-config__header">
      <h1 class="llm-config__title">LLM Configuration</h1>
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
                <option value="google">Google</option>
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
              <input
                v-model="config.modelName"
                type="text"
                class="field__input field__input--mono"
                placeholder="gpt-4-turbo"
              />
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
        <button class="btn btn--primary" :disabled="testing" @click="testConnection">
          <span class="material-symbols-outlined btn__icon">cable</span>
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
  </div>
</template>

<style scoped>
.llm-config {
  padding: 24px;
  max-width: 56rem;
  margin: 0 auto;
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

/* Utility */
.text-primary {
  color: #3525cd;
}

.material-symbols-outlined {
  font-variation-settings:
    'FILL' 0,
    'wght' 400,
    'GRAD' 0,
    'opsz' 24;
  vertical-align: middle;
}
</style>
