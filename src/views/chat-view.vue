<script setup lang="ts">
import { ref, onMounted, computed, nextTick } from 'vue';
import { useRouter } from 'vue-router';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useChatStore } from '@/stores/chat';
import { useToast } from '@/composables/use-toast';
import type { Article } from '@/types';
import { marked } from 'marked';

const router = useRouter();
const toast = useToast();
const chatStore = useChatStore();

const checkingLlm = ref(true);
const isLlmConfigured = ref(false);
const articles = ref<Article[]>([]);
const showSelector = ref(false);
const searchQuery = ref('');
const inputMessage = ref('');
const chatScrollContainer = ref<HTMLElement | null>(null);

onMounted(async () => {
  await checkLlmConfig();
  if (isLlmConfigured.value) {
    await loadArticles();
    scrollToBottom();
  }
});

async function checkLlmConfig() {
  try {
    isLlmConfigured.value = await tauriCommand<boolean>('has_llm_config');
  } catch (e) {
    isLlmConfigured.value = false;
  } finally {
    checkingLlm.value = false;
  }
}

async function loadArticles() {
  try {
    const all = await tauriCommand<Article[]>('get_articles');
    // Filter out duplicates (duplicate_of is not null or status is duplicate)
    articles.value = all.filter((a) => a.status !== 'duplicate' && !a.duplicateOf);
  } catch (e) {
    toast.show('Failed to load articles list', 'error');
  }
}

function truncateString(str: string, maxLen = 20): string {
  if (!str) return '';
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 3) + '...';
}

function getAuthorText(article: Article): string {
  const author = article.authors?.[0] ?? 'Unknown';
  return truncateString(author, 20);
}

function getTitleText(article: Article): string {
  return truncateString(article.title, 20);
}

const selectedArticles = computed(() => {
  return articles.value.filter((a) => chatStore.selectedArticleIds.includes(a.id));
});

const filteredArticles = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return articles.value;
  return articles.value.filter(
    (a) =>
      a.title.toLowerCase().includes(q) ||
      a.authors.some((author) => author.toLowerCase().includes(q)) ||
      (a.journal && a.journal.toLowerCase().includes(q)) ||
      (a.publicationYear && String(a.publicationYear).includes(q))
  );
});

function toggleArticleSelection(id: string) {
  if (chatStore.selectedArticleIds.includes(id)) {
    chatStore.removeSelectedArticle(id);
  } else {
    chatStore.addSelectedArticle(id);
  }
}

async function handleSend() {
  if (!inputMessage.value.trim() || chatStore.loading) return;
  const msg = inputMessage.value;
  inputMessage.value = '';
  await chatStore.sendMessage(msg);
  scrollToBottom();
}

function scrollToBottom() {
  void nextTick(() => {
    if (chatScrollContainer.value) {
      chatScrollContainer.value.scrollTop = chatScrollContainer.value.scrollHeight;
    }
  });
}

function formatAuthorsList(authors: string[]): string {
  if (!authors || authors.length === 0) return 'Unknown';
  if (authors.length <= 2) return authors.join('; ');
  return `${authors[0]}; ${authors[1]} et al.`;
}

// Tooltip state for hovered articles in context pills
const hoveredArticle = ref<Article | null>(null);
const tooltipX = ref(0);
const tooltipY = ref(0);

function handleMouseEnter(event: MouseEvent, article: Article) {
  hoveredArticle.value = article;
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  tooltipX.value = rect.left + rect.width / 2;
  tooltipY.value = rect.top;
}

function handleMouseLeave() {
  hoveredArticle.value = null;
}
</script>

<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <div class="px-container-padding py-4 flex items-center justify-between">
      <div>
        <h1 class="page-title">Chat</h1>
        <p class="page-subtitle">RAG academic research assistant</p>
      </div>
      <button
        v-if="isLlmConfigured && chatStore.messages.length > 0"
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-200 bg-white hover:bg-slate-50 text-slate-600 hover:text-slate-900 transition-colors text-xs font-semibold"
        @click="chatStore.clearChat()"
      >
        <span class="material-symbols-outlined text-[16px]">delete</span>
        Clear Chat
      </button>
    </div>

    <!-- Spinner State -->
    <div v-if="checkingLlm" class="flex-1 flex items-center justify-center">
      <div class="text-center">
        <div
          class="animate-spin rounded-full h-8 w-8 border-b-2 border-indigo-600 mx-auto mb-4"
        ></div>
        <p class="text-sm text-slate-500">Checking LLM configuration...</p>
      </div>
    </div>

    <!-- Unconfigured LLM State -->
    <div v-else-if="!isLlmConfigured" class="flex-1 flex items-center justify-center p-6">
      <div
        class="max-w-md w-full bg-white rounded-2xl border border-slate-200 shadow-sm p-6 text-center animate-fade-in"
      >
        <div
          class="w-16 h-16 bg-amber-50 rounded-full flex items-center justify-center mx-auto mb-4 text-amber-600"
        >
          <span class="material-symbols-outlined text-[32px]">chat_error</span>
        </div>
        <h3 class="text-lg font-semibold text-slate-900 mb-2">LLM Provider Not Configured</h3>
        <p class="text-sm text-slate-500 mb-6 leading-relaxed">
          The Chat interface uses Retrieval-Augmented Generation (RAG) to query your article
          database. To enable it, please configure an LLM provider in your Settings.
        </p>
        <button
          class="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-indigo-600 hover:bg-indigo-700 text-white font-medium shadow-sm transition-colors text-sm"
          @click="router.push('/settings')"
        >
          <span class="material-symbols-outlined text-[18px]">settings</span>
          Configure LLM Settings
        </button>
      </div>
    </div>

    <!-- Main Chat Workspace -->
    <div v-else class="flex-1 flex flex-col min-h-0">
      <!-- Chat history log -->
      <div
        ref="chatScrollContainer"
        class="flex-1 overflow-y-auto p-container-padding space-y-4 flex flex-col"
      >
        <!-- Welcome state -->
        <div
          v-if="chatStore.messages.length === 0"
          class="my-auto py-12 text-center max-w-md mx-auto"
        >
          <div
            class="w-16 h-16 bg-indigo-50 rounded-full flex items-center justify-center mx-auto mb-4 text-indigo-600"
          >
            <span class="material-symbols-outlined text-[32px]">chat_add_on</span>
          </div>
          <h3 class="text-lg font-semibold text-slate-900 mb-2">Academic Research Chat</h3>
          <p class="text-sm text-slate-500 leading-relaxed mb-6">
            Ask questions about the articles in your library. Add articles to the context using the
            `+` button to ground the responses in specific research text.
          </p>
        </div>

        <template v-else>
          <div
            v-for="(msg, idx) in chatStore.messages"
            :key="idx"
            class="flex flex-col max-w-[80%]"
            :class="
              msg.role === 'user'
                ? 'self-end items-end animate-slide-in-right'
                : 'self-start items-start animate-slide-in-left'
            "
          >
            <!-- Sender details -->
            <span class="text-[11px] text-slate-400 mb-1 font-medium px-1">
              {{ msg.role === 'user' ? 'You' : 'Assistant' }} &bull; {{ msg.timestamp }}
            </span>
            <!-- Bubble -->
            <div
              class="px-4 py-3 rounded-2xl text-sm leading-relaxed"
              :class="
                msg.role === 'user'
                  ? 'bg-indigo-600 text-white rounded-tr-none shadow-sm shadow-indigo-200'
                  : 'bg-white text-slate-800 border border-slate-200 rounded-tl-none shadow-sm markdown-body'
              "
            >
              <template v-if="msg.role === 'user'">
                <div style="white-space: pre-wrap">{{ msg.content }}</div>
              </template>
              <template v-else>
                <!-- eslint-disable-next-line vue/no-v-html -- trusted LLM output rendered via marked -->
                <div class="markdown-content" v-html="marked.parse(msg.content)" />
              </template>
            </div>
          </div>
        </template>

        <!-- Loading / Thinking indicator -->
        <div
          v-if="chatStore.loading"
          class="flex flex-col items-start max-w-[80%] self-start animate-pulse"
        >
          <span class="text-[11px] text-slate-400 mb-1 font-medium px-1">
            Assistant &bull; Thinking
          </span>
          <div
            class="px-4 py-3 rounded-2xl bg-white text-slate-500 border border-slate-200 rounded-tl-none shadow-sm flex items-center gap-2"
          >
            <div class="flex gap-1">
              <span class="dot-1 w-1.5 h-1.5 bg-indigo-600 rounded-full"></span>
              <span class="dot-2 w-1.5 h-1.5 bg-indigo-600 rounded-full"></span>
              <span class="dot-3 w-1.5 h-1.5 bg-indigo-600 rounded-full"></span>
            </div>
            <span class="text-xs">Analyzing article context...</span>
          </div>
        </div>
      </div>

      <!-- Context pills and Input area -->
      <div class="border-t border-slate-200 bg-white p-4">
        <!-- Selected articles panel -->
        <div class="mb-3">
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">
              Selected Context ({{ selectedArticles.length }})
            </span>
            <button
              v-if="selectedArticles.length > 0"
              class="text-[11px] text-indigo-600 hover:text-indigo-800 font-semibold"
              @click="chatStore.clearSelectedArticles()"
            >
              Clear Context
            </button>
          </div>

          <!-- Horizontal scrolling pills -->
          <div class="flex flex-wrap gap-2 max-h-32 overflow-y-auto py-1">
            <div
              v-for="art in selectedArticles"
              :key="art.id"
              class="relative flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-slate-100 border border-slate-200 text-xs text-slate-700 hover:bg-slate-200 transition-colors cursor-help"
              @mouseenter="handleMouseEnter($event, art)"
              @mouseleave="handleMouseLeave"
            >
              <span class="font-semibold text-slate-800">{{ getAuthorText(art) }}</span>
              <span class="text-slate-500">({{ art.publicationYear ?? 'N/A' }})</span>
              <span class="text-slate-400">-</span>
              <span class="truncate max-w-[120px]">{{ getTitleText(art) }}</span>
              <button
                class="flex items-center justify-center w-4 h-4 rounded-full bg-slate-200 hover:bg-slate-300 text-slate-500 hover:text-slate-800 ml-1 transition-colors"
                @click="
                  chatStore.removeSelectedArticle(art.id);
                  handleMouseLeave();
                "
              >
                <span class="material-symbols-outlined text-[12px]">close</span>
              </button>
            </div>

            <p v-if="selectedArticles.length === 0" class="text-xs text-slate-400 italic py-1">
              No articles added. Click (+) to select articles from your library to include in this
              query.
            </p>
          </div>
        </div>

        <!-- Chat bar input container -->
        <div class="flex items-center gap-3">
          <!-- Plus button -->
          <button
            class="flex items-center justify-center w-10 h-10 rounded-full border border-slate-200 hover:bg-slate-50 text-indigo-600 transition-all active:scale-95 flex-shrink-0"
            title="Add article context"
            @click="showSelector = true"
          >
            <span class="material-symbols-outlined text-[24px]">add</span>
          </button>

          <!-- Input field -->
          <div class="flex-1 relative">
            <input
              v-model="inputMessage"
              type="text"
              placeholder="Ask a question about the selected articles..."
              class="w-full pl-4 pr-12 py-2.5 rounded-full border border-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 text-sm transition-all"
              @keydown.enter="handleSend"
            />

            <!-- Submit button -->
            <button
              class="absolute right-1.5 top-1/2 -translate-y-1/2 flex items-center justify-center w-8 h-8 rounded-full bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-40 transition-colors cursor-pointer"
              :disabled="!inputMessage.trim() || chatStore.loading"
              @click="handleSend"
            >
              <span class="material-symbols-outlined text-[18px]">send</span>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Article Selection Modal -->
    <Teleport to="body">
      <div
        v-if="showSelector"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4"
        @click.self="showSelector = false"
      >
        <div
          class="bg-white rounded-2xl shadow-xl w-full max-w-2xl max-h-[80vh] flex flex-col border border-slate-100 overflow-hidden animate-zoom-in"
        >
          <!-- Modal Header -->
          <div class="px-6 py-4 border-b border-slate-150 flex items-center justify-between">
            <div>
              <h3 class="text-base font-bold text-slate-900">Include Articles in Context</h3>
              <p class="text-xs text-slate-500">
                Search and toggle articles to provide as background knowledge
              </p>
            </div>
            <button
              class="w-8 h-8 rounded-full hover:bg-slate-100 flex items-center justify-center text-slate-500 transition-colors"
              @click="showSelector = false"
            >
              <span class="material-symbols-outlined text-[20px]">close</span>
            </button>
          </div>

          <!-- Search Bar -->
          <div class="px-6 py-3 border-b border-slate-100 bg-slate-50/50">
            <div class="relative">
              <span
                class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 text-[18px]"
                >search</span
              >
              <input
                v-model="searchQuery"
                type="text"
                placeholder="Search by title, authors, or journal..."
                class="w-full pl-9 pr-4 py-2 rounded-xl border border-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 text-sm bg-white"
              />
            </div>
          </div>

          <!-- Articles list -->
          <div class="flex-1 overflow-y-auto p-4 divide-y divide-slate-100">
            <div
              v-for="art in filteredArticles"
              :key="art.id"
              class="flex items-center gap-4 py-3 px-2.5 hover:bg-slate-50 rounded-xl cursor-pointer transition-colors"
              @click="toggleArticleSelection(art.id)"
            >
              <!-- Checkbox -->
              <input
                type="checkbox"
                class="accent-indigo-600 rounded cursor-pointer w-4 h-4 flex-shrink-0"
                :checked="chatStore.selectedArticleIds.includes(art.id)"
                @click.stop="toggleArticleSelection(art.id)"
              />

              <!-- Info -->
              <div class="flex-1 min-w-0">
                <p class="text-sm font-semibold text-slate-900 truncate mb-0.5">
                  {{ art.title }}
                </p>
                <div class="flex items-center gap-2 text-xs text-slate-500">
                  <span class="font-medium text-slate-600">{{
                    formatAuthorsList(art.authors)
                  }}</span>
                  <span class="text-slate-300">&bull;</span>
                  <span>{{ art.publicationYear ?? 'N/A' }}</span>
                  <span v-if="art.journal" class="text-slate-300">&bull;</span>
                  <span v-if="art.journal" class="italic truncate max-w-[150px]">{{
                    art.journal
                  }}</span>
                </div>
              </div>

              <!-- Status badge -->
              <div
                class="px-2 py-0.5 rounded text-[10px] font-semibold uppercase tracking-wider"
                :class="{
                  'bg-emerald-50 text-emerald-700': art.status === 'included',
                  'bg-indigo-50 text-indigo-700': art.status === 'working',
                  'bg-red-50 text-red-700': art.status === 'rejected',
                }"
              >
                {{ art.status }}
              </div>
            </div>

            <!-- Empty Selector State -->
            <div
              v-if="filteredArticles.length === 0"
              class="text-center py-12 text-slate-400 text-sm"
            >
              No matching articles found in your library.
            </div>
          </div>

          <!-- Footer -->
          <div
            class="px-6 py-4 border-t border-slate-100 bg-slate-50/50 flex justify-between items-center text-xs"
          >
            <span class="text-slate-500">
              {{ chatStore.selectedArticleIds.length }} article(s) selected
            </span>
            <button
              class="px-4 py-2 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg font-semibold shadow-sm transition-colors text-xs"
              @click="showSelector = false"
            >
              Done
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Floating Tooltip for pills -->
    <Teleport to="body">
      <Transition name="tooltip-fade">
        <div
          v-if="hoveredArticle"
          class="fixed z-50 w-80 p-3 rounded-xl bg-slate-900 text-white text-[11px] leading-normal shadow-xl border border-slate-800 flex flex-col gap-1 pointer-events-none text-left"
          :style="{
            left: tooltipX + 'px',
            top: tooltipY + 'px',
            transform: 'translate(-50%, -108%)',
          }"
        >
          <div class="font-bold text-slate-400">Title</div>
          <div class="font-medium text-white break-words">{{ hoveredArticle.title }}</div>
          <div class="font-bold text-slate-400 mt-1">Authors</div>
          <div class="text-slate-300 break-words">
            {{ formatAuthorsList(hoveredArticle.authors) }}
          </div>
          <!-- Tooltip arrow -->
          <div
            class="absolute top-full left-1/2 -translate-x-1/2 border-4 border-transparent border-t-slate-900"
          ></div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped>
@keyframes slide-in-right {
  from {
    transform: translateX(12px);
    opacity: 0;
  }
  to {
    transform: translateX(0);
    opacity: 1;
  }
}

@keyframes slide-in-left {
  from {
    transform: translateX(-12px);
    opacity: 0;
  }
  to {
    transform: translateX(0);
    opacity: 1;
  }
}

@keyframes zoom-in {
  from {
    transform: scale(0.95);
    opacity: 0;
  }
  to {
    transform: scale(1);
    opacity: 1;
  }
}

@keyframes fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.animate-slide-in-right {
  animation: slide-in-right 0.25s cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

.animate-slide-in-left {
  animation: slide-in-left 0.25s cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

.animate-zoom-in {
  animation: zoom-in 0.2s cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

.animate-fade-in {
  animation: fade-in 0.3s ease-out forwards;
}

.dot-1,
.dot-2,
.dot-3 {
  animation: bounce 1.4s infinite ease-in-out both;
}
.dot-1 {
  animation-delay: -0.32s;
}
.dot-2 {
  animation-delay: -0.16s;
}

@keyframes bounce {
  0%,
  80%,
  100% {
    transform: scale(0);
  }
  40% {
    transform: scale(1);
  }
}

/* Markdown styling in chat bubble */
.markdown-content :deep(p) {
  margin-bottom: 0.5rem;
}
.markdown-content :deep(p:last-child) {
  margin-bottom: 0;
}
.markdown-content :deep(h1),
.markdown-content :deep(h2),
.markdown-content :deep(h3) {
  font-weight: 600;
  margin-top: 0.75rem;
  margin-bottom: 0.375rem;
  color: var(--color-on-surface, #0f172a);
}
.markdown-content :deep(h1) {
  font-size: 1.15rem;
}
.markdown-content :deep(h2) {
  font-size: 1.05rem;
}
.markdown-content :deep(h3) {
  font-size: 0.95rem;
}
.markdown-content :deep(ul),
.markdown-content :deep(ol) {
  padding-left: 1.25rem;
  margin-bottom: 0.5rem;
}
.markdown-content :deep(ul) {
  list-style-type: disc;
}
.markdown-content :deep(ol) {
  list-style-type: decimal;
}
.markdown-content :deep(li) {
  margin-bottom: 0.25rem;
}
.markdown-content :deep(strong) {
  font-weight: 600;
}
.markdown-content :deep(em) {
  font-style: italic;
}
.markdown-content :deep(code) {
  background-color: #f1f5f9;
  padding: 2px 4px;
  border-radius: 4px;
  font-size: 0.85em;
  font-family: monospace;
}
.markdown-content :deep(pre) {
  background-color: #f1f5f9;
  padding: 0.5rem;
  border-radius: 6px;
  overflow-x: auto;
  margin: 0.5rem 0;
}
.markdown-content :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 0.5rem 0;
  font-size: 0.85rem;
}
.markdown-content :deep(th),
.markdown-content :deep(td) {
  border: 1px solid #e2e8f0;
  padding: 0.375rem 0.5rem;
  text-align: left;
}
.markdown-content :deep(th) {
  background-color: #f8fafc;
  font-weight: 600;
}

/* Tooltip animation */
.tooltip-fade-enter-active,
.tooltip-fade-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}
.tooltip-fade-enter-from,
.tooltip-fade-leave-to {
  opacity: 0;
  transform: translate(-50%, -100%) scale(0.95) !important;
}
</style>
