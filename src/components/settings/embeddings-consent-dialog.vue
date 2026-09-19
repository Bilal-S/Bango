<script setup lang="ts">
import { computed, ref, onMounted } from 'vue';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { LocalComponentStatus } from '@/composables/use-local-embeddings';

/**
 * Consent dialog shown before the FIRST Bango Local download (plan §8):
 * what runs locally, the download size, and the Gemma terms link. Emits
 * `confirm` (Download and Enable) / `cancel` - the parent owns the actual
 * backend switch + install sequence. Escape cancels; the cancel button
 * takes initial focus (a large download must never be focus-armed by
 * default).
 */
const props = defineProps<{
  /** Current component status (drives the download-size line). */
  status: LocalComponentStatus | null;
}>();

const emit = defineEmits<{ confirm: []; cancel: [] }>();

/** Download size rounded to a friendly "about X MB" label. */
const downloadLabel = computed(() => {
  const bytes = props.status?.downloadBytes ?? 0;
  if (bytes <= 0) return 'about 220-300 MB';
  return `about ${Math.round(bytes / 1_000_000)} MB`;
});

const cancelButton = ref<HTMLButtonElement | null>(null);

onMounted(() => {
  cancelButton.value?.focus();
});

/** Escape cancels (keyboard parity with the overlay click). */
function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.stopPropagation();
    emit('cancel');
  }
}

const licenseUrl = computed(() => props.status?.licenseUrl ?? '');

function openLicense(): void {
  if (!licenseUrl.value) return;
  openUrl(licenseUrl.value).catch(() => undefined);
}
</script>

<template>
  <div class="dialog-overlay" @click.self="emit('cancel')" @keydown="onKeydown">
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label="Download Bango Local embeddings"
    >
      <h2>
        <span class="material-symbols-outlined text-primary">memory</span>
        Download Bango Local Embeddings?
      </h2>
      <div class="dialog__desc">
        <p>
          Bango Local runs the embedding model
          <strong>{{ status?.model ?? 'EmbeddingGemma 300M (Q4)' }}</strong> entirely
          <strong>on this device</strong> - article text used for semantic search never leaves your
          computer for embeddings. Cloud LLM calls (summaries, screening, chat) are unaffected and
          stay disclosed separately in their own settings.
        </p>
        <p>
          This downloads the model and the ONNX Runtime engine ({{ downloadLabel }}) into your Bango
          documents folder. The download can be cancelled and resumed, and everything can be removed
          later from this card.
        </p>
        <p>
          The model is distributed under the
          <a href="#" @click.prevent="openLicense">{{ status?.license ?? 'Gemma Terms of Use' }}</a>
          (opens in your browser).
        </p>
      </div>
      <div class="dialog__actions">
        <button ref="cancelButton" class="btn btn--secondary" @click="emit('cancel')">
          Cancel
        </button>
        <button class="btn btn--primary" @click="emit('confirm')">
          <span class="material-symbols-outlined btn__icon">download</span>
          Download and Enable
        </button>
      </div>
    </div>
  </div>
</template>
