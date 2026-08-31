<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { usePrisma } from '@/composables/use-prisma';
import type { PrismaReportFormat } from '@/composables/use-prisma';
import ExportDialog from '@/components/export-dialog.vue';

const {
  data,
  loading,
  error,
  showExclusionReasons,
  loadDiagram,
  exportSvg,
  exportPng,
  exportReport,
} = usePrisma();
const showExport = ref(false);

/* ── Export Report dropdown ──────────────────────────────────────────────
 * Anchored at the "Export Report" button; closes on item pick, anchor
 * re-click, outside click, and Escape (bulk-action-bar More-menu pattern). */
const reportMenuOpen = ref(false);
const reportMenuRef = ref<HTMLElement | null>(null);

function toggleReportMenu(): void {
  reportMenuOpen.value = !reportMenuOpen.value;
}

/** Item picked: close the menu, then run the export for the chosen format. */
function pickReportFormat(format: PrismaReportFormat): void {
  reportMenuOpen.value = false;
  void exportReport(format);
}

/** Outside click closes the dropdown (suggest-input.vue pattern). */
function handleReportOutsideClick(event: MouseEvent): void {
  if (reportMenuRef.value && !reportMenuRef.value.contains(event.target as Node)) {
    reportMenuOpen.value = false;
  }
}

function handleReportKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') reportMenuOpen.value = false;
}

onMounted(() => {
  loadDiagram();
  document.addEventListener('click', handleReportOutsideClick);
  document.addEventListener('keydown', handleReportKeydown);
});

onUnmounted(() => {
  document.removeEventListener('click', handleReportOutsideClick);
  document.removeEventListener('keydown', handleReportKeydown);
});
</script>

<template>
  <div class="prisma-view">
    <!-- Header -->
    <header class="prisma-header">
      <h1 class="page-title">PRISMA 2020 Flow Diagram</h1>
      <div class="prisma-header__right">
        <!-- Toggle -->
        <label class="prisma-toggle" :class="{ 'prisma-toggle--active': showExclusionReasons }">
          <span class="prisma-toggle__label">Show exclusion reasons breakdown</span>
          <button
            role="switch"
            :aria-checked="showExclusionReasons"
            class="prisma-toggle__track"
            @click="showExclusionReasons = !showExclusionReasons"
          >
            <span class="prisma-toggle__thumb" />
          </button>
        </label>

        <!-- Export actions -->
        <div class="prisma-header__actions">
          <button class="btn btn--secondary" :disabled="loading || !data" @click="exportSvg">
            <span class="material-symbols-outlined btn__icon">download</span>
            Export SVG
          </button>
          <button class="btn btn--secondary" :disabled="loading || !data" @click="exportPng">
            <span class="material-symbols-outlined btn__icon">download</span>
            Export PNG
          </button>
          <button class="btn btn--secondary" @click="showExport = true">
            <span class="material-symbols-outlined btn__icon">download</span>
            Export RIS
          </button>
          <!-- Export Report: dropdown with Markdown / PDF (print dialog) -->
          <div ref="reportMenuRef" class="export-report">
            <button
              class="btn btn--secondary"
              :disabled="loading || !data"
              aria-haspopup="menu"
              :aria-expanded="reportMenuOpen"
              @click="toggleReportMenu"
            >
              <span class="material-symbols-outlined btn__icon">summarize</span>
              Export Report
              <span class="material-symbols-outlined btn__icon export-report__chevron">
                expand_more
              </span>
            </button>
            <ul v-if="reportMenuOpen" class="export-report__menu" role="menu">
              <li
                role="menuitem"
                title="Save the screening reasons report as a Markdown file"
                @click="pickReportFormat('markdown')"
              >
                <span class="material-symbols-outlined">description</span>
                Markdown
              </li>
              <li
                role="menuitem"
                title="Print the screening reasons report (choose Save as PDF)"
                @click="pickReportFormat('pdf')"
              >
                <span class="material-symbols-outlined">picture_as_pdf</span>
                PDF
              </li>
            </ul>
          </div>
          <button class="btn btn--primary" :disabled="loading" @click="loadDiagram">
            <span class="material-symbols-outlined btn__icon">refresh</span>
            Refresh
          </button>
        </div>
      </div>
    </header>

    <!-- Error State -->
    <div v-if="error" class="prisma-error">
      <span class="material-symbols-outlined">error</span>
      {{ error }}
    </div>

    <!-- Loading State -->
    <div v-if="loading && !data" class="prisma-loading">
      <div class="prisma-loading__spinner" />
      <p>Loading PRISMA data&hellip;</p>
    </div>

    <!-- Diagram Canvas -->
    <div v-if="data" class="prisma-canvas">
      <div class="prisma-flow">
        <!-- Level 1: Identification -->
        <div class="prisma-box prisma-box--main">
          <h3 class="prisma-box__title">Identification</h3>
          <p class="prisma-box__desc">
            Records identified from databases (n={{ data.recordsIdentified.toLocaleString() }})
          </p>
        </div>

        <!-- Connector: Identification → Screening (with side branch) -->
        <div class="prisma-connector">
          <div class="prisma-connector__line" />
          <!-- Horizontal branch to side box -->
          <div class="prisma-connector__branch" />
          <div class="prisma-connector__arrow-right">
            <span class="material-symbols-outlined">arrow_right</span>
          </div>
          <!-- Down arrow -->
          <div class="prisma-connector__arrow-down">
            <span class="material-symbols-outlined">arrow_drop_down</span>
          </div>
          <!-- Side Box: Duplicates -->
          <div class="prisma-box prisma-box--side">
            <p class="prisma-box__desc">
              Duplicates removed (n={{ data.duplicatesRemoved.toLocaleString() }})
            </p>
          </div>
        </div>

        <!-- Level 2: Screening -->
        <div class="prisma-box prisma-box--main">
          <h3 class="prisma-box__title">Screening</h3>
          <p class="prisma-box__desc">
            Records screened (n={{ data.recordsScreened.toLocaleString() }})
          </p>
        </div>

        <!-- Connector: Screening → Eligibility (with side branch) -->
        <div class="prisma-connector">
          <div class="prisma-connector__line" />
          <div class="prisma-connector__branch" />
          <div class="prisma-connector__arrow-right">
            <span class="material-symbols-outlined">arrow_right</span>
          </div>
          <div class="prisma-connector__arrow-down">
            <span class="material-symbols-outlined">arrow_drop_down</span>
          </div>
          <!-- Side Box: Generally Excluded -->
          <div class="prisma-box prisma-box--side">
            <p class="prisma-box__desc">
              Records generally excluded (n={{ data.recordsExcludedGeneral.toLocaleString() }})
            </p>
          </div>
        </div>

        <!-- Level 3: Eligibility -->
        <div class="prisma-box prisma-box--main">
          <h3 class="prisma-box__title">Eligibility</h3>
          <p class="prisma-box__desc">
            Full-text articles assessed (n={{ data.recordsAssessed.toLocaleString() }})
          </p>
        </div>

        <!-- Connector: Eligibility → Included (with side branch for exclusion reasons) -->
        <div class="prisma-connector">
          <div class="prisma-connector__line" />
          <div class="prisma-connector__branch" />
          <div class="prisma-connector__arrow-right">
            <span class="material-symbols-outlined">arrow_right</span>
          </div>
          <div class="prisma-connector__arrow-down">
            <span class="material-symbols-outlined">arrow_drop_down</span>
          </div>
          <!-- Side Box: Excluded with reasons -->
          <div class="prisma-box prisma-box--side prisma-box--side-reasons">
            <p class="prisma-box__desc">
              Records excluded with reasons (n={{
                data.recordsExcludedWithReasons.toLocaleString()
              }})
            </p>
            <ul
              v-if="showExclusionReasons && data.exclusionReasons.length > 0"
              class="prisma-reasons"
            >
              <li v-for="reason in data.exclusionReasons" :key="reason.criterionId">
                {{ reason.criterionText }} (n={{ reason.count }})
              </li>
            </ul>
          </div>
        </div>

        <!-- Ongoing: In-progress articles (only shown if > 0) -->
        <template v-if="data.recordsInProgress > 0">
          <div class="prisma-box prisma-box--ongoing">
            <h3 class="prisma-box__title prisma-box__title--ongoing">Ongoing</h3>
            <p class="prisma-box__desc">
              Articles in progress (n={{ data.recordsInProgress.toLocaleString() }})
            </p>
          </div>
          <div class="prisma-connector">
            <div class="prisma-connector__line" />
            <div class="prisma-connector__arrow-down">
              <span class="material-symbols-outlined">arrow_drop_down</span>
            </div>
          </div>
        </template>

        <!-- Level 4: Included -->
        <div class="prisma-box prisma-box--included">
          <h3 class="prisma-box__title prisma-box__title--included">Included</h3>
          <p class="prisma-box__desc prisma-box__desc--included">
            Studies included in review (n={{ data.studiesIncluded.toLocaleString() }})
          </p>
        </div>
      </div>
    </div>

    <!-- Export Dialog -->
    <ExportDialog v-if="showExport" @close="showExport = false" />
  </div>
</template>

<style scoped>
/* ── View Container ── */
.prisma-view {
  padding: var(--container-padding);
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
  min-height: 100%;
}

/* ── Header ── */
.prisma-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-4);
}

.prisma-header__right {
  display: flex;
  align-items: center;
  gap: var(--space-6);
  flex-wrap: wrap;
}

.prisma-header__actions {
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
}

/* ── Toggle Switch ── */
.prisma-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  cursor: pointer;
}

.prisma-toggle__label {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  white-space: nowrap;
}

.prisma-toggle__track {
  position: relative;
  width: 40px;
  height: 24px;
  border-radius: var(--radius-pill);
  background-color: var(--color-surface-container-highest);
  border: 2px solid var(--color-outline);
  cursor: pointer;
  transition: all 0.2s ease;
  padding: 0;
  flex-shrink: 0;
}

.prisma-toggle--active .prisma-toggle__track {
  background-color: var(--color-primary);
  border-color: var(--color-primary);
}

.prisma-toggle__thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background-color: var(--color-outline);
  transition: all 0.2s ease;
}

.prisma-toggle--active .prisma-toggle__thumb {
  transform: translateX(16px);
  background-color: var(--color-on-primary);
}

/* ── Error ── */
.prisma-error {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
}

/* ── Loading ── */
.prisma-loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-4);
  padding: var(--space-16, 64px) 0;
  color: var(--color-on-surface-variant);
}

.prisma-loading__spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--color-surface-container-highest);
  border-top-color: var(--color-primary);
  border-radius: 50%;
  animation: prisma-spin 0.8s linear infinite;
}

@keyframes prisma-spin {
  to {
    transform: rotate(360deg);
  }
}

/* ── Diagram Canvas ── */
.prisma-canvas {
  flex: 1;
  overflow: auto;
  display: flex;
  justify-content: center;
  padding: var(--space-10) 0;
}

.prisma-flow {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 100%;
  max-width: 720px;
  min-width: 560px;
}

/* ── Flow Boxes ── */
.prisma-box--main {
  width: 256px;
  background-color: var(--color-surface-container-lowest);
  border: 1px solid var(--color-outline-variant);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
  text-align: center;
  box-shadow: var(--shadow-sm);
  position: relative;
  z-index: 1;
}

.prisma-box--ongoing {
  width: 256px;
  background-color: var(--color-surface-container-high);
  border: 1px dashed var(--color-outline-variant);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
  text-align: center;
  position: relative;
  z-index: 1;
}

.prisma-box__title--ongoing {
  color: var(--color-on-surface-variant);
}

.prisma-box--included {
  width: 256px;
  background-color: var(--color-primary-fixed);
  border: 1px solid var(--color-primary-fixed-dim, #c3c0ff);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
  text-align: center;
  box-shadow: var(--shadow-sm);
  position: relative;
  z-index: 1;
}

.prisma-box--side {
  position: absolute;
  top: 50%;
  right: 0;
  transform: translateY(-50%) translateX(calc(1rem + 4px));
  width: 192px;
  background-color: var(--color-surface-container-low);
  border: 1px solid var(--color-outline-variant);
  border-style: dashed;
  border-radius: var(--radius-lg);
  padding: var(--space-3);
  text-align: center;
}

.prisma-box--side-reasons {
  text-align: left;
}

.prisma-box__title {
  font-size: var(--font-size-h2);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-1);
}

.prisma-box__title--included {
  color: var(--color-on-primary-fixed, #0f0069);
}

.prisma-box__desc {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  margin: 0;
}

.prisma-box__desc--included {
  color: var(--color-on-primary-fixed-variant, #3323cc);
}

/* ── Exclusion Reasons List ── */
.prisma-reasons {
  list-style: disc;
  padding-left: var(--space-4);
  margin-top: var(--space-2);
  font-size: 11px;
  line-height: 16px;
  color: var(--color-on-surface-variant);
  opacity: 0.8;
}

.prisma-reasons li {
  margin-bottom: var(--space-1);
}

/* ── Connectors ── */
.prisma-connector {
  position: relative;
  width: 100%;
  height: 64px;
  display: flex;
  justify-content: center;
}

.prisma-connector__line {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  width: 1px;
  height: 100%;
  background-color: var(--color-outline-variant);
}

.prisma-connector__branch {
  position: absolute;
  top: 50%;
  left: 50%;
  width: 128px;
  height: 1px;
  background-color: var(--color-outline-variant);
}

.prisma-connector__arrow-right {
  position: absolute;
  top: 50%;
  left: calc(50% + 128px - 4px);
  transform: translateY(-50%);
  color: var(--color-outline-variant);
  font-size: 16px;
  line-height: 1;
}

.prisma-connector__arrow-down {
  position: absolute;
  bottom: -4px;
  left: 50%;
  transform: translateX(-50%);
  color: var(--color-outline-variant);
  font-size: 16px;
  line-height: 1;
}

/* ── Buttons (shared) ── */
.btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.15s ease;
}

.btn__icon {
  font-size: 18px;
}

.btn--primary {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.btn--primary:hover:not(:disabled) {
  opacity: 0.9;
}

.btn--secondary {
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
}

.btn--secondary:hover:not(:disabled) {
  background-color: var(--color-surface-container-highest);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* ── Export Report dropdown ── */
.export-report {
  position: relative;
}

.export-report__chevron {
  font-size: 16px;
  margin-left: calc(var(--space-1) * -1);
}

.export-report__menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  min-width: 168px;
  margin: 0;
  padding: var(--space-1);
  list-style: none;
  background-color: var(--color-surface-container-lowest);
  border: 1px solid var(--color-outline-variant);
  border-radius: var(--radius-default);
  box-shadow: var(--shadow-sm);
  z-index: 40;
}

.export-report__menu li {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  cursor: pointer;
}

.export-report__menu li:hover {
  background-color: var(--color-surface-container-high);
}

.export-report__menu li .material-symbols-outlined {
  font-size: 16px;
}

/* ── Responsive ── */
@media (max-width: 767px) {
  .prisma-view {
    padding: var(--container-padding-sm);
    gap: var(--space-4);
  }

  .prisma-header {
    flex-direction: column;
    align-items: flex-start;
  }

  .prisma-header__right {
    width: 100%;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .prisma-header__actions {
    flex-wrap: wrap;
  }

  .prisma-canvas {
    overflow-x: auto;
    padding: var(--space-4) 0;
  }

  .prisma-flow {
    min-width: 480px;
  }
}
</style>
