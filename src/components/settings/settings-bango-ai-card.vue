<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useBangoAi } from '@/composables/use-bango-ai';

/**
 * Bango AI panel (Settings -> Provider section, shown when the Bango AI
 * backend is selected). Owns status, install progress, Test Bango AI,
 * engine settings, Verify/Remove, and Component Details. The consent dialog
 * and the backend switch live in the parent (`settings-ai-section.vue`).
 */
const emit = defineEmits<{ setup: []; switchProvider: [] }>();

const {
  status,
  progress,
  testOutcome,
  verifyOutcome,
  error,
  loading,
  installing,
  testing,
  verifying,
  removing,
  load,
  cancelInstall,
  test,
  verify,
  remove,
  loadSettings,
  saveSettings,
} = useBangoAi();

const removeArmed = ref(false);
const showDetails = ref(false);

const STAGE_LABELS: Record<string, string> = {
  installing: 'Installing AI engine',
  downloading: 'Downloading AI model',
  verifying: 'Verifying download',
  done: 'Ready',
  error: 'Setup failed',
};

const state = computed(() => status.value?.state ?? 'not_installed');
const ready = computed(() => state.value === 'ready');
const unsupported = computed(() => state.value === 'unsupported');
const needsSetup = computed(() => !ready.value && !unsupported.value);

const stageLabel = computed(() => {
  const phase = installing.value ? (progress.value?.phase ?? 'installing') : state.value;
  return STAGE_LABELS[phase] ?? 'Setting up Bango AI';
});

const overallPercent = computed(() => {
  const p = progress.value;
  if (!p || !p.overallTotal) return 0;
  return Math.min(100, Math.round((p.overallBytes / p.overallTotal) * 100));
});

/** Per-component (current file) progress: engine archive or model GGUF. */
const filePercent = computed(() => {
  const p = progress.value;
  if (!p || !p.fileTotal) return 0;
  return Math.min(100, Math.round((p.fileBytes / p.fileTotal) * 100));
});

/** Human label for the component currently downloading. */
const fileStageLabel = computed(() => {
  const phase = progress.value?.phase;
  return phase === 'downloading' || phase === 'verifying' ? 'AI model' : 'AI engine';
});

/** Human-readable byte count (KB / MB / GB). */
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 MB';
  const mb = bytes / (1024 * 1024);
  if (mb < 1) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  if (mb < 1024) return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

const downloadLabel = computed(() => {
  const bytes = status.value?.downloadBytes ?? 0;
  if (!bytes) return 'about 1.3 GB';
  return `about ${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
});

const memoryLine = computed(() => {
  const hw = status.value?.hardware;
  if (!hw) return '';
  const gb = Math.round(hw.totalRamMb / 1024);
  return `Memory: ${gb} GB detected${hw.totalRamMb >= 16 * 1024 ? ' - Recommended' : ''}`;
});

const warningReasons = computed(() =>
  status.value?.verdict.status === 'warning' ? (status.value.verdict.reasons ?? []) : []
);

const failureReasons = computed(() =>
  status.value?.verdict.status === 'unsupported' ? (status.value.verdict.reasons ?? []) : []
);

const timings = computed(() => {
  const t = testOutcome.value;
  if (!t) return null;
  return {
    load: `${(t.modelLoadMs / 1000).toFixed(1)}s`,
    response: `${(t.responseMs / 1000).toFixed(1)}s`,
    speed: `${t.tokensPerSecond.toFixed(1)} tok/s`,
    context: t.effectiveContext.toLocaleString(),
  };
});

async function onSetup(): Promise<void> {
  emit('setup');
}

async function onRemove(): Promise<void> {
  if (!removeArmed.value) {
    removeArmed.value = true;
    return;
  }
  removeArmed.value = false;
  await remove();
}

const settings = ref({ context: 16384, threads: 4, reasoning: false });

async function onSettingsChange(): Promise<void> {
  await saveSettings({ ...settings.value });
}

onMounted(async () => {
  await load();
  const saved = await loadSettings();
  if (saved) settings.value = saved;
});
</script>

<template>
  <div class="bango-card" aria-label="Bango AI">
    <!-- Unsupported machine: actionable blocked state, never cloud fallback. -->
    <div v-if="unsupported" class="bango-card__blocked">
      <span class="material-symbols-outlined">block</span>
      <div>
        <strong>Bango AI unavailable on this machine</strong>
        <p v-for="reason in failureReasons" :key="reason">{{ reason }}</p>
        <button class="btn btn--secondary" @click="emit('switchProvider')">
          Use Configured Provider
        </button>
      </div>
    </div>

    <template v-else>
      <!-- Status line -->
      <div class="bango-card__status">
        <span class="bango-card__dot" :class="{ 'is-ready': ready, 'is-busy': installing }" />
        <span v-if="loading">Checking Bango AI...</span>
        <span v-else-if="installing">{{ stageLabel }} - {{ overallPercent }}%</span>
        <span v-else-if="ready">Bango AI - Ready - {{ status?.model }}</span>
        <span v-else>{{ stageLabel }}</span>
        <span v-if="status?.busy" class="bango-card__busy"
          >Bango AI is busy - your request is queued</span
        >
      </div>

      <!-- Install progress: ONE overall meter + text byte counts (the
           per-component slim bar was removed as redundant). -->
      <div v-if="installing" class="bango-card__progress">
        <div class="bango-card__bar"><div :style="{ width: `${overallPercent}%` }" /></div>
        <div v-if="filePercent > 0" class="bango-card__progress-row">
          <span>{{ fileStageLabel }}</span>
          <span
            >{{ formatBytes(progress?.fileBytes ?? 0) }} of
            {{ formatBytes(progress?.fileTotal ?? 0) }}</span
          >
        </div>
        <div class="bango-card__progress-row">
          <span>{{ progress?.file || stageLabel }}</span>
          <button class="btn btn--secondary" @click="cancelInstall">Cancel</button>
        </div>
        <p class="bango-card__hint">You can cancel and resume later.</p>
      </div>

      <!-- Set-up pitch (also covers a restored selection without components) -->
      <div v-else-if="needsSetup" class="bango-card__pitch">
        <p>Your content stays on this device for Bango AI requests.</p>
        <p>No API key or per-token charges. Works offline after setup.</p>
        <p class="bango-card__hint">
          Local inference runs on this computer and is
          <strong>generally slower than cloud providers</strong>.
        </p>
        <p>
          <strong>Download: {{ downloadLabel }}</strong>
        </p>
        <p class="bango-card__hint">
          Disk needed: {{ Math.round((status?.requiredBytes ?? 0) / (1024 * 1024 * 1024)) }} GB.
          {{ memoryLine }}
        </p>
        <p v-for="reason in warningReasons" :key="reason" class="bango-card__warning">
          {{ reason }}
        </p>
        <div class="bango-card__actions">
          <button class="btn btn--primary" @click="onSetup">Set Up Bango AI</button>
          <button class="btn btn--secondary" @click="emit('switchProvider')">
            Use Configured Provider
          </button>
        </div>
      </div>

      <!-- Ready actions -->
      <template v-else>
        <div class="bango-card__actions">
          <button class="btn btn--secondary" :disabled="testing" @click="test">
            {{ testing ? 'Testing...' : 'Test Bango AI' }}
          </button>
          <button class="btn btn--secondary" :disabled="verifying" @click="verify">
            <span class="material-symbols-outlined btn__icon">verified</span>
            {{ verifying ? 'Verifying...' : 'Verify Installation' }}
          </button>
          <button class="btn btn--danger" :disabled="removing" @click="onRemove">
            <span class="material-symbols-outlined btn__icon">delete</span>
            {{ removeArmed ? 'Click again to confirm removal' : 'Remove' }}
          </button>
        </div>

        <p class="bango-card__hint">
          Local generation is generally slower than cloud providers; large batches take noticeably
          longer.
        </p>

        <div v-if="timings" class="bango-card__timings">
          <span>Model load {{ timings.load }}</span>
          <span>First response {{ timings.response }}</span>
          <span>{{ timings.speed }}</span>
          <span>Context {{ timings.context }}</span>
        </div>

        <details class="bango-card__advanced">
          <summary>Advanced</summary>
          <label class="bango-card__field">
            Context
            <select v-model.number="settings.context" @change="onSettingsChange">
              <option :value="8192">8,192</option>
              <option :value="16384">16,384</option>
              <option :value="32768">32,768</option>
              <option :value="65536">65,536</option>
            </select>
          </label>
          <label class="bango-card__field">
            Threads
            <input
              v-model.number="settings.threads"
              type="number"
              min="1"
              max="16"
              @change="onSettingsChange"
            />
          </label>
          <label class="bango-card__field bango-card__field--check">
            <input v-model="settings.reasoning" type="checkbox" @change="onSettingsChange" />
            Deeper reasoning for prose tasks
          </label>
        </details>

        <button class="bango-card__details-toggle" @click="showDetails = !showDetails">
          {{ showDetails ? 'Hide' : 'Show' }} Component Details
        </button>
        <dl v-if="showDetails" class="bango-card__details">
          <dt>Model</dt>
          <dd>{{ status?.model }} ({{ status?.profile }})</dd>
          <dt>Engine</dt>
          <dd>{{ status?.engine }} {{ status?.runtimeVersion }}</dd>
          <dt>Installed size</dt>
          <dd>{{ ((status?.installedBytes ?? 0) / (1024 * 1024 * 1024)).toFixed(1) }} GB</dd>
          <dt>Model path</dt>
          <dd class="is-path">{{ status?.modelRoot }}</dd>
          <dt>Runtime path</dt>
          <dd class="is-path">{{ status?.runtimeRoot }}</dd>
          <dt>Log</dt>
          <dd class="is-path">{{ status?.logPath }}</dd>
          <p v-if="status?.usedFallback" class="bango-card__hint">
            OneDrive detected: models live in app data instead.
          </p>
        </dl>
      </template>
    </template>

    <p v-if="verifyOutcome && !verifyOutcome.healthy" class="bango-card__warning">
      Verification found {{ verifyOutcome.failures.length }} problem(s). Use Set Up Bango AI to
      repair.
    </p>
    <p v-if="error" class="bango-card__error">{{ error }}</p>
  </div>
</template>

<style scoped>
.bango-card {
  border: 1px solid var(--color-outline-variant, #d7d7e0);
  border-radius: 0.75rem;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}
.bango-card__status {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.95rem;
}
.bango-card__dot {
  width: 0.6rem;
  height: 0.6rem;
  border-radius: 9999px;
  background: #9ca3af;
}
.bango-card__dot.is-ready {
  background: #10b981;
}
.bango-card__dot.is-busy {
  background: #f59e0b;
}
.bango-card__busy {
  font-size: 0.8rem;
  color: #92400e;
  margin-left: auto;
}
.bango-card__progress,
.bango-card__pitch {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}
.bango-card__bar {
  height: 0.4rem;
  background: #e5e7eb;
  border-radius: 9999px;
  overflow: hidden;
}
.bango-card__bar div {
  height: 100%;
  background: var(--color-primary, #4f46e5);
  transition: width 0.2s ease;
}
.bango-card__progress-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 0.85rem;
}
.bango-card__actions {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
}
.bango-card__timings {
  display: flex;
  gap: 1rem;
  flex-wrap: wrap;
  font-size: 0.85rem;
  color: var(--color-on-surface-variant, #464555);
}
.bango-card__field {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  font-size: 0.9rem;
  padding: 0.25rem 0;
}
.bango-card__field--check {
  justify-content: flex-start;
}
.bango-card__details-toggle {
  align-self: flex-start;
  background: none;
  border: none;
  color: var(--color-primary, #4f46e5);
  cursor: pointer;
  padding: 0;
}
.bango-card__details {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 0.25rem 1rem;
  font-size: 0.85rem;
  margin: 0;
}
.bango-card__details dt {
  font-weight: 600;
}
.bango-card__details dd {
  margin: 0;
  word-break: break-all;
}
.bango-card__hint {
  font-size: 0.8rem;
  color: var(--color-on-surface-variant, #464555);
  margin: 0;
}
.bango-card__warning {
  color: #92400e;
  font-size: 0.85rem;
  margin: 0;
}
.bango-card__error {
  color: #b91c1c;
  font-size: 0.85rem;
  margin: 0;
  white-space: pre-wrap;
}
.bango-card__blocked {
  display: flex;
  gap: 0.75rem;
  align-items: flex-start;
}
</style>
