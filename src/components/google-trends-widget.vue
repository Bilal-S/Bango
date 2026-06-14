<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue';
import { buildWidgetSrcdoc } from '../utils/google-trends';
import { openUrl } from '@tauri-apps/plugin-opener';

const props = defineProps<{
  type: 'TIMESERIES' | 'GEO_MAP';
  keywords: string[];
  range: { apiTime: string; queryDate: string };
  revision: number;
}>();

const srcdoc = ref('');
let timeoutId: ReturnType<typeof setTimeout> | null = null;

function updateSrcdoc() {
  srcdoc.value = buildWidgetSrcdoc(
    props.type,
    props.keywords,
    props.range.apiTime,
    props.range.queryDate
  );
}

function handleMessage(event: MessageEvent) {
  if (event.data && event.data.type === 'open_external_trends' && event.data.url) {
    openUrl(event.data.url).catch((err) => {
      console.error('Failed to open Google Trends link in browser:', err);
    });
  }
}

// Watch props to debounce iframe updates and avoid rate-limiting (429) from Google Trends
watch(
  () => [props.type, props.keywords, props.range, props.revision],
  () => {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(() => {
      updateSrcdoc();
    }, 600); // 600ms debounce
  },
  { deep: true }
);

onMounted(() => {
  window.addEventListener('message', handleMessage);
  updateSrcdoc();
});

onUnmounted(() => {
  window.removeEventListener('message', handleMessage);
  if (timeoutId) {
    clearTimeout(timeoutId);
  }
});
</script>

<template>
  <div
    class="trends-widget-container w-full h-full border border-slate-150 rounded-lg overflow-hidden shadow-sm bg-white relative"
  >
    <!-- 
      We do not use the sandbox attribute here because Google Trends embeds require 
      proper cookie access to prevent anti-bot mechanisms from triggering 429 Rate Limit blocks.
    -->
    <iframe
      :key="srcdoc"
      :srcdoc="srcdoc"
      class="w-full h-full border-0 block bg-white"
      :title="type === 'TIMESERIES' ? 'Google Trends Chart' : 'Google Trends Map'"
      loading="lazy"
    />
  </div>
</template>

<style scoped>
.trends-widget-container {
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}
</style>
