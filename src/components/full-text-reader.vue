<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { openPath } from '@tauri-apps/plugin-opener';
import type { Article } from '@/types';
import { requestArticleAiSummary } from '@/composables/use-ai-summary';
import { getFullTextFileIcon } from '@/utils/formatters';

const props = defineProps<{
  article: Article;
  fullScreen: boolean;
  canRequestAiSummary: boolean;
  isAiSummaryPending: boolean;
  openReaderId: string | null;
}>();

const emit = defineEmits<{
  toggleFullScreen: [];
  deleteFullText: [id: string];
  refreshArticle: [id: string];
  requestOpen: [];
  readerOpened: [];
}>();

const showFullTextView = ref(false);
const fullTextContent = ref<string | null>(null);
const fullTextExpanded = ref(false);
const pdfSrc = ref<string | null>(null);
const absoluteFilePath = ref<string | null>(null);

/** Whether the attached file is a PDF */
const isPdfAttachment = computed(() => {
  const name = props.article.fullTextFileName;
  return !!name && name.toLowerCase().endsWith('.pdf');
});

/** Determine the file type icon based on filename */
const fullTextFileIcon = computed(() => getFullTextFileIcon(props.article.fullTextFileName));

/** Open the full-text reading view */
async function openFullTextView(): Promise<void> {
  showFullTextView.value = true;
  fullTextContent.value = props.article.fullText;
  pdfSrc.value = null;
  absoluteFilePath.value = null;

  if (isPdfAttachment.value) {
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    try {
      const bytes = await tauriCommand<ArrayBuffer | null>('read_full_text_file_bytes', {
        articleId: props.article.id,
      });
      if (bytes) {
        const blob = new Blob([new Uint8Array(bytes as unknown as ArrayLike<number>)], {
          type: 'application/pdf',
        });
        pdfSrc.value = URL.createObjectURL(blob);
      }
    } catch (e) {
      console.warn('Failed to load PDF bytes for inline viewing:', e);
    }
    const filePath = await tauriCommand<string | null>('get_full_text_file_path', {
      articleId: props.article.id,
    });
    if (filePath) {
      absoluteFilePath.value = filePath;
    }
  }
}

/** Open the attached file in the platform's default viewer */
async function openFileExternally(): Promise<void> {
  if (!absoluteFilePath.value) {
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    const filePath = await tauriCommand<string | null>('get_full_text_file_path', {
      articleId: props.article.id,
    });
    if (filePath) {
      absoluteFilePath.value = filePath;
    }
  }
  if (absoluteFilePath.value) {
    await openPath(absoluteFilePath.value);
  }
}

/** Revoke the Blob URL to free memory */
function revokePdfSrc(): void {
  if (pdfSrc.value && pdfSrc.value.startsWith('blob:')) {
    URL.revokeObjectURL(pdfSrc.value);
  }
}

/** Close the full-text reading view */
function closeFullTextView(): void {
  revokePdfSrc();
  pdfSrc.value = null;
  showFullTextView.value = false;
  fullTextExpanded.value = false;
}

/** Toggle full-text expand */
function toggleFullTextExpand(): void {
  fullTextExpanded.value = !fullTextExpanded.value;
  if (fullTextExpanded.value && !props.fullScreen) {
    emit('toggleFullScreen');
  }
}

/** Delete the full-text attachment */
function handleDeleteFullText(): void {
  emit('deleteFullText', props.article.id);
  showFullTextView.value = false;
  fullTextExpanded.value = false;
}

/** Trigger AI summary generation */
function handleRequestAiSummary(): void {
  requestArticleAiSummary(props.article.id, props.article.title, async (articleId: string) => {
    emit('refreshArticle', articleId);
  });
}

// Reset when article changes
watch(
  () => props.article.id,
  () => {
    showFullTextView.value = false;
    fullTextExpanded.value = false;
    fullTextContent.value = null;
    pdfSrc.value = null;
    absoluteFilePath.value = null;
  }
);

// Auto-open reader when triggered from the article table
watch(
  [() => props.openReaderId, () => props.article.id],
  ([readerId, articleId]) => {
    if (readerId && readerId === articleId && !showFullTextView.value) {
      void nextTick().then(() => {
        if (!showFullTextView.value) {
          void openFullTextView();
          emit('readerOpened');
        }
      });
    }
  },
  { immediate: true }
);

// Expose openFullTextView so parent can trigger it
defineExpose({ openFullTextView, fullTextFileIcon });
</script>

<template>
  <!-- Full-Text Reading View Overlay -->
  <div v-if="showFullTextView" class="absolute inset-0 z-50 bg-white flex flex-col">
    <!-- Full-text header bar -->
    <div
      class="flex items-center justify-between px-4 py-3 border-b border-slate-200 bg-slate-50 shrink-0"
    >
      <div class="flex items-center gap-2">
        <span
          class="material-symbols-outlined text-[18px]"
          :class="
            article.fullTextFileName?.toLowerCase().endsWith('.pdf')
              ? 'text-red-500'
              : 'text-blue-500'
          "
        >
          {{ fullTextFileIcon ?? 'description' }}
        </span>
        <span class="text-sm font-semibold text-slate-700 truncate max-w-[200px]">
          {{ article.fullTextFileName ?? 'Full Text' }}
        </span>
      </div>
      <div class="flex items-center gap-1">
        <!-- AI Summary trigger in reading view -->
        <button
          v-if="canRequestAiSummary"
          class="material-symbols-outlined text-[18px] text-violet-500 hover:text-violet-700 hover:bg-violet-50 cursor-pointer rounded px-1 transition-colors"
          title="Generate AI summary from full text"
          @click="handleRequestAiSummary"
        >
          auto_awesome
        </button>
        <span
          v-else-if="isAiSummaryPending"
          class="material-symbols-outlined text-[18px] text-violet-400 animate-spin px-1"
          title="AI summary in progress..."
        >
          progress_activity
        </span>
        <button
          class="material-symbols-outlined text-[18px] text-slate-400 hover:text-slate-900 cursor-pointer rounded px-1 transition-colors"
          :title="fullTextExpanded ? 'Collapse' : 'Expand to full width'"
          @click="toggleFullTextExpand"
        >
          {{ fullTextExpanded ? 'close_fullscreen' : 'open_in_full' }}
        </button>
        <button
          class="material-symbols-outlined text-[18px] text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer rounded px-1 transition-colors"
          title="Open in system viewer"
          @click="openFileExternally"
        >
          open_in_new
        </button>
        <button
          class="material-symbols-outlined text-[18px] text-red-400 hover:text-red-600 hover:bg-red-50 cursor-pointer rounded px-1 transition-colors"
          title="Delete full text attachment"
          @click="handleDeleteFullText"
        >
          delete
        </button>
        <button
          class="material-symbols-outlined text-[18px] text-slate-400 hover:text-slate-900 cursor-pointer rounded px-1 transition-colors"
          title="Close full text view"
          @click="closeFullTextView"
        >
          close
        </button>
      </div>
    </div>
    <!-- PDF inline viewer using Blob URL -->
    <div v-if="isPdfAttachment && pdfSrc" class="flex-1 overflow-hidden">
      <iframe :src="pdfSrc" class="w-full h-full border-0" title="PDF Viewer" />
    </div>
    <!-- Fallback: extracted text -->
    <div v-else class="flex-1 overflow-y-auto p-6">
      <pre
        v-if="fullTextContent || article.fullText"
        class="whitespace-pre-wrap font-body-main text-body-main text-on-surface leading-relaxed break-words"
        >{{ fullTextContent ?? article.fullText }}</pre
      >
      <div v-else class="text-center py-16 text-slate-400 text-sm">
        No full text content available.
      </div>
    </div>
  </div>
</template>
