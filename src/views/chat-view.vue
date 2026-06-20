<script setup lang="ts">
import { ref, onMounted, computed, nextTick, watch } from 'vue';
import { useRouter } from 'vue-router';
import { open } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useChatStore } from '@/stores/chat';
import { useToast } from '@/composables/use-toast';
import type { Article } from '@/types';
import type { WikiStatus } from '@/types/wiki';
import { marked } from 'marked';
import { renderWikiMarkdown } from '@/utils/wiki-markdown';
import { useArticleSearch } from '@/composables/use-article-search';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import WikiPageViewer from '@/components/wiki/wiki-page-viewer.vue';

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

const isDetailFullScreen = ref(false);

// Wiki-mode UI state.
const checkingWiki = ref(true);
/** Wiki reader slide-over navigation stack. The last entry is the visible page;
 *  popping back to empty closes the panel. Keeping a stack lets inner
 *  [[wikilink]] clicks chain while preserving the chat thread underneath. */
const wikiNavStack = ref<string[]>([]);
const wikiPanelOpen = computed(() => wikiNavStack.value.length > 0);
const wikiSlug = computed(() => wikiNavStack.value[wikiNavStack.value.length - 1] ?? null);

const {
  selectedArticle: detailArticle,
  auditTrail: detailAuditTrail,
  selectArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  moveArticle,
  attachFullText,
  deleteFullTextAttachment,
} = useArticleSearch();

// Synchronize updates from the detail view back into the chat's article list
watch(detailArticle, (newVal) => {
  if (newVal) {
    const idx = articles.value.findIndex((a) => a.id === newVal.id);
    if (idx >= 0) {
      articles.value[idx] = newVal;
    }
  } else {
    isDetailFullScreen.value = false;
  }
});

onMounted(async () => {
  await checkLlmConfig();
  if (isLlmConfigured.value) {
    await Promise.all([loadArticles(), checkWikiStatus()]);
    scrollToBottom();
  }
});

async function checkLlmConfig() {
  try {
    isLlmConfigured.value = await tauriCommand<boolean>('has_llm_config');
  } catch {
    isLlmConfigured.value = false;
  } finally {
    checkingLlm.value = false;
  }
}

/** Fetch wiki status and flip the store's `wikiReady` flag (drives toggle
 *  visibility). The wiki toggle only appears when the wiki is initialized AND
 *  has at least one page. */
async function checkWikiStatus() {
  try {
    const status = await tauriCommand<WikiStatus>('wiki_get_status');
    chatStore.setWikiReady(!!status.initialized && status.pageCount > 0);
    // If the wiki became unavailable while wiki mode was on, drop back to articles.
    if (!chatStore.wikiReady && chatStore.source === 'wiki') {
      chatStore.setSource('articles');
    }
  } catch {
    chatStore.setWikiReady(false);
  } finally {
    checkingWiki.value = false;
  }
}

async function loadArticles() {
  try {
    const all = await tauriCommand<Article[]>('get_articles');
    // Filter out duplicates (duplicate_of is not null or status is duplicate)
    articles.value = all.filter((a) => a.status !== 'duplicate' && !a.duplicateOf);
  } catch {
    toast.show('Failed to load articles list', 'error');
  }
}

function truncateString(str: string, maxLen = 20): string {
  if (!str) return '';
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 3) + '...';
}

// Format author to be up to 20 chars
function getAuthorText(article: Article): string {
  const author = article.authors?.[0] ?? 'Unknown';
  return truncateString(author, 20);
}

// Format title to be up to 20 chars
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

/** Flip the wiki / article retrieval mode. */
function onToggleWiki() {
  const next = chatStore.toggleWikiMode();
  if (next === 'wiki') {
    // Entering wiki mode: article context is irrelevant, hide it to reduce clutter.
    toast.show('Wiki mode: answers are grounded by FTS5 search over your wiki pages.', 'info');
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

/**
 * Render an assistant message body. Wiki-sourced messages go through the shared
 * wiki renderer so `[[slug]]` citations become clickable `.wikilink` spans;
 * article-sourced messages use plain `marked` (no wikilink interpretation, so
 * bracketed text in article content is never misinterpreted).
 */
function renderMessage(msg: { role: string; content: string; source?: string }): string {
  if (msg.source === 'wiki') {
    return renderWikiMarkdown(msg.content);
  }
  return marked.parse(msg.content) as string;
}

/** Delegated click handler for assistant bubbles: detect wiki links and
 *  article references and route them to the right slide-over. */
function handleBubbleClick(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (target.classList.contains('wikilink')) {
    const slug = target.getAttribute('data-slug');
    if (slug) openWikiPage(slug);
  } else if (target.classList.contains('art-ref')) {
    const artId = target.getAttribute('data-art-id');
    if (artId) void openArticleDetail(artId);
  }
}

/** Open the wiki reader slide-over on a given slug. Closes the article panel so
 *  only one slide-over is visible at a time. */
function openWikiPage(slug: string) {
  // Mutually exclusive with the article detail panel.
  detailArticle.value = null;
  wikiNavStack.value = [slug];
}

/** Inner [[wikilink]] navigation: push onto the stack so the back button works. */
function navigateWiki(slug: string) {
  wikiNavStack.value = [...wikiNavStack.value, slug];
}

/** Pop the wiki reader back-stack; close the panel when the stack is empty. */
function goBackWiki() {
  wikiNavStack.value = wikiNavStack.value.slice(0, -1);
}

/** Close the wiki reader entirely (clears history). */
function closeWikiPanel() {
  wikiNavStack.value = [];
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

async function openArticleDetail(articleId: string) {
  // Mutually exclusive with the wiki reader panel.
  closeWikiPanel();
  try {
    await selectArticle(articleId);
  } catch {
    toast.show('Failed to load article details', 'error');
  }
}

async function handleAttachFullText(articleId: string): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: 'Documents',
          extensions: ['pdf', 'txt'],
        },
      ],
    });
    if (!selected) return;
    toast.show('Importing full text…', 'info');
    await attachFullText(articleId, selected);
    toast.show('Full text attached successfully.', 'success');
  } catch {
    toast.show('Failed to attach full text', 'error');
  }
}
</script>

<template>
  <div class="h-full flex flex-row overflow-hidden">
    <!-- Left Workspace: Chat Interface -->
    <div v-show="!isDetailFullScreen" class="flex-1 flex flex-col min-h-0 bg-slate-50/20">
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
              Ask questions about the articles in your library. Add articles to the context using
              the `+` button to ground the responses in specific research text.
              <span v-if="chatStore.wikiReady">
                Toggle the <strong>Wiki</strong> button to answer from your synthesized knowledge
                base instead.
              </span>
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
              <span
                class="text-[11px] text-slate-400 mb-1 font-medium px-1 flex items-center gap-1"
              >
                {{ msg.role === 'user' ? 'You' : 'Assistant' }} &bull; {{ msg.timestamp }}
                <span
                  v-if="msg.source === 'wiki'"
                  class="wiki-badge"
                  title="Answer grounded by FTS5 search over your wiki pages"
                  >wiki</span
                >
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
                  <div @click="handleBubbleClick">
                    <!-- eslint-disable-next-line vue/no-v-html -- trusted LLM output; wiki links sanitized to data attributes -->
                    <div class="markdown-content" v-html="renderMessage(msg)" />
                  </div>
                </template>
              </div>
            </div>
          </template>

          <!-- Loading / Thinking indicator -->
          <div
            v-if="chatStore.loading"
            class="flex flex-col items-start max-w-[80%] self-start animate-pulse"
          >
            <span class="text-[11px] text-slate-400 mb-1 font-medium px-1 flex items-center gap-1">
              Assistant &bull; Thinking
              <span v-if="chatStore.source === 'wiki'" class="wiki-badge">wiki</span>
            </span>
            <div
              class="px-4 py-3 rounded-2xl bg-white text-slate-500 border border-slate-200 rounded-tl-none shadow-sm flex items-center gap-2"
            >
              <div class="flex gap-1">
                <span class="dot-1 w-1.5 h-1.5 bg-indigo-600 rounded-full"></span>
                <span class="dot-2 w-1.5 h-1.5 bg-indigo-600 rounded-full"></span>
                <span class="dot-3 w-1.5 h-1.5 bg-indigo-600 rounded-full"></span>
              </div>
              <span class="text-xs">{{
                chatStore.source === 'wiki'
                  ? 'Searching wiki pages...'
                  : 'Analyzing article context...'
              }}</span>
            </div>
          </div>
        </div>

        <!-- Context pills and Input area -->
        <div class="border-t border-slate-200 bg-white p-4">
          <!-- Wiki-mode banner (replaces the article context picker) -->
          <div v-if="chatStore.source === 'wiki'" class="mb-3">
            <div class="wiki-banner flex items-center gap-2">
              <span class="material-symbols-outlined text-[16px]">local_library</span>
              <span class="text-xs font-semibold text-indigo-700"
                >Wiki mode: answers are grounded by FTS5 search over your wiki pages.</span
              >
              <button
                class="ml-auto text-[11px] text-indigo-600 hover:text-indigo-800 font-semibold"
                @click="router.push('/wiki')"
              >
                Open Wiki
              </button>
            </div>
          </div>

          <!-- Selected articles panel (article mode only) -->
          <div v-else class="mb-3">
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
                class="relative flex items-center rounded-full bg-slate-100 border border-slate-200 text-xs text-slate-700 hover:bg-slate-200 transition-colors"
              >
                <!-- Info text area with help cursor and hover tooltip trigger -->
                <div
                  class="flex items-center gap-1.5 pl-3 py-1.5 pr-2 cursor-help rounded-l-full"
                  @mouseenter="handleMouseEnter($event, art)"
                  @mouseleave="handleMouseLeave"
                >
                  <span class="font-semibold text-slate-800">{{ getAuthorText(art) }}</span>
                  <span class="text-slate-500">({{ art.publicationYear ?? 'N/A' }})</span>
                  <span class="text-slate-400">-</span>
                  <span class="truncate max-w-[120px]">{{ getTitleText(art) }}</span>
                </div>

                <!-- Control actions area (does NOT trigger hover tooltip, has pointer cursor) -->
                <div
                  class="flex items-center gap-1.5 pr-3 py-1 border-l border-slate-200/60 pl-2 rounded-r-full"
                >
                  <!-- Open In New details action -->
                  <button
                    class="flex items-center justify-center w-5 h-5 rounded-full hover:bg-slate-300 text-slate-500 hover:text-indigo-600 transition-colors cursor-pointer"
                    title="Open article details"
                    @click="openArticleDetail(art.id)"
                  >
                    <span class="material-symbols-outlined text-[14px]">open_in_new</span>
                  </button>

                  <!-- Close button -->
                  <button
                    class="flex items-center justify-center w-5 h-5 rounded-full hover:bg-slate-300 text-slate-500 hover:text-rose-600 transition-colors cursor-pointer"
                    title="Remove from context"
                    @click="
                      chatStore.removeSelectedArticle(art.id);
                      handleMouseLeave();
                    "
                  >
                    <span class="material-symbols-outlined text-[14px]">close</span>
                  </button>
                </div>
              </div>

              <p v-if="selectedArticles.length === 0" class="text-xs text-slate-400 italic py-1">
                No articles added. Click (+) to select articles from your library to include in this
                query.
              </p>
            </div>
          </div>

          <!-- Chat bar input container -->
          <div class="flex items-center gap-3">
            <!-- Plus button (article mode only; hidden in wiki mode) -->
            <button
              v-if="chatStore.source === 'articles'"
              class="flex items-center justify-center w-10 h-10 rounded-full border border-slate-200 hover:bg-slate-50 text-indigo-600 transition-all active:scale-95 flex-shrink-0"
              title="Add article context"
              @click="showSelector = true"
            >
              <span class="material-symbols-outlined text-[24px]">add</span>
            </button>

            <!-- Wiki toggle button. Always adjacent to (+) when visible. Halo + indigo fill when active. -->
            <button
              v-if="chatStore.wikiReady"
              class="wiki-toggle"
              :class="{ 'wiki-toggle--active': chatStore.source === 'wiki' }"
              :title="
                chatStore.source === 'wiki'
                  ? 'Wiki mode active. Click to return to article context.'
                  : 'Answer from your wiki knowledge base (FTS5 search)'
              "
              :aria-pressed="chatStore.source === 'wiki'"
              @click="onToggleWiki"
            >
              <span class="material-symbols-outlined text-[24px]">local_library</span>
            </button>

            <!-- Input field -->
            <div class="flex-1 relative">
              <input
                v-model="inputMessage"
                type="text"
                :placeholder="
                  chatStore.source === 'wiki'
                    ? 'Ask a question about your wiki...'
                    : 'Ask a question about the selected articles...'
                "
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
    </div>
    <!-- Left Workspace: Chat Interface -->

    <!-- Right Workspace: Article Details Side Panel -->
    <Transition name="slide">
      <ArticleDetailPanel
        v-if="detailArticle"
        :article="detailArticle"
        :audit-trail="detailAuditTrail"
        :has-previous="false"
        :has-next="false"
        :has-return-target="false"
        :full-screen="isDetailFullScreen"
        :article-position="1"
        :article-total="1"
        @close="detailArticle = null"
        @toggle-full-screen="isDetailFullScreen = !isDetailFullScreen"
        @update-notes="updateNotes"
        @update-tags="updateTags"
        @update-labels="updateLabels"
        @update-criteria="updateCriteria"
        @move-article="moveArticle"
        @attach-full-text="handleAttachFullText"
        @delete-full-text="deleteFullTextAttachment"
        @refresh-article="selectArticle"
      />
    </Transition>

    <!-- Wiki reader slide-over. Floats on the right; opening it closes the article panel. -->
    <Transition name="slide">
      <div v-if="wikiPanelOpen" class="wiki-reader">
        <div class="wiki-reader__chrome">
          <button
            v-if="wikiNavStack.length > 1"
            class="wiki-reader__back"
            title="Back"
            @click="goBackWiki"
          >
            <span class="material-symbols-outlined text-[18px]">arrow_back</span>
          </button>
          <span class="wiki-reader__title">Wiki</span>
          <button class="wiki-reader__close" title="Close" @click="closeWikiPanel">
            <span class="material-symbols-outlined text-[18px]">close</span>
          </button>
        </div>
        <div class="wiki-reader__body">
          <WikiPageViewer
            :slug="wikiSlug"
            @navigate="navigateWiki"
            @view-article="openArticleDetail"
            @close="closeWikiPanel"
          />
        </div>
      </div>
    </Transition>

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

/* Wiki mode toggle button (right of the (+) icon). Halo + indigo fill when active. */
.wiki-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 9999px;
  border: 1px solid rgb(203 213 225); /* slate-300 */
  background-color: #fff;
  color: rgb(99 102 241); /* indigo-600 */
  flex-shrink: 0;
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s,
    box-shadow 0.15s,
    border-color 0.15s;
}

.wiki-toggle:hover:not(.wiki-toggle--active) {
  background-color: rgb(238 242 255); /* indigo-50 */
  border-color: rgb(165 180 252); /* indigo-300 */
}

.wiki-toggle--active {
  background-color: rgb(99 102 241); /* indigo-600 */
  border-color: rgb(79 70 229); /* indigo-700 */
  color: #fff;
  /* Halo */
  box-shadow:
    0 0 0 3px rgb(199 210 254 / 0.9),
    /* indigo-200 ring */ 0 1px 2px rgb(15 23 42 / 0.08);
}

/* Small "wiki" badge on message timestamps. */
.wiki-badge {
  display: inline-flex;
  align-items: center;
  padding: 0.0625rem 0.375rem;
  border-radius: 9999px;
  background-color: rgb(224 231 255); /* indigo-100 */
  color: rgb(67 56 202); /* indigo-800 */
  font-size: 0.55rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

/* Wiki-mode banner (replaces the article context picker). */
.wiki-banner {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.5rem 0.75rem;
  border-radius: 0.5rem;
  background-color: rgb(238 242 255); /* indigo-50 */
  border: 1px solid rgb(199 210 254); /* indigo-200 */
  color: rgb(55 48 163); /* indigo-900 */
}

/* Wiki reader slide-over panel. */
.wiki-reader {
  position: fixed;
  top: 0;
  right: 0;
  height: 100vh;
  width: 100%;
  max-width: 640px;
  background: #fff;
  z-index: 50;
  display: flex;
  flex-direction: column;
  border-left: 1px solid rgb(226 232 240);
  box-shadow: -4px 0 24px rgb(0 0 0 / 12%);
}

.wiki-reader__chrome {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  border-bottom: 1px solid rgb(226 232 240);
  background: #fff;
  flex-shrink: 0;
}

.wiki-reader__title {
  font-size: 0.8rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: rgb(71 85 105);
  flex: 1;
}

.wiki-reader__back,
.wiki-reader__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2rem;
  height: 2rem;
  border-radius: 0.375rem;
  border: none;
  background: transparent;
  color: rgb(100 116 139);
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.wiki-reader__back:hover,
.wiki-reader__close:hover {
  background-color: rgb(241 245 249);
  color: rgb(15 23 42);
}

.wiki-reader__body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
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

/* Wiki link + article reference styling inside assistant bubbles.
   Mirrors wiki-page-viewer.vue so clicks feel consistent. */
.markdown-content :deep(.wikilink) {
  color: rgb(79 70 229);
  text-decoration: underline;
  cursor: pointer;
  text-decoration-style: dotted;
}
.markdown-content :deep(.wikilink:hover) {
  text-decoration-style: solid;
}
.markdown-content :deep(.art-ref) {
  display: inline;
  color: rgb(21 128 61);
  background: rgb(240 253 244);
  border: 1px solid rgb(220 252 231);
  padding: 0 0.3rem;
  border-radius: 0.25rem;
  font-size: 0.75rem;
  cursor: pointer;
  text-decoration: none;
  font-weight: 500;
}
.markdown-content :deep(.art-ref:hover) {
  background: rgb(220 252 231);
}
.markdown-content :deep(.art-ref--missing) {
  color: rgb(148 163 184);
  background: rgb(241 245 249);
  border-color: rgb(226 232 240);
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
/* Slide transition for side panel */
.slide-enter-active,
.slide-leave-active {
  transition:
    transform 0.25s cubic-bezier(0.16, 1, 0.3, 1),
    opacity 0.25s ease;
}
.slide-enter-from,
.slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
</style>
