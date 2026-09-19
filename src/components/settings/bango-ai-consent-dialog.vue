<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { openUrl } from '@tauri-apps/plugin-opener';

/**
 * Consent gate shown before the first Bango AI download (runtime + model):
 * what runs locally, the download size, and the model license link. Emits
 * `confirm` / `cancel`; the parent owns the selection + install sequence.
 * Escape cancels and the cancel button takes initial focus (a multi-GB
 * download must never be focus-armed by default).
 */
const props = defineProps<{
  model: string;
  downloadBytes: number;
  license: string;
  licenseUrl: string;
}>();

const emit = defineEmits<{ confirm: []; cancel: [] }>();

const cancelButton = ref<HTMLButtonElement | null>(null);

const downloadLabel = computed(() => {
  if (props.downloadBytes <= 0) return 'about 5.8 GB';
  const gb = props.downloadBytes / (1024 * 1024 * 1024);
  return `about ${gb.toFixed(1)} GB`;
});

onMounted(() => {
  cancelButton.value?.focus();
});

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.stopPropagation();
    emit('cancel');
  }
}

function openLicense(): void {
  if (!props.licenseUrl) return;
  openUrl(props.licenseUrl).catch(() => undefined);
}
</script>

<template>
  <div class="dialog-overlay" @click.self="emit('cancel')" @keydown="onKeydown">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Set up Bango AI">
      <h2>
        <span class="material-symbols-outlined text-primary">smart_toy</span>
        Set up Bango AI?
      </h2>
      <div class="dialog__desc">
        <p>
          Bango AI runs the model <strong>{{ model }}</strong> entirely
          <strong>on this device</strong> - your article text never leaves your computer for Bango
          AI requests.
        </p>
        <p>
          It downloads the AI engine and the model ({{ downloadLabel }}). The download can be
          cancelled and resumed later, and everything can be removed from Settings.
        </p>
        <p>
          No API key or per-token charges. The model is distributed under the
          <a href="#" @click.prevent="openLicense">{{ license }}</a> (opens in your browser).
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
