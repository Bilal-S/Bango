<script setup lang="ts">
import { computed } from 'vue';
import { useToast } from '@/composables/use-toast';

const { history, clearHistory } = useToast();

/** Newest-first view of the in-memory notification history. */
const sortedHistory = computed(() => [...history.value].reverse());

const typeDotClass = (type: string): string => {
  switch (type) {
    case 'success':
      return 'notif__dot--success';
    case 'info':
      return 'notif__dot--info';
    case 'warning':
      return 'notif__dot--warning';
    case 'error':
      return 'notif__dot--error';
    default:
      return 'notif__dot--default';
  }
};

const typeLabel = (type: string): string => {
  switch (type) {
    case 'success':
      return 'Success';
    case 'info':
      return 'Info';
    case 'warning':
      return 'Warning';
    case 'error':
      return 'Error';
    default:
      return 'Info';
  }
};

/** Format an epoch-ms timestamp as a concise local time string. */
function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">notifications</span>
      Notification History
    </h2>
    <p class="settings-card__desc">
      Recent toast notifications from this session. This list is in-memory only and clears when the
      app restarts.
    </p>
    <div class="settings-card__actions">
      <button
        class="btn btn--secondary"
        :disabled="sortedHistory.length === 0"
        @click="clearHistory"
      >
        <span class="material-symbols-outlined btn__icon">mop</span>
        Clear History
      </button>
    </div>

    <!-- Notification history entries -->
    <div v-if="sortedHistory.length > 0" class="notif-list">
      <div v-for="entry in sortedHistory" :key="entry.id" class="notif">
        <span class="notif__dot" :class="typeDotClass(entry.type)" />
        <div class="notif__body">
          <div class="notif__meta">
            <span class="notif__type">{{ typeLabel(entry.type) }}</span>
            <span class="notif__time">{{ formatTime(entry.timestamp) }}</span>
          </div>
          <p class="notif__msg">{{ entry.message }}</p>
        </div>
      </div>
    </div>
    <p v-else class="notif-list__empty">No notifications recorded this session.</p>
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';

.notif-list {
  margin-top: 1rem;
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: var(--radius-lg, 0.5rem);
  overflow: hidden;
  max-height: 360px;
  overflow-y: auto;
}

.notif-list__empty {
  margin-top: 1rem;
  padding: 1rem;
  font-size: 13px;
  color: var(--color-on-surface-variant, #464555);
  text-align: center;
  border: 1px dashed var(--color-surface-variant, #e4e1ee);
  border-radius: var(--radius-lg, 0.5rem);
}

.notif {
  display: flex;
  align-items: flex-start;
  gap: 0.625rem;
  padding: 0.625rem 0.875rem;
  border-bottom: 1px solid var(--color-surface-variant, #e4e1ee);
  background-color: var(--color-surface-container-lowest, #ffffff);
}

.notif:last-child {
  border-bottom: none;
}

.notif__dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  margin-top: 6px;
  border-radius: 9999px;
  background-color: var(--color-on-surface-variant, #777587);
}

.notif__dot--success {
  background-color: #16a34a;
}

.notif__dot--info {
  background-color: #2563eb;
}

.notif__dot--warning {
  background-color: #d97706;
}

.notif__dot--error {
  background-color: #dc2626;
}

.notif__dot--default {
  background-color: var(--color-on-surface-variant, #777587);
}

.notif__body {
  flex: 1;
  min-width: 0;
}

.notif__meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.125rem;
}

.notif__type {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--color-on-surface-variant, #464555);
}

.notif__time {
  font-size: 11px;
  color: var(--color-outline, #777587);
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
}

.notif__msg {
  margin: 0;
  font-size: 13px;
  line-height: 18px;
  color: var(--color-on-surface, #1b1b24);
  word-break: break-word;
}
</style>
