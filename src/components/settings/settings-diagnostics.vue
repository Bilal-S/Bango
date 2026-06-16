<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const showErrorLog = ref(false);
const errorLogEntries = ref<Array<{ id: string; timestamp: string; details: string | null }>>([]);
const errorLogLoading = ref(false);
const clearLogsStatus = ref<string | null>(null);

async function doShowErrorLog(): Promise<void> {
  if (showErrorLog.value) {
    showErrorLog.value = false;
    return;
  }
  errorLogLoading.value = true;
  try {
    const entries = await invoke<Array<{ id: string; timestamp: string; details: string | null }>>(
      'get_generic_audit_entries',
      { limit: 10 }
    );
    errorLogEntries.value = entries;
    showErrorLog.value = true;
  } catch {
    errorLogEntries.value = [];
  } finally {
    errorLogLoading.value = false;
  }
}

async function doClearSystemLogs(): Promise<void> {
  try {
    const count = await invoke<number>('clear_generic_audit');
    clearLogsStatus.value = `Cleared ${count} system log entry(ies).`;
    setTimeout(() => {
      clearLogsStatus.value = null;
    }, 3000);
  } catch (e) {
    clearLogsStatus.value = `Failed to clear logs: ${e}`;
  }
}
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">troubleshoot</span>
      Diagnostics
    </h2>
    <p class="settings-card__desc">View recent system errors and diagnostic information.</p>
    <p v-if="clearLogsStatus" class="settings-card__status">{{ clearLogsStatus }}</p>
    <div class="settings-card__actions">
      <button class="btn btn--secondary" :disabled="errorLogLoading" @click="doShowErrorLog">
        <span v-if="errorLogLoading" class="material-symbols-outlined btn__icon spinner"
          >progress_activity</span
        >
        <span v-else class="material-symbols-outlined btn__icon">{{
          showErrorLog ? 'visibility_off' : 'bug_report'
        }}</span>
        {{ showErrorLog ? 'Hide Errors' : 'Show Last 10 Errors' }}
      </button>
      <button class="btn btn--secondary" @click="doClearSystemLogs">
        <span class="material-symbols-outlined btn__icon">mop</span>
        Clear System Logs
      </button>
    </div>

    <!-- Error log entries -->
    <div v-if="showErrorLog" class="error-log">
      <p v-if="errorLogEntries.length === 0" class="error-log__empty">No system errors recorded.</p>
      <div v-for="entry in errorLogEntries" :key="entry.id" class="error-log__entry">
        <span class="error-log__time">{{ entry.timestamp }}</span>
        <span class="error-log__details">{{ entry.details }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';

.error-log {
  margin-top: 1rem;
  border: 1px solid #fecaca;
  border-radius: var(--radius-lg, 0.5rem);
  overflow: hidden;
}

.error-log__empty {
  padding: 1rem;
  font-size: 13px;
  color: var(--color-on-surface-variant, #464555);
  text-align: center;
}

.error-log__entry {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid #fecaca;
  background-color: #fef2f2;
}

.error-log__entry:last-child {
  border-bottom: none;
}

.error-log__time {
  font-size: 11px;
  color: var(--color-outline, #777587);
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
}

.error-log__details {
  font-size: 13px;
  color: #991b1b;
  line-height: 18px;
  word-break: break-word;
}
</style>
