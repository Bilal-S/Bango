<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { useTrendsQueueStore } from '../stores/trends-queue';
import GoogleTrendsWidget from './google-trends-widget.vue';
import { TIME_RANGE_PRESETS, type TimeRangeId } from '../utils/google-trends';

const trendsQueue = useTrendsQueueStore();

// Offline detection
const isOnline = ref(typeof navigator !== 'undefined' ? navigator.onLine : true);
const updateOnlineStatus = () => {
  isOnline.value = navigator.onLine;
};

// Resize logic
const panelHeight = ref(360);
const isResizing = ref(false);

function startResize(e: MouseEvent) {
  isResizing.value = true;
  document.addEventListener('mousemove', handleResize);
  document.addEventListener('mouseup', stopResize);
  e.preventDefault();
}

function handleResize(e: MouseEvent) {
  if (!isResizing.value) return;
  const newHeight = window.innerHeight - e.clientY;
  panelHeight.value = Math.max(280, Math.min(600, newHeight));
}

function stopResize() {
  isResizing.value = false;
  document.removeEventListener('mousemove', handleResize);
  document.removeEventListener('mouseup', stopResize);
}

onMounted(() => {
  window.addEventListener('online', updateOnlineStatus);
  window.addEventListener('offline', updateOnlineStatus);
});

onUnmounted(() => {
  window.removeEventListener('online', updateOnlineStatus);
  window.removeEventListener('offline', updateOnlineStatus);
  document.removeEventListener('mousemove', handleResize);
  document.removeEventListener('mouseup', stopResize);
});

// View mode toggle
const viewMode = ref<'dual' | 'chart' | 'map'>('dual');

// Custom date inputs
const localStart = ref(trendsQueue.customStart || '');
const localEnd = ref(trendsQueue.customEnd || '');
const customRangeError = ref('');
const customClampedWarning = ref(false);

function handleRangeSelect(e: Event) {
  const target = e.target as HTMLSelectElement;
  const val = target.value as TimeRangeId;
  if (val !== 'custom') {
    trendsQueue.setTimeRange(val);
    customRangeError.value = '';
    customClampedWarning.value = false;
  }
}

function applyCustomDates() {
  customRangeError.value = '';
  customClampedWarning.value = false;

  if (!localStart.value || !localEnd.value) {
    customRangeError.value = 'Both dates are required.';
    return;
  }

  const startD = new Date(localStart.value);
  const endD = new Date(localEnd.value);

  if (isNaN(startD.getTime()) || isNaN(endD.getTime())) {
    customRangeError.value = 'Invalid date values.';
    return;
  }

  if (startD > endD) {
    customRangeError.value = 'Start date must be before end date.';
    return;
  }

  const today = new Date();
  if (endD > today) {
    customRangeError.value = 'End date cannot be in the future.';
    return;
  }

  const wasClamped = trendsQueue.setCustomRange(localStart.value, localEnd.value);
  if (wasClamped) {
    customClampedWarning.value = true;
    localStart.value = trendsQueue.customStart || '';
  }
}
</script>

<template>
  <div
    v-if="trendsQueue.hasKeywords"
    class="trends-panel border-t border-slate-200 bg-slate-50/80 backdrop-blur-sm flex flex-col relative shrink-0"
    :style="{ height: trendsQueue.collapsed ? 'auto' : panelHeight + 'px' }"
  >
    <!-- Resize Handle -->
    <div
      v-if="!trendsQueue.collapsed"
      class="resize-handle absolute top-0 left-0 right-0 h-1 cursor-ns-resize z-50 hover:bg-indigo-400/50 transition-colors"
      @mousedown="startResize"
    ></div>

    <!-- Panel Header Toolbar -->
    <header
      class="flex flex-wrap items-center justify-between gap-3 px-4 py-2 border-b border-slate-200 bg-white select-none shrink-0"
    >
      <div class="flex items-center gap-3">
        <!-- Range Selector -->
        <div class="flex items-center gap-1.5">
          <span class="material-symbols-outlined text-[15px] text-slate-400">schedule</span>
          <select
            :value="trendsQueue.timeRangeId"
            class="text-xs bg-white border border-slate-200 rounded px-2 py-1 text-slate-700 font-medium focus:outline-none focus:border-indigo-400 cursor-pointer"
            @change="handleRangeSelect"
          >
            <option v-for="preset in TIME_RANGE_PRESETS" :key="preset.id" :value="preset.id">
              {{ preset.label }}
            </option>
            <option value="research" :disabled="!trendsQueue.researchRange">Research Range</option>
            <option value="custom">Custom Dates...</option>
          </select>
        </div>

        <!-- Custom Date Pickers Inline -->
        <div
          v-if="trendsQueue.timeRangeId === 'custom'"
          class="flex items-center gap-1 bg-slate-50 px-2 py-0.5 rounded border border-slate-150"
        >
          <input
            v-model="localStart"
            type="date"
            class="text-[11px] bg-transparent border-0 focus:outline-none text-slate-600 cursor-pointer"
          />
          <span class="text-slate-400 text-[10px]">to</span>
          <input
            v-model="localEnd"
            type="date"
            class="text-[11px] bg-transparent border-0 focus:outline-none text-slate-600 cursor-pointer"
          />
          <button
            class="ml-1 bg-white hover:bg-slate-100 border border-slate-200 rounded px-1.5 py-0.5 text-[10px] font-semibold text-slate-600 cursor-pointer transition-colors"
            @click="applyCustomDates"
          >
            Apply
          </button>
        </div>
        <span v-if="customRangeError" class="text-[10px] text-rose-500 font-medium">
          {{ customRangeError }}
        </span>
        <span
          v-if="customClampedWarning"
          class="text-[10px] text-amber-600 font-medium flex items-center gap-0.5"
        >
          <span class="material-symbols-outlined text-xs">warning</span>
          Clamped to 5y maximum
        </span>

        <span class="text-slate-300">|</span>

        <!-- View Mode Selector -->
        <div
          v-if="!trendsQueue.collapsed"
          class="flex border border-slate-200 rounded overflow-hidden text-[10px] font-semibold text-slate-600"
        >
          <button
            class="px-2 py-1 cursor-pointer transition-colors"
            :class="
              viewMode === 'dual' ? 'bg-indigo-50 text-indigo-600' : 'bg-white hover:bg-slate-50'
            "
            @click="viewMode = 'dual'"
          >
            Dual View
          </button>
          <button
            class="px-2 py-1 cursor-pointer border-l border-r border-slate-200 transition-colors"
            :class="
              viewMode === 'chart' ? 'bg-indigo-50 text-indigo-600' : 'bg-white hover:bg-slate-50'
            "
            @click="viewMode = 'chart'"
          >
            Chart
          </button>
          <button
            class="px-2 py-1 cursor-pointer transition-colors"
            :class="
              viewMode === 'map' ? 'bg-indigo-50 text-indigo-600' : 'bg-white hover:bg-slate-50'
            "
            @click="viewMode = 'map'"
          >
            Map
          </button>
        </div>
      </div>

      <!-- Keyword pills -->
      <div class="flex-1 flex items-center gap-1.5 overflow-x-auto min-w-0 max-w-xl px-2">
        <span class="text-[10px] text-slate-400 uppercase tracking-wider font-semibold shrink-0">
          Compare ({{ trendsQueue.keywords.length }}/5):
        </span>
        <div class="flex items-center gap-1 overflow-x-auto py-0.5 scrollbar-thin">
          <span
            v-for="kw in trendsQueue.keywords"
            :key="kw"
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] bg-slate-100 border border-slate-200 text-slate-700 font-medium shrink-0"
          >
            {{ kw }}
            <button
              class="hover:text-rose-500 rounded-full hover:bg-slate-200 p-0.5 text-slate-400 cursor-pointer flex items-center justify-center transition-colors"
              @click="trendsQueue.removeKeyword(kw)"
            >
              <span class="material-symbols-outlined text-[10px] font-bold">close</span>
            </button>
          </span>
        </div>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <button
          class="text-[11px] font-semibold text-slate-500 hover:text-rose-600 transition-colors cursor-pointer"
          @click="trendsQueue.clearAll"
        >
          Clear all
        </button>

        <button
          class="p-1 rounded hover:bg-slate-100 text-slate-500 transition-colors cursor-pointer flex items-center justify-center"
          :title="trendsQueue.collapsed ? 'Expand panel' : 'Collapse panel'"
          @click="trendsQueue.toggleCollapsed"
        >
          <span class="material-symbols-outlined text-base">
            {{ trendsQueue.collapsed ? 'expand_less' : 'expand_more' }}
          </span>
        </button>
      </div>
    </header>

    <!-- Panel Body (Embeds or placeholders) -->
    <div v-if="!trendsQueue.collapsed" class="flex-1 min-h-0 p-3 flex flex-col bg-slate-50/60">
      <!-- Offline Fallback -->
      <div
        v-if="!isOnline"
        class="flex-1 flex flex-col items-center justify-center py-6 text-slate-500 border border-dashed border-slate-200 rounded-lg bg-white"
      >
        <span class="material-symbols-outlined text-4xl text-slate-400 mb-2">signal_wifi_off</span>
        <h4 class="text-sm font-semibold text-slate-700">Offline Mode</h4>
        <p class="text-xs text-slate-400 mt-1 max-w-xs text-center">
          Google Trends visualizations require an active internet connection. Connect to the
          internet to load search volume trends.
        </p>
      </div>

      <!-- Online IFrame Views -->
      <div
        v-else
        class="flex-1 min-h-0"
        :class="{
          'grid grid-cols-1 md:grid-cols-2 gap-3 h-full': viewMode === 'dual',
          'h-full': viewMode !== 'dual',
        }"
      >
        <GoogleTrendsWidget
          v-if="viewMode === 'dual' || viewMode === 'chart'"
          type="TIMESERIES"
          :keywords="trendsQueue.keywords"
          :range="trendsQueue.resolvedRange"
          :revision="trendsQueue.revision"
          class="h-full min-h-0"
        />
        <GoogleTrendsWidget
          v-if="viewMode === 'dual' || viewMode === 'map'"
          type="GEO_MAP"
          :keywords="trendsQueue.keywords"
          :range="trendsQueue.resolvedRange"
          :revision="trendsQueue.revision"
          class="h-full min-h-0"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.trends-panel {
  box-shadow: 0 -2px 10px rgba(0, 0, 0, 0.03);
}

.resize-handle {
  transition: background-color 0.2s;
}

/* Custom minimal scrollbar for keyword lists */
.scrollbar-thin::-webkit-scrollbar {
  height: 4px;
}
.scrollbar-thin::-webkit-scrollbar-track {
  background: transparent;
}
.scrollbar-thin::-webkit-scrollbar-thumb {
  background: #cbd5e1;
  border-radius: 2px;
}
.scrollbar-thin::-webkit-scrollbar-thumb:hover {
  background: #94a3b8;
}
</style>
