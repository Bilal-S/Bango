<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { openUrl } from '@tauri-apps/plugin-opener';

/**
 * Contextual activation prompt: a gated feature (for example a Citation
 * Finder or chat submit) hit "Bango AI selected but not ready". Offers the
 * in-place choice [Set Up Bango AI] / [Use Configured Provider] / [Cancel]
 * so the user never needs a Settings trip. Streaming install progress and
 * errors render inline while setup runs.
 */
const props = defineProps<{
  model: string;
  downloadBytes: number;
  license: string;
  licenseUrl: string;
  installing: boolean;
  progress: { file: string; overallBytes: number; overallTotal: number } | null;
  error: string | null;
}>();

const emit = defineEmits<{ setup: []; useConfigured: []; cancel: [] }>();

const cancelButton = ref<HTMLButtonElement | null>(null);

const downloadLabel = computed(() => {
  if (props.downloadBytes <= 0) return 'about 5.8 GB';
  return `about ${(props.downloadBytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
});

const percent = computed(() => {
  const p = props.progress;
  if (!p || !p.overallTotal) return 0;
  return Math.min(100, Math.round((p.overallBytes / p.overallTotal) * 100));
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
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Bango AI is not set up">
      <h2>
        <span class="material-symbols-outlined text-primary">smart_toy</span>
        Bango AI is not set up yet
      </h2>
      <div class="dialog__desc">
        <p>
          You are using Bango AI, which runs on this computer. The model
          <strong>{{ model }}</strong> has not been downloaded yet ({{ downloadLabel }}).
        </p>
        <p>
          Set it up now to continue locally, or use your configured provider for this request.
          Licensed under
          <a href="#" @click.prevent="openLicense">{{ license }}</a
          >.
        </p>
        <div v-if="installing" class="bango-contextual__progress">
          <div class="bango-contextual__bar"><div :style="{ width: `${percent}%` }" /></div>
          <span>{{ progress?.file || 'Setting up Bango AI' }} - {{ percent }}%</span>
        </div>
        <p v-if="error" class="bango-contextual__error">{{ error }}</p>
      </div>
      <div class="dialog__actions">
        <button ref="cancelButton" class="btn btn--secondary" @click="emit('cancel')">
          Cancel
        </button>
        <button class="btn btn--secondary" :disabled="installing" @click="emit('useConfigured')">
          Use Configured Provider
        </button>
        <button class="btn btn--primary" :disabled="installing" @click="emit('setup')">
          <span class="material-symbols-outlined btn__icon">download</span>
          Set Up Bango AI
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.bango-contextual__progress {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  font-size: 0.85rem;
}
.bango-contextual__bar {
  height: 0.4rem;
  background: #e5e7eb;
  border-radius: 9999px;
  overflow: hidden;
}
.bango-contextual__bar div {
  height: 100%;
  background: var(--color-primary, #4f46e5);
  transition: width 0.2s ease;
}
.bango-contextual__error {
  color: #b91c1c;
  white-space: pre-wrap;
}
</style>
