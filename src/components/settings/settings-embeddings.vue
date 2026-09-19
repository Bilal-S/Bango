<script setup lang="ts">
import { computed, ref, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { useLlmConfigStore } from '@/stores/llm-config';
import { useLocalEmbeddings, type EmbeddingBackendId } from '@/composables/use-local-embeddings';
import EmbeddingsConsentDialog from './embeddings-consent-dialog.vue';

const {
  backend,
  status,
  progress,
  verifyResult,
  error,
  loading,
  installing,
  verifying,
  removing,
  load,
  selectBackend,
  install,
  cancelInstall,
  verify,
  remove,
} = useLocalEmbeddings();

const router = useRouter();

/** Open the Help Reference Embeddings section (what embeddings do, provider options). */
function openEmbeddingsHelp(): void {
  router.push('/help?tab=reference#ref-embeddings');
}

const llmConfig = useLlmConfigStore();

/**
 * Providers with no embedding API - the frontend mirror of the backend's
 * static `check_embedding_support` override (`src-tauri/src/llm/embedding.rs`).
 * Keep in sync; keys use the store's serde camelCase provider ids.
 */
const EMBEDDING_UNSUPPORTED_PROVIDERS = new Set(['anthropic', 'zAi']);

/** Display labels mirroring the provider card's select options. */
const PROVIDER_LABELS: Record<string, string> = {
  openai: 'OpenAI',
  anthropic: 'Anthropic',
  google: 'Google Gemini',
  mistralAi: 'Mistral AI',
  zAi: 'Z.AI',
  llamaCpp: 'llama.cpp',
  ollama: 'Ollama',
  lmStudio: 'LM Studio',
  custom: 'Custom',
};

/**
 * The Configured Provider option state, derived LIVE from the currently
 * selected chat provider (saved or not): the moment the selection in the
 * provider card switches to a provider without an embedding API, this
 * disables the option and names that exact provider in the inline message.
 */
const selectedProvider = computed(() => llmConfig.config.provider);
const cloudOptionDisabled = computed(() =>
  EMBEDDING_UNSUPPORTED_PROVIDERS.has(selectedProvider.value)
);
const cloudOptionHint = computed(() =>
  cloudOptionDisabled.value
    ? `${PROVIDER_LABELS[selectedProvider.value] ?? selectedProvider.value} does not support embeddings.`
    : "Uses your AI provider's embedding API (requires an API connection)."
);

/** Whether the consent dialog is open (pending switch to Bango Local). */
const showConsent = ref(false);
/** Component Details expansion. */
const detailsOpen = ref(false);
/** A backend switch is persisting (radios disabled so it cannot double-fire). */
const switching = ref(false);
/** Confirm state for Remove (two-step destructive action). */
const confirmRemove = ref(false);
/** Transient error surfaced by card-level actions. */
const actionError = ref<string | null>(null);

onMounted(() => {
  void load();
});

const state = computed(() => status.value?.state ?? 'unknown');
const readyForUse = computed(
  () => state.value === 'ready' && (status.value?.runtimeReady ?? false)
);
const needsRepair = computed(
  () =>
    status.value !== null &&
    (state.value === 'repair_required' || (state.value === 'ready' && !status.value.runtimeReady))
);
/** Install button label doubles as repair. */
const installLabel = computed(() =>
  state.value === 'not_installed' ? 'Download' : 'Repair Installation'
);

/** Human state label for the status line. */
const stateLabel = computed(() => {
  switch (state.value) {
    case 'ready':
      return status.value?.runtimeReady === false
        ? 'Installed - runtime missing'
        : 'Installed and ready';
    case 'repair_required':
      return 'Needs repair';
    case 'installing':
      return 'Installing…';
    case 'unsupported':
      return 'Not supported on this system';
    case 'not_installed':
      return 'Not downloaded';
    default:
      return 'Unknown';
  }
});

/** Overall install progress percent (0-100), null when not computable. */
const overallPercent = computed(() => {
  const p = progress.value;
  if (!p || p.overallTotal <= 0) return null;
  return Math.min(100, Math.round((p.overallBytes / p.overallTotal) * 100));
});

function formatMb(bytes: number): string {
  if (bytes <= 0) return '-';
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

/**
 * Radio change handler: switching to Bango Local before a healthy install
 * opens the consent dialog (Cancel reverts the radio); switching away just
 * persists the selection (artifacts stay until removed).
 */
async function onBackendChange(value: EmbeddingBackendId): Promise<void> {
  actionError.value = null;
  if (value === 'bango_local' && !readyForUse.value) {
    showConsent.value = true; // radio visually reverts until confirmed
    return;
  }
  switching.value = true;
  try {
    await selectBackend(value);
  } catch (e) {
    actionError.value = String(e);
  } finally {
    switching.value = false;
  }
}

/** Consent confirmed: select the local backend, then start the download. */
async function onConsentConfirm(): Promise<void> {
  showConsent.value = false;
  try {
    await selectBackend('bango_local');
    await install();
  } catch (e) {
    actionError.value = String(e);
  }
}

function onConsentCancel(): void {
  showConsent.value = false;
}

/**
 * Install / repair with the CURRENT selection (manual button). L4
 * (findings-7): when the local backend is NOT selected, the download routes
 * through the consent dialog first (locked decision 16 - the Gemma terms
 * must precede any first download, wherever it is triggered from); the
 * confirm handler persists `bango_local` then installs. With local already
 * selected the consent was given at selection time - install directly.
 */
async function onInstall(): Promise<void> {
  actionError.value = null;
  if (backend.value !== 'bango_local') {
    showConsent.value = true;
    return;
  }
  try {
    await install();
  } catch (e) {
    actionError.value = String(e);
  }
}

async function onVerify(): Promise<void> {
  actionError.value = null;
  try {
    await verify();
  } catch (e) {
    actionError.value = String(e);
  }
}

async function onRemove(): Promise<void> {
  if (!confirmRemove.value) {
    confirmRemove.value = true;
    return;
  }
  confirmRemove.value = false;
  actionError.value = null;
  try {
    await remove();
    verifyResult.value = null;
  } catch (e) {
    actionError.value = String(e);
  }
}
</script>

<template>
  <section id="settings-embeddings" class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">psychology</span>
      Embeddings
    </h2>
    <p class="settings-card__desc">
      Embeddings let Bango find articles by meaning instead of exact keywords, powering semantic
      search and the Citation Finder.
      <button class="settings-card__learn-more" @click="openEmbeddingsHelp">
        <span class="material-symbols-outlined">menu_book</span>
        Learn more
      </button>
    </p>

    <div v-if="actionError || error" class="settings-card__status emb-error">
      {{ actionError ?? error }}
    </div>

    <fieldset class="emb-options" :disabled="loading || installing || switching">
      <legend class="emb-options__legend">Embedding Provider</legend>
      <label
        class="emb-option"
        :class="{ 'emb-option--active': backend === 'configured_provider' }"
      >
        <input
          type="radio"
          name="embedding-backend"
          value="configured_provider"
          :checked="backend === 'configured_provider'"
          :disabled="cloudOptionDisabled"
          @change="onBackendChange('configured_provider')"
        />
        <span class="emb-option__body">
          <span class="emb-option__label">Configured Provider</span>
          <span class="emb-option__hint">
            {{ cloudOptionHint }}
          </span>
        </span>
      </label>
      <label class="emb-option" :class="{ 'emb-option--active': backend === 'bango_local' }">
        <input
          type="radio"
          name="embedding-backend"
          value="bango_local"
          :checked="backend === 'bango_local'"
          :disabled="status?.supportedTarget === false"
          @change="onBackendChange('bango_local')"
        />
        <span class="emb-option__body">
          <span class="emb-option__label">Bango Local</span>
          <span class="emb-option__hint">
            On-device EmbeddingGemma 300M ({{ formatMb(status?.downloadBytes ?? 0) }} download).
            {{ status?.supportedTarget === false ? 'Not available on this system.' : '' }}
          </span>
        </span>
      </label>
    </fieldset>

    <!-- Local component status / install -->
    <div v-if="status" class="emb-local">
      <div class="emb-status">
        <span
          class="emb-status__dot"
          :class="{
            'emb-status__dot--ok': readyForUse,
            'emb-status__dot--warn': needsRepair || state === 'installing',
            'emb-status__dot--off': state === 'not_installed' || state === 'unsupported',
          }"
        ></span>
        <span class="emb-status__label">{{ stateLabel }}</span>
        <span v-if="status.installedBytes > 0" class="emb-status__size">
          {{ formatMb(status.installedBytes) }} installed
        </span>
      </div>

      <!-- Progress while installing -->
      <div v-if="progress && installing" class="emb-progress">
        <div class="emb-progress__row">
          <span class="emb-progress__phase">{{ progress.phase }}</span>
          <span class="emb-progress__file">{{ progress.file }}</span>
        </div>
        <div class="emb-progress__bar">
          <div class="emb-progress__fill" :style="{ width: `${overallPercent ?? 0}%` }"></div>
        </div>
        <div class="emb-progress__meta">
          <span v-if="overallPercent !== null">{{ overallPercent }}%</span>
          <button class="btn btn--secondary emb-progress__cancel" @click="cancelInstall">
            Cancel
          </button>
        </div>
      </div>

      <!-- Repair / runtime-missing banner -->
      <div v-if="needsRepair && !installing" class="emb-banner emb-banner--warn">
        <span class="material-symbols-outlined">build</span>
        <span>
          {{
            state === 'repair_required'
              ? 'The installed components are incomplete or damaged.'
              : 'The ONNX Runtime engine library is missing or damaged.'
          }}
          Re-download to repair.
        </span>
      </div>

      <div v-if="state !== 'unsupported' && !installing" class="settings-card__actions emb-actions">
        <button
          v-if="state === 'not_installed' || needsRepair"
          class="btn btn--primary"
          @click="onInstall"
        >
          <span class="material-symbols-outlined btn__icon">download</span>
          {{ installLabel }}
        </button>
        <button class="btn btn--secondary" :disabled="verifying" @click="onVerify">
          <span class="material-symbols-outlined btn__icon">verified</span>
          Verify Installation
        </button>
        <button
          v-if="state !== 'not_installed'"
          class="btn btn--danger"
          :disabled="removing"
          @click="onRemove"
        >
          <span class="material-symbols-outlined btn__icon">delete</span>
          {{ confirmRemove ? 'Click again to confirm removal' : 'Remove' }}
        </button>
      </div>

      <!-- Verify outcome -->
      <div
        v-if="verifyResult"
        class="emb-verify"
        :class="{ 'emb-verify--bad': !verifyResult.healthy }"
      >
        <p v-if="verifyResult.healthy">
          <span class="material-symbols-outlined">check_circle</span> All components verified
          against their pinned hashes.
        </p>
        <ul v-else>
          <li v-for="failure in verifyResult.failures" :key="failure.name">
            <strong>{{ failure.name }}</strong
            >: {{ failure.reason }}
          </li>
        </ul>
      </div>

      <!-- Component Details -->
      <div class="emb-details">
        <button class="emb-details__toggle" @click="detailsOpen = !detailsOpen">
          <span class="material-symbols-outlined">{{
            detailsOpen ? 'expand_less' : 'expand_more'
          }}</span>
          Component Details
        </button>
        <dl v-if="detailsOpen" class="emb-details__grid">
          <dt>Profile</dt>
          <dd>
            <code>{{ status.profile }}</code>
          </dd>
          <dt>Model</dt>
          <dd>{{ status.model }} (768 dimensions)</dd>
          <dt>Engine</dt>
          <dd>
            ONNX Runtime {{ status.runtimeVersion ?? '-' }} (CPU, up to
            {{ status.threadBudget }} threads)
          </dd>
          <dt>Installed size</dt>
          <dd>{{ formatMb(status.installedBytes) }}</dd>
          <dt>Model files</dt>
          <dd>
            <code>{{ status.modelRoot }}</code>
          </dd>
          <dt>Runtime</dt>
          <dd>
            <code>{{ status.runtimeRoot }}</code>
          </dd>
        </dl>
        <p v-if="detailsOpen && status.usedFallback" class="emb-details__fallback">
          <span class="material-symbols-outlined">info</span>
          Your documents folder is managed by OneDrive, so the model files are stored in the local
          app-data folder instead (Bango never uploads them).
        </p>
      </div>
    </div>

    <!-- Consent dialog (first Bango Local download) -->
    <EmbeddingsConsentDialog
      v-if="showConsent"
      :status="status"
      @confirm="onConsentConfirm"
      @cancel="onConsentCancel"
    />
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';

.emb-error {
  color: #991b1b;
  background-color: #fef2f2;
}

.emb-options {
  border: none;
  padding: 0;
  margin: 0.75rem 0 0.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.emb-options__legend {
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--color-outline, #777587);
  padding: 0;
  margin-bottom: 0.25rem;
}

.emb-option {
  display: flex;
  gap: 0.625rem;
  align-items: flex-start;
  padding: 0.625rem 0.875rem;
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: 0.5rem;
  background-color: var(--color-surface-container-low, #f5f2ff);
  cursor: pointer;
}

.emb-option--active {
  border-color: var(--color-primary, #3525cd);
}

.emb-option input {
  margin-top: 0.25rem;
  accent-color: var(--color-primary, #3525cd);
}

.emb-option__body {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}

.emb-option__label {
  font-weight: 600;
  color: var(--color-on-surface, #1b1b24);
}

.emb-option__hint {
  font-size: 12px;
  color: var(--color-on-surface-variant, #464555);
}

.emb-local {
  margin-top: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
}

.emb-status {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 13px;
  color: var(--color-on-surface, #1b1b24);
}

.emb-status__dot {
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 9999px;
  background-color: var(--color-outline, #777587);
  flex-shrink: 0;
}

.emb-status__dot--ok {
  background-color: #15803d;
}

.emb-status__dot--warn {
  background-color: #b45309;
}

.emb-status__size {
  color: var(--color-outline, #777587);
  font-size: 12px;
}

.emb-progress {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.emb-progress__row {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--color-on-surface-variant, #464555);
}

.emb-progress__phase {
  text-transform: capitalize;
  font-weight: 600;
}

.emb-progress__bar {
  height: 0.375rem;
  border-radius: 9999px;
  background-color: var(--color-surface-variant, #e4e1ee);
  overflow: hidden;
}

.emb-progress__fill {
  height: 100%;
  background-color: var(--color-primary, #3525cd);
  transition: width 0.2s ease;
}

.emb-progress__meta {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 12px;
  color: var(--color-outline, #777587);
}

.emb-progress__cancel {
  font-size: 12px;
  padding: 0.125rem 0.625rem;
}

.emb-banner {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  font-size: 13px;
  padding: 0.5rem 0.75rem;
  border-radius: 0.5rem;
}

.emb-banner--warn {
  background-color: #fffbeb;
  color: #92400e;
  border: 1px solid #fde68a;
}

.emb-banner .material-symbols-outlined {
  font-size: 18px;
  flex-shrink: 0;
}

.emb-actions {
  margin-top: 0;
}

.emb-verify {
  font-size: 13px;
  padding: 0.5rem 0.75rem;
  border-radius: 0.5rem;
  background-color: #f0fdf4;
  color: #166534;
}

.emb-verify--bad {
  background-color: #fef2f2;
  color: #991b1b;
}

.emb-verify ul {
  margin: 0;
  padding-left: 1rem;
}

.emb-verify p {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  margin: 0;
}

.emb-details__toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  background: none;
  border: none;
  padding: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--color-primary, #3525cd);
  cursor: pointer;
}

.emb-details__grid {
  display: grid;
  grid-template-columns: minmax(110px, auto) 1fr;
  gap: 0.375rem 0.75rem;
  font-size: 12.5px;
  margin: 0.5rem 0 0;
  padding: 0.625rem 0.875rem;
  background-color: var(--color-surface-container-low, #f5f2ff);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: 0.5rem;
}

.emb-details__grid dt {
  color: var(--color-on-surface-variant, #464555);
  font-weight: 600;
}

.emb-details__grid dd {
  margin: 0;
  color: var(--color-on-surface, #1b1b24);
  word-break: break-all;
}

.emb-details__grid code {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  font-size: 11.5px;
}

.emb-details__fallback {
  display: flex;
  gap: 0.375rem;
  align-items: center;
  font-size: 12px;
  color: var(--color-on-surface-variant, #464555);
  margin: 0.5rem 0 0;
}

.emb-details__fallback .material-symbols-outlined {
  font-size: 16px;
  flex-shrink: 0;
}
</style>
