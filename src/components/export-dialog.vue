<script setup lang="ts">
import { ref, computed } from 'vue';
import { useExport } from '@/composables/use-export';

const props = defineProps<{
  activeTab?: string;
  statusCounts?: Record<string, number>;
  tabLabel?: string;
}>();
const emit = defineEmits<{ close: [] }>();
const { exporting, error, exportRis, exportRisForTab, exportProject } = useExport();
const showBackup = ref(false);

/** Map tab key to a human-readable label for the export button and messages */
const TAB_LABELS: Record<string, string> = {
  all: 'All',
  duplicate: 'Duplicate',
  working: 'Working',
  included: 'Included',
  rejected: 'Rejected',
  error: 'Error',
};

/** Whether this dialog is being used from the article list (tab-aware) */
const isTabExport = computed(() => !!props.activeTab && props.activeTab !== 'prisma');

const currentTabLabel = computed(
  () => props.tabLabel ?? TAB_LABELS[props.activeTab ?? 'all'] ?? 'All'
);

/** Number of articles in the current tab */
const tabCount = computed(() => {
  if (!props.activeTab || !props.statusCounts) return 0;
  return props.statusCounts[props.activeTab] ?? 0;
});

/** Whether the current tab has articles to export */
const hasArticles = computed(() => tabCount.value > 0);

const isPrismaTab = computed(() => props.activeTab === 'prisma');
</script>

<template>
  <div class="dialog-overlay" @click.self="emit('close')">
    <div class="dialog">
      <h2>Export</h2>

      <div v-if="error" class="dialog__error">{{ error }}</div>

      <div v-if="!showBackup" class="dialog__options">
        <!-- Tab-aware export (article list) -->
        <template v-if="isTabExport">
          <div v-if="!hasArticles" class="dialog__empty">
            <p class="dialog__empty-msg">No {{ currentTabLabel }} articles found to export</p>
          </div>
          <template v-else>
            <button
              class="btn btn--primary"
              :disabled="exporting"
              @click="
                async () => {
                  if (await exportRisForTab(activeTab!, activeTab === 'error', currentTabLabel))
                    emit('close');
                }
              "
            >
              Export {{ currentTabLabel }} Articles (RIS)
            </button>
          </template>
        </template>

        <!-- PRISMA tab: always export included -->
        <template v-else-if="isPrismaTab">
          <button
            class="btn btn--primary"
            :disabled="exporting"
            @click="
              async () => {
                if (await exportRis()) emit('close');
              }
            "
          >
            Export Included Articles (RIS)
          </button>
        </template>

        <!-- Default fallback -->
        <template v-else>
          <button
            class="btn btn--primary"
            :disabled="exporting"
            @click="
              async () => {
                if (await exportRis()) emit('close');
              }
            "
          >
            Export Included Articles (RIS)
          </button>
        </template>

        <button class="btn btn--secondary" @click="showBackup = true">Export Project Backup</button>
      </div>

      <div v-if="showBackup" class="dialog__backup">
        <p>
          Export your project data to a <code>.bango.json</code> file. Note: API keys are NOT
          included in the backup.
        </p>
        <div class="dialog__actions">
          <button class="btn btn--secondary" @click="showBackup = false">Back</button>
          <button
            class="btn btn--primary"
            :disabled="exporting"
            @click="
              async () => {
                if (await exportProject()) emit('close');
              }
            "
          >
            {{ exporting ? 'Exporting...' : 'Export Backup' }}
          </button>
        </div>
      </div>

      <div class="dialog__actions">
        <button class="btn btn--outline" @click="emit('close')">Cancel</button>
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
.dialog__options {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 12px);
}
.dialog__empty {
  padding: var(--space-4, 16px);
  text-align: center;
}
.dialog__empty-msg {
  font-size: var(--font-size-caption, 13px);
  color: var(--color-on-surface-variant, #464555);
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
  justify-content: flex-end;
  gap: var(--space-2, 8px);
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
.btn--outline {
  background: transparent;
  color: var(--color-on-surface-variant, #464555);
  border: 1px solid var(--color-outline, #777587);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
