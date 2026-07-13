<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '@/composables/use-toast';

interface OpenAlexSettings {
  hasApiKey: boolean;
  mailto: string;
  retrieveReferences: boolean;
}

const toast = useToast();
const settings = ref<OpenAlexSettings | null>(null);
const apiKeyInput = ref('');
const mailtoInput = ref('');
const retrieveReferences = ref(false);
const loading = ref(false);
const error = ref<string | null>(null);

async function loadSettings(): Promise<void> {
  try {
    settings.value = await invoke<OpenAlexSettings>('get_openalex_settings');
    mailtoInput.value = settings.value.mailto;
    retrieveReferences.value = settings.value.retrieveReferences;
    // Don't pre-fill the API key field for security; show a placeholder if set.
    apiKeyInput.value = '';
  } catch (e) {
    error.value = String(e);
  }
}

async function saveApiKey(): Promise<void> {
  if (!apiKeyInput.value.trim()) return;
  loading.value = true;
  try {
    await invoke('set_openalex_settings', {
      settings: { apiKey: apiKeyInput.value.trim() },
    });
    toast.show('OpenAlex API key saved.', 'success');
    apiKeyInput.value = '';
    await loadSettings();
  } catch (e) {
    toast.show(`Failed to save API key: ${e}`, 'error');
  } finally {
    loading.value = false;
  }
}

async function clearApiKey(): Promise<void> {
  loading.value = true;
  try {
    await invoke('set_openalex_settings', { settings: { apiKey: '' } });
    toast.show('OpenAlex API key cleared.', 'success');
    await loadSettings();
  } catch (e) {
    toast.show(`Failed to clear API key: ${e}`, 'error');
  } finally {
    loading.value = false;
  }
}

async function saveMailto(): Promise<void> {
  loading.value = true;
  try {
    await invoke('set_openalex_settings', {
      settings: { mailto: mailtoInput.value.trim() },
    });
    toast.show('Email saved.', 'success');
    await loadSettings();
  } catch (e) {
    toast.show(`Failed to save email: ${e}`, 'error');
  } finally {
    loading.value = false;
  }
}

async function toggleRetrieveReferences(): Promise<void> {
  const newValue = !retrieveReferences.value;
  retrieveReferences.value = newValue;
  try {
    await invoke('set_openalex_settings', { settings: { retrieveReferences: newValue } });
    toast.show(
      newValue ? 'Reference details harvest enabled.' : 'Reference details harvest disabled.',
      'success'
    );
    await loadSettings();
  } catch (e) {
    // Revert on failure
    retrieveReferences.value = !newValue;
    toast.show(`Failed to update setting: ${e}`, 'error');
  }
}

onMounted(() => {
  void loadSettings();
});
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">travel_explore</span>
      OpenAlex Search
    </h2>
    <p class="settings-card__desc">
      Configure the OpenAlex catalog search integration. OpenAlex is free and open; the optional API
      key raises the rate limit from 10 to 100 requests per second.
    </p>

    <div v-if="error" class="settings-card__status error">{{ error }}</div>

    <div v-if="settings" class="openalex-settings">
      <!-- API Key -->
      <div class="field-group">
        <label class="field-label">API Key (optional)</label>
        <div class="api-key-row">
          <input
            v-model="apiKeyInput"
            type="password"
            class="field-input"
            :placeholder="
              settings.hasApiKey
                ? 'API key is set. Enter a new key to replace.'
                : 'Enter your OpenAlex API key'
            "
          />
          <button
            class="btn btn--secondary"
            :disabled="loading || !apiKeyInput.trim()"
            @click="saveApiKey"
          >
            Save
          </button>
          <button
            v-if="settings.hasApiKey"
            class="btn btn--secondary"
            :disabled="loading"
            @click="clearApiKey"
          >
            Clear
          </button>
        </div>
        <p v-if="settings.hasApiKey" class="field-status field-status--ok">
          <span class="material-symbols-outlined">check_circle</span>
          API key is configured
        </p>
      </div>

      <!-- Email (mailto) -->
      <div class="field-group">
        <label class="field-label">Email (for polite pool)</label>
        <div class="mailto-row">
          <input
            v-model="mailtoInput"
            type="email"
            class="field-input"
            placeholder="your-email@university.edu"
          />
          <button class="btn btn--secondary" :disabled="loading" @click="saveMailto">Save</button>
        </div>
        <p class="field-hint">
          Sent with every OpenAlex request for the polite-pool rate limit. Defaults to
          <code>research@bango.app</code> if not set.
        </p>
      </div>

      <!-- Retrieve Reference Details toggle -->
      <div class="field-group">
        <div class="toggle-row">
          <label class="toggle-label">Retrieve Reference Details</label>
          <button
            class="toggle-switch"
            :class="{ 'toggle-switch--on': retrieveReferences }"
            :disabled="loading"
            @click="toggleRetrieveReferences"
          >
            <span class="toggle-switch__thumb"></span>
          </button>
        </div>
        <p class="field-hint field-hint--warning">
          <span class="material-symbols-outlined">warning</span>
          When enabled, importing an article from OpenAlex will batch-fetch its referenced works
          metadata (up to 50 per call). This may trigger rate limits for articles with many
          references.
        </p>
      </div>
    </div>
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';

.openalex-settings {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.field-group {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.field-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-on-surface, #1b1b24);
}

.field-input {
  flex: 1;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: 0.375rem;
  font-size: 14px;
  background-color: var(--color-surface, #ffffff);
  color: var(--color-on-surface, #1b1b24);
  outline: none;
  transition: border-color 0.15s;
}

.field-input:focus {
  border-color: var(--color-primary, #3525cd);
}

.api-key-row,
.mailto-row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.field-hint {
  font-size: 12px;
  color: var(--color-outline, #777587);
  line-height: 1.4;
}

.field-hint code {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  font-size: 11px;
  background-color: var(--color-surface-container-low, #f5f2ff);
  padding: 0.0625rem 0.375rem;
  border-radius: 0.25rem;
}

.field-hint--warning {
  display: flex;
  align-items: flex-start;
  gap: 0.25rem;
  color: #92400e;
}

.field-hint--warning .material-symbols-outlined {
  font-size: 16px;
  flex-shrink: 0;
  margin-top: 0.0625rem;
}

.field-status {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 12px;
}

.field-status--ok {
  color: #16a34a;
}

.field-status .material-symbols-outlined {
  font-size: 16px;
}

/* Toggle switch */
.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.toggle-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-on-surface, #1b1b24);
}

.toggle-switch {
  position: relative;
  width: 40px;
  height: 22px;
  border-radius: 9999px;
  border: none;
  background-color: var(--color-surface-variant, #e4e1ee);
  cursor: pointer;
  transition: background-color 0.2s;
  flex-shrink: 0;
}

.toggle-switch--on {
  background-color: var(--color-primary, #3525cd);
}

.toggle-switch__thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  border-radius: 9999px;
  background-color: #ffffff;
  transition: transform 0.2s;
}

.toggle-switch--on .toggle-switch__thumb {
  transform: translateX(18px);
}

.error {
  color: #991b1b;
  background-color: #fef2f2;
}

.btn {
  padding: 0.5rem 1rem;
  font-size: 13px;
  font-weight: 500;
  border-radius: 0.375rem;
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  background-color: var(--color-surface, #ffffff);
  color: var(--color-on-surface, #1b1b24);
  cursor: pointer;
  transition: background-color 0.15s;
  white-space: nowrap;
}

.btn:hover:not(:disabled) {
  background-color: var(--color-surface-container-low, #f5f2ff);
}

.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.btn--secondary {
  background-color: var(--color-surface-container-low, #f5f2ff);
}
</style>
