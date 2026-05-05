<script setup lang="ts">
import { ref } from 'vue';
import { useExport } from '@/composables/use-export';

const emit = defineEmits<{ close: [] }>();
const { exporting, error, exportRis, exportProject } = useExport();
const password = ref('');
const showBackup = ref(false);
</script>

<template>
  <div class="dialog-overlay" @click.self="emit('close')">
    <div class="dialog">
      <h2>Export</h2>

      <div v-if="error" class="dialog__error">{{ error }}</div>

      <div v-if="!showBackup" class="dialog__options">
        <button class="btn btn--primary" :disabled="exporting" @click="exportRis()">
          Export Included Articles (RIS)
        </button>
        <button class="btn btn--secondary" @click="showBackup = true">Export Project Backup</button>
      </div>

      <div v-if="showBackup" class="dialog__backup">
        <p>Enter a password to encrypt your API keys in the backup:</p>
        <input v-model="password" type="password" placeholder="Password" class="input" />
        <div class="dialog__actions">
          <button
            class="btn btn--primary"
            :disabled="exporting || !password"
            @click="exportProject(password)"
          >
            {{ exporting ? 'Exporting...' : 'Export Backup' }}
          </button>
          <button class="btn btn--secondary" @click="showBackup = false">Back</button>
        </div>
      </div>

      <button class="btn btn--ghost" @click="emit('close')">Cancel</button>
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
.dialog__options {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 12px);
}
.dialog__backup {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 12px);
}
.dialog__backup p {
  font-size: var(--font-size-caption, 13px);
  color: var(--color-on-surface-variant, #464555);
}
.dialog__actions {
  display: flex;
  gap: var(--space-2, 8px);
}
.input {
  padding: var(--space-2, 8px) var(--space-3, 12px);
  border: 1px solid var(--color-outline, #777587);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-caption, 13px);
}
.btn {
  padding: var(--space-2, 8px) var(--space-4, 16px);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-caption, 13px);
  font-weight: var(--font-weight-semibold, 600);
  cursor: pointer;
  text-align: center;
}
.btn--primary {
  background-color: var(--color-primary, #3525cd);
  color: var(--color-on-primary, #ffffff);
}
.btn--secondary {
  background-color: var(--color-surface-container-high, #eae6f4);
  color: var(--color-on-surface, #1b1b24);
}
.btn--ghost {
  color: var(--color-on-surface-variant, #464555);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
