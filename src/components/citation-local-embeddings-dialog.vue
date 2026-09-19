<script setup lang="ts">
import { computed, ref, onMounted } from 'vue';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { LocalComponentStatus, ComponentProgress } from '@/composables/use-local-embeddings';

/**
 * Contextual Citation-Finder prompt (plan §8, T7): the user selected Bango
 * Local as the embedding backend but the components are not installed.
 * Three actions (emitted; the parent owns the follow-up):
 * - `download`  -> [Download and Continue] - install, then continue the search
 * - `useCloud`  -> [Use Configured Provider] - switch the backend + continue
 * - `cancel`    -> restore the prose and abort
 *
 * While `installing` is true the dialog shows the live `embedding:component`
 * progress (phase + percent) and disables all actions.
 */
const props = defineProps<{
  /** Current component status (download-size line + Gemma license). */
  status: LocalComponentStatus | null;
  /** Live install progress while downloading (null until the first event). */
  progress: ComponentProgress | null;
  /** Whether the install command is in flight. */
  installing: boolean;
  /** Error message from a failed install (shown inline; actions re-enable). */
  error: string | null;
  /** Whether the CHAT provider supports an embedding API (findings-7): the
   * "Use Configured Provider" option is a dead end for Anthropic/Z.AI users,
   * so it hides with an explanation when false. */
  chatProviderSupportsEmbeddings: boolean;
}>();

const emit = defineEmits<{ download: []; useCloud: []; cancel: [] }>();

const cancelButton = ref<HTMLButtonElement | null>(null);

onMounted(() => {
  cancelButton.value?.focus();
});

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape' && !props.installing) {
    event.stopPropagation();
    emit('cancel');
  }
}

const downloadLabel = computed(() => {
  const bytes = props.status?.downloadBytes ?? 0;
  if (bytes <= 0) return 'about 220-300 MB';
  return `about ${Math.round(bytes / 1_000_000)} MB`;
});

const percent = computed(() => {
  const p = props.progress;
  if (!p || p.overallTotal <= 0) return null;
  return Math.min(100, Math.round((p.overallBytes / p.overallTotal) * 100));
});

const licenseUrl = computed(() => props.status?.licenseUrl ?? '');

/** Open the Gemma license terms (L4, findings-7: license surfaces in EVERY
 * download-triggering surface, not just the Settings consent dialog). */
function openLicense(): void {
  if (!licenseUrl.value) return;
  openUrl(licenseUrl.value).catch(() => undefined);
}
</script>

<template>
  <div class="dialog-overlay" @click.self="!installing && emit('cancel')" @keydown="onKeydown">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Bango Local embeddings needed">
      <h2>
        <span class="material-symbols-outlined text-primary">memory</span>
        Download Bango Local Embeddings?
      </h2>
      <div class="dialog__desc">
        <p>
          Bango Local is selected as your embedding provider, but its on-device model ({{
            status?.model ?? 'EmbeddingGemma 300M (Q4)'
          }}) is not downloaded yet. Citation Finder needs it to search your library semantically.
        </p>
        <p v-if="!installing">
          Download {{ downloadLabel }} now and continue{{
            chatProviderSupportsEmbeddings
              ? ", or use your configured provider's embedding API for this search"
              : ''
          }}.
        </p>
        <p v-if="!installing" class="cf-local-license">
          The model is distributed under the
          <a href="#" @click.prevent="openLicense">{{ status?.license ?? 'Gemma Terms of Use' }}</a>
          (opens in your browser).
        </p>
        <div v-else class="cf-local-progress">
          <div class="cf-local-progress__row">
            <span>{{ progress?.phase ?? 'starting' }}</span>
            <span v-if="percent !== null">{{ percent }}%</span>
          </div>
          <div class="cf-local-progress__bar">
            <div class="cf-local-progress__fill" :style="{ width: `${percent ?? 0}%` }"></div>
          </div>
          <p class="cf-local-progress__hint">
            The model is verified (SHA-256 pinned) and self-tested before use. You can keep using
            Bango while it downloads.
          </p>
        </div>
        <p v-if="error" class="cf-local-error">{{ error }}</p>
      </div>
      <div class="dialog__actions">
        <button
          ref="cancelButton"
          class="btn btn--secondary"
          :disabled="installing"
          @click="emit('cancel')"
        >
          Cancel
        </button>
        <button
          v-if="chatProviderSupportsEmbeddings"
          class="btn btn--secondary"
          :disabled="installing"
          @click="emit('useCloud')"
        >
          Use Configured Provider
        </button>
        <span
          v-else
          class="cf-local-deadend"
          title="Your configured chat provider does not offer an embedding API"
        >
          Your provider has no embedding API
        </span>
        <button class="btn btn--primary" :disabled="installing" @click="emit('download')">
          <span class="material-symbols-outlined btn__icon">download</span>
          Download and Continue
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cf-local-license {
  font-size: 11.5px;
  color: var(--color-outline, #777587);
}

.cf-local-deadend {
  font-size: 11.5px;
  color: var(--color-outline, #777587);
  align-self: center;
}

.cf-local-progress {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
  margin-top: 0.5rem;
}

.cf-local-progress__row {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--color-on-surface-variant, #464555);
  text-transform: capitalize;
}

.cf-local-progress__bar {
  height: 0.375rem;
  border-radius: 9999px;
  background-color: var(--color-surface-variant, #e4e1ee);
  overflow: hidden;
}

.cf-local-progress__fill {
  height: 100%;
  background-color: var(--color-primary, #3525cd);
  transition: width 0.2s ease;
}

.cf-local-progress__hint {
  font-size: 11.5px;
  color: var(--color-outline, #777587);
}

.cf-local-error {
  color: #991b1b;
  font-size: 12.5px;
  margin: 0.5rem 0 0;
}
</style>
