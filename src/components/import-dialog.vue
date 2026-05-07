<script setup lang="ts">
import { ref } from 'vue';
import { useExport } from '@/composables/use-export';

const emit = defineEmits<{ close: []; imported: [] }>();
const { exporting, error, importProject } = useExport();
const selectedFile = ref<File | null>(null);

function onFileChange(event: Event): void {
  const target = event.target as HTMLInputElement;
  selectedFile.value = target.files?.[0] ?? null;
}

async function doImport(): Promise<void> {
  if (!selectedFile.value) return;
  await importProject(selectedFile.value);
  if (!error.value) {
    emit('imported');
    emit('close');
  }
}
</script>

<template>
  <div class="dialog-overlay" @click.self="emit('close')">
    <div class="dialog">
      <h2>Import Project</h2>

      <div v-if="error" class="dialog__error">{{ error }}</div>

      <div class="dialog__form">
        <label class="field__label">Backup File (.bango.json)</label>
        <input type="file" accept=".bango.json,.json" class="input" @change="onFileChange" />
        <p class="hint">Note: API keys are NOT included in the backup and must be re-entered.</p>
      </div>

      <div class="dialog__actions">
        <button class="btn btn--primary" :disabled="exporting || !selectedFile" @click="doImport">
          {{ exporting ? 'Importing...' : 'Import' }}
        </button>
        <button class="btn btn--ghost" @click="emit('close')">Cancel</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
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
  padding: var(--space-6, 24px);
  border-radius: var(--radius-md, 0.5rem);
  width: 420px;
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 16px);
}
.dialog h2 {
  font-size: var(--font-size-h1, 20px);
}
.dialog__error {
  padding: var(--space-3, 12px);
  background-color: var(--color-error-container, #ffdad6);
  color: var(--color-error, #ba1a1a);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-caption, 13px);
}
.dialog__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 12px);
}
.field__label {
  font-size: var(--font-size-label, 11px);
  font-weight: var(--font-weight-semibold, 600);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--color-on-surface-variant, #464555);
}
.input {
  padding: var(--space-2, 8px) var(--space-3, 12px);
  border: 1px solid var(--color-outline, #777587);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-caption, 13px);
}
.hint {
  font-size: 11px;
  color: var(--color-on-surface-variant, #464555);
}
.dialog__actions {
  display: flex;
  gap: var(--space-2, 8px);
}
.btn {
  padding: var(--space-2, 8px) var(--space-4, 16px);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-caption, 13px);
  font-weight: var(--font-weight-semibold, 600);
  cursor: pointer;
}
.btn--primary {
  background-color: var(--color-primary, #3525cd);
  color: var(--color-on-primary, #ffffff);
}
.btn--ghost {
  color: var(--color-on-surface-variant, #464555);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
