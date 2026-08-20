<template>
  <Transition name="cluster-themes-slide">
    <div
      v-if="visible"
      class="absolute top-0 right-0 h-full w-96 max-w-[90vw] bg-white border-l border-slate-200 shadow-xl z-40 flex flex-col overflow-hidden"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3 border-b border-slate-100">
        <h3 class="text-sm font-semibold text-slate-800 truncate" :title="title">{{ title }}</h3>
        <div class="flex items-center gap-1">
          <button
            class="p-1 rounded hover:bg-slate-100 cursor-pointer transition-colors disabled:opacity-40 disabled:cursor-default"
            :disabled="!markdown"
            title="Copy markdown"
            @click="$emit('copy', markdown ?? '')"
          >
            <span class="material-symbols-outlined text-base text-slate-400">content_copy</span>
          </button>
          <button
            class="p-1 rounded hover:bg-slate-100 cursor-pointer transition-colors disabled:opacity-40 disabled:cursor-default"
            :disabled="loading"
            title="Re-analyze cluster"
            @click="$emit('reanalyze')"
          >
            <span class="material-symbols-outlined text-base text-slate-400">refresh</span>
          </button>
          <button
            class="p-1 rounded hover:bg-slate-100 cursor-pointer transition-colors"
            title="Close"
            @click="$emit('close')"
          >
            <span class="material-symbols-outlined text-base text-slate-400">close</span>
          </button>
        </div>
      </div>

      <!-- Loading -->
      <div v-if="loading" class="flex-1 flex flex-col items-center justify-center gap-3">
        <span class="animate-spin rounded-full h-6 w-6 border-b-2 border-indigo-600"></span>
        <p class="text-xs text-slate-500">Analyzing cluster themes with the LLM...</p>
      </div>

      <!-- Error state with retry -->
      <div v-else-if="error" class="flex-1 flex flex-col items-center justify-center gap-3 px-6">
        <span class="material-symbols-outlined text-2xl text-red-400">error</span>
        <p class="text-xs text-red-600 text-center">{{ error }}</p>
        <button
          class="px-3 py-1.5 text-xs font-medium text-indigo-700 bg-indigo-50 hover:bg-indigo-100 border border-indigo-300 rounded-lg cursor-pointer transition-colors"
          @click="$emit('reanalyze')"
        >
          Retry
        </button>
      </div>

      <!-- Markdown body -->
      <!-- eslint-disable vue/no-v-html -- trusted LLM output; protocol links render as data-attribute spans, raw HTML is escaped by the renderer -->
      <div
        v-else-if="renderedHtml"
        class="cluster-themes-panel__body flex-1 overflow-y-auto px-4 py-3 text-sm leading-relaxed text-slate-700 markdown-body"
        @click="onContentClick"
        v-html="renderedHtml"
      />
      <!-- eslint-enable vue/no-v-html -->
      <div v-else class="flex-1 flex items-center justify-center">
        <p class="text-xs text-slate-400 italic">No analysis yet</p>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Marked } from 'marked';

/**
 * Slide-over panel rendering one cluster's thematic analysis.
 *
 * Rendering contract (XSS-safe):
 * - `author:{id}` / `article:{id}` markdown links whose prefix exists in the
 *   injected `linkHandlers` registry render as clickable spans carrying
 *   data attributes (never an href).
 * - Every other markdown link renders as plain text.
 * - Raw HTML from the LLM is escaped, never passed through.
 *
 * The protocol registry (prefix -> handler) is injected by the hosting view,
 * so a future protocol (e.g. `term:`) is a new registry entry, not a panel
 * rewrite.
 */
const props = defineProps<{
  visible: boolean;
  title: string;
  markdown: string | null;
  loading: boolean;
  error: string | null;
  linkHandlers: Record<string, (id: string) => void>;
}>();

defineEmits<{
  (e: 'close'): void;
  (e: 'reanalyze'): void;
  (e: 'copy', markdown: string): void;
}>();

const PROTOCOL_LINK = /^(author|article):([A-Za-z0-9-]+)$/;

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

const parser = new Marked();
parser.use({
  renderer: {
    link(token) {
      const match = PROTOCOL_LINK.exec(token.href);
      if (match && match[1]! in props.linkHandlers) {
        return `<span class="cluster-themes-link" data-protocol="${match[1]}" data-id="${match[2]}">${escapeHtml(token.text)}</span>`;
      }
      // Unknown protocol or external URL: plain text, never an href.
      return escapeHtml(token.text);
    },
    html(token) {
      // Raw HTML from LLM output is escaped, never passed through.
      return escapeHtml(token.text);
    },
  },
});

const renderedHtml = computed<string>(() => {
  if (!props.markdown) return '';
  return parser.parse(props.markdown, { async: false }) as string;
});

/** Event-delegated click handling for the protocol spans. */
function onContentClick(event: MouseEvent): void {
  const target = (event.target as HTMLElement).closest<HTMLElement>('[data-protocol]');
  if (!target) return;
  const protocol = target.dataset.protocol;
  const id = target.dataset.id;
  const handler = protocol ? props.linkHandlers[protocol] : undefined;
  if (protocol && id && handler) handler(id);
}
</script>

<style scoped>
.cluster-themes-panel__body :deep(.cluster-themes-link) {
  color: var(--color-primary, #4f46e5);
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 2px;
}

.cluster-themes-panel__body :deep(.cluster-themes-link:hover) {
  opacity: 0.8;
}

.cluster-themes-slide-enter-active,
.cluster-themes-slide-leave-active {
  transition:
    transform 0.25s ease,
    opacity 0.25s ease;
}

.cluster-themes-slide-enter-from,
.cluster-themes-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
</style>
