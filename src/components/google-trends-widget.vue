<script setup lang="ts">
import { ref, watch, computed, onMounted, onUnmounted } from 'vue';
import { buildWidgetSrcdoc, buildExternalExploreUrl, buildEmbedUrl } from '../utils/google-trends';
import { openUrl } from '@tauri-apps/plugin-opener';
import { invoke } from '@tauri-apps/api/core';

type LoadStatus = 'idle' | 'preflight' | 'loading' | 'success' | 'error';
type ErrorReason = '429' | 'http' | 'network' | 'timeout' | 'preflight_429' | '';

interface TrendsStatus {
  ok: boolean;
  statusCode: number;
  reason: 'ok' | '429' | 'http' | 'network' | 'timeout';
}

const props = defineProps<{
  type: 'TIMESERIES' | 'GEO_MAP';
  keywords: string[];
  range: { apiTime: string; queryDate: string };
  revision: number;
  /** When false, the widget stays in idle/preflight state and renders nothing. */
  readyToRender?: boolean;
  /**
   * Set by the parent when the queue is halted due to a 429. Causes the widget
   * to skip its own probe and immediately show the 429 fallback overlay, so we
   * don't make additional requests to Google while rate-limited.
   */
  rateLimited?: boolean;
}>();

const emit = defineEmits<{
  (e: 'retry'): void;
  /** Emitted whenever the widget's load status changes (for queue orchestration). */
  (e: 'status-change', status: LoadStatus, reason: ErrorReason): void;
}>();

const srcdoc = ref('');
const loadStatus = ref<LoadStatus>('idle');
const errorReason = ref<ErrorReason>('');
let timeoutId: ReturnType<typeof setTimeout> | null = null;
let preflightAbort: { cancelled: boolean } | null = null;

const externalUrl = computed(() => buildExternalExploreUrl(props.keywords, props.range.queryDate));
const isChart = computed(() => props.type === 'TIMESERIES');

function setStatus(status: LoadStatus, reason: ErrorReason = '') {
  loadStatus.value = status;
  errorReason.value = reason;
  emit('status-change', status, reason);
}

/** Computed flag for any 429-class reason (preflight or runtime). */
const isRateLimited = computed(
  () => errorReason.value === '429' || errorReason.value === 'preflight_429'
);

/**
 * Preflight probe via the Rust side. Returns true if it's safe to render
 * the iframe, false if Google is currently rate-limiting us.
 *
 * On a 429 we DO NOT auto-retry - the queue manager is responsible for
 * halting all subsequent renders until the user manually retries.
 */
async function preflight(): Promise<boolean> {
  if (!props.keywords.length) return false;

  setStatus('preflight');
  const myToken = { cancelled: false };
  preflightAbort = myToken;

  try {
    const url = buildEmbedUrl(
      props.type,
      props.keywords,
      props.range.apiTime,
      props.range.queryDate
    );
    const result = await invoke<TrendsStatus>('check_trends_url', { url });
    if (myToken.cancelled) return false;

    if (result.ok) {
      return true;
    }
    if (result.reason === '429') {
      setStatus('error', 'preflight_429');
      return false;
    }
    // Other preflight failures (network, http) - proceed anyway, the iframe
    // might still succeed since this is just a probe.
    return true;
  } catch (err) {
    console.warn('[trends] preflight failed, proceeding to render:', err);
    if (myToken.cancelled) return false;
    return true;
  }
}

async function updateSrcdoc() {
  if (timeoutId) {
    clearTimeout(timeoutId);
    timeoutId = null;
  }
  if (preflightAbort) preflightAbort.cancelled = true;

  // Queue-level halt: show the 429 overlay without hitting Google again.
  if (props.rateLimited) {
    setStatus('error', '429');
    srcdoc.value = '';
    return;
  }

  if (!props.readyToRender) {
    setStatus('idle');
    srcdoc.value = '';
    return;
  }
  if (!props.keywords.length) {
    setStatus('idle');
    srcdoc.value = '';
    return;
  }

  const ok = await preflight();
  if (!ok) return; // error status already set inside preflight()

  setStatus('loading');
  srcdoc.value = buildWidgetSrcdoc(
    props.type,
    props.keywords,
    props.range.apiTime,
    props.range.queryDate
  );
}

function handleMessage(event: MessageEvent) {
  const data = event.data;
  if (!data || typeof data !== 'object') return;

  if (data.type === 'open_external_trends' && data.url) {
    openUrl(data.url).catch((err) => {
      console.error('Failed to open Google Trends link in browser:', err);
    });
    return;
  }

  if (data.type === 'trends_embed_status') {
    if (data.status === 'success') {
      setStatus('success');
    } else if (data.status === 'error') {
      setStatus('error', (data.reason as ErrorReason) || '');
    } else if (data.status === 'timeout') {
      // Treat watchdog timeout as a soft error - surface fallback UI.
      setStatus('error', 'timeout');
    }
  }
}

/* Watch props to debounce iframe updates. Parent handles serialization
   (chart first, then map after ~2-4s); we keep a short internal debounce
   only to coalesce rapid prop changes. */
watch(
  () => [
    props.type,
    props.keywords,
    props.range,
    props.revision,
    props.readyToRender,
    props.rateLimited,
  ],
  () => {
    if (timeoutId) clearTimeout(timeoutId);
    timeoutId = setTimeout(() => {
      updateSrcdoc();
    }, 250);
  },
  { deep: true }
);

onMounted(() => {
  window.addEventListener('message', handleMessage);
  updateSrcdoc();
});

onUnmounted(() => {
  window.removeEventListener('message', handleMessage);
  if (timeoutId) clearTimeout(timeoutId);
  if (preflightAbort) preflightAbort.cancelled = true;
});

const errorTitle = computed(() => {
  switch (errorReason.value) {
    case '429':
    case 'preflight_429':
      return 'Rate limit reached';
    case 'network':
      return "Can't reach Google";
    case 'timeout':
      return 'Widget did not respond';
    case 'http':
      return 'Google Trends error';
    default:
      return 'Unable to render widget';
  }
});

const errorMessage = computed(() => {
  switch (errorReason.value) {
    case '429':
    case 'preflight_429':
      return 'Google Trends rate limit reached. Open directly in browser with your Google account instead.';
    case 'network':
      return 'We could not reach Google Trends. Check your connection or open the search directly in your browser.';
    case 'timeout':
      return 'The widget did not finish loading in time. Try again, or open the search directly in your browser.';
    default:
      return "We can't render this widget locally right now. Please use the button below to open the search in your browser.";
  }
});

const errorIcon = computed(() => {
  switch (errorReason.value) {
    case '429':
    case 'preflight_429':
      return 'timer';
    case 'network':
      return 'cloud_off';
    case 'timeout':
      return 'hourglass_disabled';
    default:
      return 'error';
  }
});

function openExternal() {
  openUrl(externalUrl.value).catch((err) => {
    console.error('Failed to open Google Trends link in browser:', err);
  });
}

async function copyUrl() {
  try {
    await navigator.clipboard.writeText(externalUrl.value);
  } catch (err) {
    console.error('Failed to copy URL:', err);
  }
}

function retry() {
  emit('retry');
}
</script>

<template>
  <div
    class="trends-widget-container w-full h-full border border-slate-150 rounded-lg overflow-hidden shadow-sm bg-white relative"
  >
    <!-- Loading / Preflight skeleton -->
    <div
      v-if="loadStatus === 'loading' || loadStatus === 'preflight'"
      class="absolute inset-0 z-40 flex flex-col items-center justify-center bg-slate-50/80 backdrop-blur-sm"
    >
      <div class="flex flex-col items-center gap-2">
        <div class="relative w-8 h-8">
          <div class="absolute inset-0 rounded-full border-2 border-slate-200"></div>
          <div
            class="absolute inset-0 rounded-full border-2 border-transparent border-t-indigo-500 animate-spin"
          ></div>
        </div>
        <span class="text-[10px] text-slate-500 font-medium">
          {{ loadStatus === 'preflight' ? 'Checking Google Trends…' : 'Loading widget…' }}
        </span>
      </div>
    </div>

    <!--
      We do not use the sandbox attribute here because Google Trends embeds require
      proper cookie access to prevent anti-bot mechanisms from triggering 429 Rate Limit blocks.
    -->
    <iframe
      v-if="srcdoc"
      :key="srcdoc"
      :srcdoc="srcdoc"
      class="w-full h-full border-0 block bg-white"
      :class="{ 'opacity-30 pointer-events-none': loadStatus === 'error' }"
      :title="isChart ? 'Google Trends Chart' : 'Google Trends Map'"
      loading="lazy"
    />

    <!-- Empty state -->
    <div
      v-else-if="loadStatus === 'idle'"
      class="absolute inset-0 flex flex-col items-center justify-center bg-slate-50/60 text-slate-400"
    >
      <span class="material-symbols-outlined text-3xl">{{
        isChart ? 'show_chart' : 'public'
      }}</span>
      <span class="text-[10px] mt-1">Waiting for keywords…</span>
    </div>

    <!-- Error Overlay -->
    <div
      v-if="loadStatus === 'error'"
      class="absolute inset-0 z-50 flex flex-col items-center justify-center bg-white/95 backdrop-blur-sm px-4 py-3 text-center"
    >
      <span class="material-symbols-outlined text-2xl text-amber-500 mb-1">{{ errorIcon }}</span>
      <h4 class="text-[12px] font-semibold text-slate-800">{{ errorTitle }}</h4>
      <p class="text-[10px] text-slate-500 mt-1 max-w-[280px] leading-relaxed">
        {{ errorMessage }}
      </p>
      <div class="flex items-center gap-1.5 mt-2.5">
        <!-- For 429 we only offer the browser fallback; no copy/retry to avoid worsening the block. -->
        <button
          class="inline-flex items-center gap-1 bg-indigo-600 hover:bg-indigo-700 text-white text-[10px] font-semibold px-2.5 py-1 rounded transition-colors cursor-pointer shadow-sm"
          @click="openExternal"
        >
          <span class="material-symbols-outlined text-xs">open_in_new</span>
          {{ isRateLimited ? 'Open in Browser' : 'Open in Google Trends' }}
        </button>
        <button
          v-if="!isRateLimited"
          class="inline-flex items-center gap-1 bg-white hover:bg-slate-100 text-slate-600 text-[10px] font-semibold px-2 py-1 rounded border border-slate-200 transition-colors cursor-pointer"
          title="Copy search URL to clipboard"
          @click="copyUrl"
        >
          <span class="material-symbols-outlined text-xs">content_copy</span>
          Copy URL
        </button>
        <!-- On 429 do not offer an immediate retry - that would worsen the rate limit. -->
        <button
          v-if="!isRateLimited"
          class="inline-flex items-center gap-1 bg-white hover:bg-slate-100 text-slate-600 text-[10px] font-semibold px-2 py-1 rounded border border-slate-200 transition-colors cursor-pointer"
          @click="retry"
        >
          <span class="material-symbols-outlined text-xs">refresh</span>
          Retry
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.trends-widget-container {
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}
</style>
