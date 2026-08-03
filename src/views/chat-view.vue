<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, nextTick, watch } from 'vue';
import { useRouter } from 'vue-router';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useChatStore } from '@/stores/chat';
import { useToast } from '@/composables/use-toast';
import type { Article } from '@/types';
import type { WikiStatus } from '@/types/wiki';
import type { CitationResult, CitationStyle } from '@/types/citation-finder';
import { marked } from 'marked';
import { renderWikiMarkdown } from '@/utils/wiki-markdown';
import { useArticleSearch } from '@/composables/use-article-search';
import { useScreening } from '@/composables/use-screening';
import { useWiki } from '@/composables/use-wiki';
import { useFullTextAttachment } from '@/composables/use-full-text-attachment';
import { useArticleDelete } from '@/composables/use-article-delete';
import { useClearAiReasoning } from '@/composables/use-clear-ai-reasoning';
import { useLlmConfigured } from '@/composables/use-llm-configured';
import { useLlmConfigStore } from '@/stores/llm-config';
import { getReadiness, stopCitationListeners } from '@/composables/use-citation-finder';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import WikiPageViewer from '@/components/wiki/wiki-page-viewer.vue';
import CitationResultCard from '@/components/citation-result-card.vue';
import type { WikiSourceInfo } from '@/types/wiki';
import type { CitationFinderMode } from '@/types/citation-finder';

const router = useRouter();
const toast = useToast();
const chatStore = useChatStore();
const llmConfigStore = useLlmConfigStore();

/**
 * Reactive "is the LLM configured?" gate from the canonical composable.
 * Replaces the former local `isLlmConfigured` ref that was populated by a
 * one-shot `has_llm_config` IPC call in `onMounted` and went stale on
 * Settings edits. Now any Settings change (e.g. clearing the API key)
 * instantly re-evaluates this gate and the empty-state card below.
 */
const isLlmConfigured = useLlmConfigured();
/**
 * True while the LLM config store is loading for the very first time so the
 * "Checking LLM configuration..." spinner shows instead of flashing the
 * unconfigured card before bootstrap resolves the store. Reactive over the
 * store's `initialized` flag.
 */
const checkingLlm = computed(() => !llmConfigStore.initialized);
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

/** Wiki page slug-to-title map. Loaded once when the wiki is ready so that
 *  bare UUIDs in wiki-sourced chat bubbles can render as synthesis-styled
 *  chips with human-readable titles instead of raw UUIDs. */
const wikiPageTitles = ref<Map<string, string>>(new Map());
const { listPages: wikiListPages, checkForUpdates: wikiCheckForUpdates } = useWiki();

/**
 * Derived source-metadata map (article id -> WikiSourceInfo) built reactively
 * from the loaded `articles` list. Passed to `renderWikiMarkdown` so bare
 * article UUIDs in wiki-sourced chat prose render as green `.art-ref` chips
 * that open the article detail panel, instead of pink wiki chips.
 */
const wikiSources = computed(() => {
  const map = new Map<string, WikiSourceInfo>();
  for (const a of articles.value) {
    map.set(a.id, {
      id: a.id,
      title: a.title,
      authors: a.authors ?? [],
      year: a.publicationYear ?? null,
      doi: a.doi ?? null,
      abstractText: a.abstractText ?? '',
      journal: a.journal ?? null,
    });
  }
  return map;
});

const {
  selectedArticle: detailArticle,
  auditTrail: detailAuditTrail,
  selectArticle,
  refreshArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  updateMetadata,
  moveArticle,
  deleteArticle,
  clearAiReasoning,
  attachFullText,
  deleteFullTextAttachment,
} = useArticleSearch();
const { screenArticle } = useScreening();

// Article delete UI orchestration is centralized in `useArticleDelete`
// (shared with the other detail-panel host views), mirroring
// `useFullTextAttachment`. The composable nulls `selectedArticle` (aliased as
// `detailArticle`), which reactively hides the panel via `v-if="detailArticle"`;
// the `onDeleted` hook resets the fullscreen flag.
const { handleDeleteArticle } = useArticleDelete({
  deleteArticle,
  onDeleted: () => {
    isDetailFullScreen.value = false;
  },
});

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

/** Citation-finder prose textarea (replaces the single-line `<input>` used by
 *  chat/wiki modes). Multi-line; submits on Ctrl/Cmd+Enter. */
const citationProse = ref('');

/** Citation-finder status-filter checkboxes state. Working + Included default
 *  ON, Rejected default OFF, Duplicate always excluded (hidden). */
const citationStatuses = ref({
  working: true,
  included: true,
  rejected: false,
});

/** Computed status filter array passed to the backend. Mirrors the checkbox
 *  state; duplicates are never included. */
const citationStatusFilter = computed(() => {
  const out: string[] = [];
  if (citationStatuses.value.working) out.push('working');
  if (citationStatuses.value.included) out.push('included');
  if (citationStatuses.value.rejected) out.push('rejected');
  return out;
});

/** Whether the citation-finder input area is shown (source === 'citation-
 *  finder'). Drives the v-if that swaps the article-context pills for the
 *  citation toolbar + prose textarea. */
const isCitationMode = computed(() => chatStore.source === 'citation-finder');

/** Citation-finder readiness check. Populates `chatStore.citationFinderReady`
 *  (drives the 3rd toggle visibility). Runs on mount + when the view is
 *  re-activated after navigation. */
async function checkCitationFinderReadiness() {
  try {
    const r = await getReadiness(citationStatusFilter.value);
    chatStore.setCitationFinderReady(r.providerSupportsEmbeddings);
  } catch {
    // Provider not configured / embeddings disabled → hide the toggle.
    chatStore.setCitationFinderReady(false);
  }
}

/** Citation-style <select> options (the shared 5-style list). */
const citationStyleOptions: CitationStyle[] = ['APA', 'MLA', 'Chicago', 'IEEE', 'AMA'];

/** Mode toggle handler (segmented button). */
function onSetCitationMode(mode: CitationFinderMode) {
  chatStore.setCitationFinderMode(mode);
}

/** Flip the citation-finder source on. Mutually exclusive with wiki (entering
 *  citation mode drops back from wiki if it was on). */
function onToggleCitationFinder() {
  if (chatStore.source === 'citation-finder') {
    chatStore.setSource('articles');
  } else {
    chatStore.setSource('citation-finder');
  }
}

/** Submit the citation search. Driven by the prose textarea + the Find
 *  Citations button + Ctrl/Cmd+Enter. Threads the live status-filter
 *  checkboxes (NEW-4) to the store's dedicated `sendCitationSearch`, which
 *  forwards them to the backend. The backend filters against the whitelist
 *  and applies NO default - an empty array (all checkboxes unchecked)
 *  returns the "No articles match the selected filters." empty result. */
async function handleCitationSend() {
  // The Find button is `:disabled` when the prose is empty, but Ctrl/Cmd+Enter
  // bypasses the disabled button, so we must guard here + show the toast
  // (NEW-7: the old second guard was unreachable because the first returned).
  if (!citationProse.value.trim()) {
    toast.show('Please paste text to search.', 'info');
    return;
  }
  if (chatStore.loading) return;
  const text = citationProse.value;
  citationProse.value = '';
  // Pass the live checkbox state; the store owns the message list + the
  // citation-finder branch. The backend's `filter_valid_statuses` whitelists
  // `['working','included','rejected']` and drops everything else.
  await chatStore.sendCitationSearch(text, citationStatusFilter.value);
  scrollToBottom();
}

/** Copy a citation string to the clipboard + toast. */
async function handleCopyCitation(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.show('Citation copied to clipboard.', 'success');
  } catch {
    toast.show('Failed to copy citation.', 'error');
  }
}

/** Flatten a `CitationResult[]` into a single card list for IEEE `[N]`
 *  numbering across the whole bubble (per-bubble numbering). Returns the
 *  matches + their 1-based index in display order. */
function flattenForIeee(results: CitationResult[]): Array<{
  match: CitationResult['matches'][number];
  ieeeIndex: number;
  claim: string | null;
}> {
  const out: Array<{
    match: CitationResult['matches'][number];
    ieeeIndex: number;
    claim: string | null;
  }> = [];
  let idx = 1;
  for (const group of results) {
    for (const match of group.matches) {
      out.push({ match, ieeeIndex: idx, claim: group.claim });
      idx += 1;
    }
  }
  return out;
}

// ── Per-statement claim-group collapse state ────────────────────────────
//
// Each claim heading is a caret toggle that collapses/expands its cards.
// Default: expanded (results visible on first paint; the user collapses the
// claims they want to tuck away). Keyed by `${msgIdx}::${claim}` so the same
// claim text in different bubbles (re-searches) stays independent, and the
// state survives as long as the message list is append-only (which it is -
// messages are never reordered, only appended).
const collapsedClaims = ref<Set<string>>(new Set());

/** Build the per-bubble key for a claim's collapse state. */
function claimKey(msgIdx: number, claim: string): string {
  return `${msgIdx}::${claim}`;
}

/** Whether a given claim's cards are currently collapsed. */
function isClaimCollapsed(msgIdx: number, claim: string): boolean {
  return collapsedClaims.value.has(claimKey(msgIdx, claim));
}

/** Toggle a claim's collapse state (add/remove from the Set). Mutating a
 *  `Set` in place doesn't trigger reactivity, so we reassign the ref to a
 *  fresh `Set` constructed from the updated contents. */
function toggleClaimCollapsed(msgIdx: number, claim: string): void {
  const key = claimKey(msgIdx, claim);
  const next = new Set(collapsedClaims.value);
  if (next.has(key)) {
    next.delete(key);
  } else {
    next.add(key);
  }
  collapsedClaims.value = next;
}

/** Count the cards under a given claim (for the count badge). Reuses the same
 *  filter predicate the template uses so the number always matches what would
 *  render when expanded. */
function claimCardCount(results: CitationResult[], claim: string): number {
  return flattenForIeee(results).filter((c) => c.claim === claim).length;
}

onMounted(async () => {
  // The LLM-configured gate is reactive (no IPC probe needed). Still kick
  // off the wiki status + citation readiness loads so the toggle visibility
  // is correct on first paint; the LLM gate itself is read reactively.
  await Promise.all([loadArticles(), checkWikiStatus(), checkCitationFinderReadiness()]);
  scrollToBottom();
});

// Tear down citation:* listeners on unmount so navigating away from Chat
// does not leave dangling event subscriptions.
onUnmounted(() => {
  stopCitationListeners();
});

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
    // Load page titles so wiki chat bubbles can render bare UUIDs as
    // synthesis-styled chips with human-readable titles.
    if (chatStore.wikiReady && wikiPageTitles.value.size === 0) {
      try {
        const pages = await wikiListPages();
        const map = new Map<string, string>();
        for (const p of pages) {
          map.set(p.slug, p.title);
        }
        wikiPageTitles.value = map;
      } catch {
        // Non-fatal: bare UUIDs fall back to raw text.
      }
    }
  } catch {
    chatStore.setWikiReady(false);
  } finally {
    checkingWiki.value = false;
  }

  // When the wiki is ready, proactively run the on-demand drift check so
  // wiki-mode chat answers reflect any external edits made since the last
  // visit. Runs lock-free on the backend; debounced 30s via useWiki.
  if (chatStore.wikiReady) {
    try {
      const result = await wikiCheckForUpdates(false);
      if (result?.rebuilt) {
        toast.show(`Wiki updated: ${result.pagesReindexed} pages re-indexed.`, 'success');
      }
    } catch {
      // Non-fatal: wiki chat still works with the existing index.
    }
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
    return renderWikiMarkdown(msg.content, {
      sources: wikiSources.value,
      pageTitles: wikiPageTitles.value,
      // Chat view: articles win over wiki pages for bare UUID resolution, so
      // an article UUID renders as a green art-ref (article detail) even when
      // a synthesis wiki page exists for the same UUID.
      articlePriority: true,
    });
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

// Full-text attach UI orchestration is centralized in
// `useFullTextAttachment` (shared with the other detail-panel host views).
const { handleAttachFullText } = useFullTextAttachment({ attachFullText });

// AI-reasoning clear UI orchestration is centralized in `useClearAiReasoning`
// (shared with the other detail-panel host views). The composable owns the
// toast; `useArticleSearch.clearAiReasoning` owns the IPC + article refresh.
const { handleClearAiReasoning } = useClearAiReasoning({ clearAiReasoning });
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
          <!-- Welcome state: three-column overview of the three chat modes -->
          <div v-if="chatStore.messages.length === 0" class="my-auto py-8 w-full max-w-5xl mx-auto">
            <div class="chat-welcome-grid">
              <!-- Academic Research Chat -->
              <div class="chat-welcome-card">
                <div class="chat-welcome-card__icon chat-welcome-card__icon--indigo">
                  <span class="material-symbols-outlined">chat_add_on</span>
                </div>
                <h3 class="chat-welcome-card__title">Academic Research Chat</h3>
                <p class="chat-welcome-card__desc">
                  Ask questions about the articles in your library. Add articles to the context
                  using the <strong>(+)</strong> button to ground the responses in specific research
                  text.
                </p>
                <p class="chat-welcome-card__hint">
                  <span class="material-symbols-outlined">add_circle</span>
                  Click <strong>(+)</strong> to select articles, then type your question.
                </p>
              </div>

              <!-- Wiki Chat -->
              <div class="chat-welcome-card">
                <div class="chat-welcome-card__icon chat-welcome-card__icon--purple">
                  <span class="material-symbols-outlined">local_library</span>
                </div>
                <h3 class="chat-welcome-card__title">Wiki Chat</h3>
                <p class="chat-welcome-card__desc">
                  Ask questions answered from your synthesized knowledge base. The Wiki is built
                  from your included articles and retrieves the most relevant pages for each
                  question.
                </p>
                <p v-if="chatStore.wikiReady" class="chat-welcome-card__hint">
                  <span class="material-symbols-outlined">local_library</span>
                  Toggle the <strong>Wiki</strong> icon (right of <strong>(+)</strong>) to start.
                </p>
                <p v-else class="chat-welcome-card__hint chat-welcome-card__hint--muted">
                  <span class="material-symbols-outlined">lock</span>
                  Initialize the Wiki first (see the Wiki screen).
                </p>
              </div>

              <!-- Citation Finder -->
              <div class="chat-welcome-card">
                <div class="chat-welcome-card__icon chat-welcome-card__icon--teal">
                  <span class="material-symbols-outlined">quick_reference_all</span>
                </div>
                <h3 class="chat-welcome-card__title">Citation Finder</h3>
                <p class="chat-welcome-card__desc">
                  Paste text you are writing and get matching citations from your library. Bango
                  finds the relevant passages first, so the AI cannot invent sources: every result
                  is grounded in your real articles.
                </p>
                <p v-if="chatStore.citationFinderReady" class="chat-welcome-card__hint">
                  <span class="material-symbols-outlined">quick_reference_all</span>
                  Click the <strong>Citation Finder</strong> icon to start.
                </p>
                <p v-else class="chat-welcome-card__hint chat-welcome-card__hint--muted">
                  <span class="material-symbols-outlined">lock</span>
                  Requires an embedding-capable LLM provider (see Settings).
                </p>
              </div>
            </div>
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
                <span
                  v-else-if="msg.source === 'citation-finder'"
                  class="citation-badge"
                  title="Citation Finder result"
                  >citation</span
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
                <!-- Citation Finder results: render the card stack instead of
                     the Markdown body. Per-bubble style frozen at submit time;
                     IEEE [N] numbering is the flattened card order across the
                     whole bubble (per-statement groups render claim headings). -->
                <template v-else-if="msg.citations">
                  <div class="citation-bubble">
                    <p v-if="msg.content" class="citation-bubble__summary">{{ msg.content }}</p>
                    <template v-if="msg.citations.some((g) => g.claim !== null)">
                      <!-- Per-statement: group cards under claim headings.
                           The predicate is "any group carries a non-null
                           claim", NOT `length > 1`: per-statement mode that
                           produces exactly 1 claim still has `claim: Some`,
                           and the claim heading must render (whole-block
                           always has `claim: null`). -->
                      <div
                        v-for="group in msg.citations"
                        :key="group.claim ?? 'whole'"
                        class="citation-bubble__group"
                      >
                        <button
                          v-if="group.claim"
                          type="button"
                          class="citation-bubble__claim-toggle"
                          :aria-expanded="!isClaimCollapsed(idx, group.claim)"
                          :title="
                            isClaimCollapsed(idx, group.claim)
                              ? 'Expand citations for this statement'
                              : 'Collapse citations for this statement'
                          "
                          @click="toggleClaimCollapsed(idx, group.claim)"
                        >
                          <span
                            class="citation-bubble__claim-count"
                            :title="
                              claimCardCount(msg.citations, group.claim) +
                              ' citation' +
                              (claimCardCount(msg.citations, group.claim) === 1 ? '' : 's')
                            "
                            >{{ claimCardCount(msg.citations, group.claim) }}</span
                          >
                          <span class="citation-bubble__claim-text">{{ group.claim }}</span>
                          <span
                            class="material-symbols-outlined citation-bubble__claim-caret"
                            :class="{
                              'citation-bubble__claim-caret--collapsed': isClaimCollapsed(
                                idx,
                                group.claim
                              ),
                            }"
                            >expand_more</span
                          >
                        </button>
                        <CitationResultCard
                          v-for="card in flattenForIeee(msg.citations).filter(
                            (c) => c.claim === group.claim
                          )"
                          v-show="group.claim ? !isClaimCollapsed(idx, group.claim) : true"
                          :key="card.match.articleId + '-' + card.ieeeIndex"
                          :match="card.match"
                          :style="msg.citationStyle ?? 'APA'"
                          :ieee-index="card.ieeeIndex"
                          @copy="handleCopyCitation"
                          @view="openArticleDetail"
                        />
                      </div>
                    </template>
                    <template v-else>
                      <!-- Whole-block: flat card list (every group has
                           `claim: null`). -->
                      <CitationResultCard
                        v-for="card in flattenForIeee(msg.citations)"
                        :key="card.match.articleId + '-' + card.ieeeIndex"
                        :match="card.match"
                        :style="msg.citationStyle ?? 'APA'"
                        :ieee-index="card.ieeeIndex"
                        @copy="handleCopyCitation"
                        @view="openArticleDetail"
                      />
                    </template>
                  </div>
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

          <!-- Loading / Thinking indicator.
               HIDDEN in citation-finder mode: the citation-progress bar above
               the input area already communicates Phase B/C status (with a
               Cancel button + per-phase message), so the generic "Analyzing
               article context..." text would be stale, misleading, and
               redundant. Wiki + article modes keep the thinking dots. -->
          <div
            v-if="chatStore.loading && !isCitationMode"
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

          <!-- Citation Finder input area (replaces article-context pills +
               single-line input). Holds the style <select>, mode toggle,
               status checkboxes, prose textarea, Find Citations button, and
               the live progress + Cancel UI. -->
          <div v-else-if="isCitationMode" class="citation-input-area">
            <!-- Single control row: Citation Style dropdown → status
                 checkboxes → Mode segmented toggle → close (X). Everything is
                 at the same level so there's no extra whitespace; the close
                 button is pushed to the right edge with margin-left:auto. -->
            <div class="citation-input-area__row">
              <label class="citation-input-area__field">
                <span class="citation-input-area__label">Citation Style</span>
                <select
                  :value="chatStore.citationStyle"
                  class="citation-input-area__select"
                  @change="
                    chatStore.setCitationStyle(
                      ($event.target as HTMLSelectElement).value as CitationStyle
                    )
                  "
                >
                  <option v-for="s in citationStyleOptions" :key="s" :value="s">{{ s }}</option>
                </select>
              </label>

              <!-- Status checkboxes with a "ARTICLES TO SEARCH" header matching
                   the Citation Style header. Duplicate is always excluded. -->
              <div class="citation-input-area__field" role="group" aria-label="Status filter">
                <span class="citation-input-area__label">Articles to Search</span>
                <div class="citation-input-area__statuses">
                  <label class="citation-input-area__checkbox">
                    <input v-model="citationStatuses.working" type="checkbox" />
                    <span>Working</span>
                  </label>
                  <label class="citation-input-area__checkbox">
                    <input v-model="citationStatuses.included" type="checkbox" />
                    <span>Included</span>
                  </label>
                  <label class="citation-input-area__checkbox">
                    <input v-model="citationStatuses.rejected" type="checkbox" />
                    <span>Rejected</span>
                  </label>
                </div>
                <span class="citation-input-area__statuses-hint">Duplicates always excluded</span>
              </div>

              <!-- Mode toggle with a "SCOPE" header matching Citation Style. -->
              <div
                class="citation-input-area__field"
                role="group"
                aria-label="Citation Finder mode"
              >
                <span class="citation-input-area__label">Scope</span>
                <div class="citation-input-area__mode">
                  <button
                    type="button"
                    class="citation-input-area__mode-btn"
                    :class="{
                      'citation-input-area__mode-btn--active':
                        chatStore.citationFinderMode === 'whole_block',
                    }"
                    @click="onSetCitationMode('whole_block')"
                  >
                    Whole Block
                  </button>
                  <button
                    type="button"
                    class="citation-input-area__mode-btn"
                    :class="{
                      'citation-input-area__mode-btn--active':
                        chatStore.citationFinderMode === 'per_statement',
                    }"
                    @click="onSetCitationMode('per_statement')"
                  >
                    Per Statement
                  </button>
                </div>
              </div>
              <button
                type="button"
                class="citation-input-area__close"
                title="Close Citation Finder"
                @click="onToggleCitationFinder"
              >
                <span class="material-symbols-outlined text-[18px]">close</span>
              </button>
            </div>

            <!-- Row 3: Prose textarea + (Find Citations button OR live
                 progress). While a search is running, the progress indicator
                 replaces the Find Citations button in place - the textarea
                 stays visible so the user can draft the next search. -->
            <div class="citation-input-area__prose-row">
              <textarea
                v-model="citationProse"
                class="citation-input-area__textarea"
                placeholder="Paste the text you want to find citations for..."
                rows="4"
                @keydown.enter.ctrl="handleCitationSend"
                @keydown.enter.meta="handleCitationSend"
              ></textarea>

              <!-- Idle: Find Citations button -->
              <button
                v-if="!chatStore.citationProgress"
                type="button"
                class="citation-input-area__find-btn"
                :disabled="!citationProse.trim() || chatStore.loading"
                @click="handleCitationSend"
              >
                <span class="material-symbols-outlined text-[18px]">search</span>
                Find Citations
              </button>

              <!-- Running: compact progress replaces the button -->
              <div v-else class="citation-progress citation-progress--inline">
                <div class="citation-progress__header">
                  <span class="citation-progress__message">{{
                    chatStore.citationProgress.message
                  }}</span>
                  <button
                    type="button"
                    class="citation-progress__cancel"
                    :disabled="chatStore.cancelling"
                    @click="chatStore.cancelCitationSearch()"
                  >
                    <span
                      v-if="chatStore.cancelling"
                      class="citation-progress__cancel-spinner"
                    ></span>
                    <span v-else class="material-symbols-outlined text-[14px]">cancel</span>
                    {{ chatStore.cancelling ? 'Cancelling…' : 'Cancel' }}
                  </button>
                </div>
                <div class="citation-progress__bar-track">
                  <div
                    class="citation-progress__bar-fill"
                    :style="{
                      width:
                        (chatStore.citationProgress.phase === 'preparing_embeddings'
                          ? chatStore.citationProgress.overallPercent
                          : 100) + '%',
                    }"
                    :class="{
                      'citation-progress__bar-fill--indeterminate':
                        chatStore.citationProgress.phase === 'searching',
                    }"
                  ></div>
                </div>
              </div>
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

          <!-- Chat bar input container.
               HIDDEN in citation-finder mode: the citation input area above
               owns the active input (prose textarea + Find/progress), and the
               mode toggles here are redundant (the citation area has its own
               close button + the toggle is not how the user exits). Wiki +
               article modes keep the full chat bar. -->
          <div v-if="!isCitationMode" class="flex items-center gap-3">
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

            <!-- Citation Finder toggle button (3rd toggle). Visible only when
                 the provider supports embeddings. Same chrome as the wiki
                 toggle (.citation-toggle mirrors .wiki-toggle). -->
            <button
              v-if="chatStore.citationFinderReady"
              class="citation-toggle"
              :class="{ 'citation-toggle--active': isCitationMode }"
              :title="
                isCitationMode
                  ? 'Citation Finder active. Click to return to article context.'
                  : 'Find citations for text you are writing (semantic search over your library)'
              "
              :aria-pressed="isCitationMode"
              @click="onToggleCitationFinder"
            >
              <span class="material-symbols-outlined text-[24px]">quick_reference_all</span>
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
        @delete-article="handleDeleteArticle"
        @clear-ai-reasoning="handleClearAiReasoning"
        @toggle-full-screen="isDetailFullScreen = !isDetailFullScreen"
        @update-notes="updateNotes"
        @update-tags="updateTags"
        @update-labels="updateLabels"
        @update-criteria="updateCriteria"
        @update-metadata="updateMetadata"
        @screen-article="screenArticle"
        @move-article="moveArticle"
        @attach-full-text="handleAttachFullText"
        @delete-full-text="deleteFullTextAttachment"
        @refresh-article="refreshArticle"
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

/* Three-column welcome grid (Academic Chat / Wiki Chat / Citation Finder).
   Responsive: 3 columns on md+, single column on small screens. */
.chat-welcome-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 1rem;
}

@media (min-width: 768px) {
  .chat-welcome-grid {
    grid-template-columns: repeat(3, 1fr);
  }
}

.chat-welcome-card {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  background: #fff;
  border: 1px solid rgb(226 232 240); /* slate-200 */
  border-radius: 0.75rem;
  padding: 1.25rem;
  box-shadow: 0 1px 2px rgb(15 23 42 / 0.04);
}

.chat-welcome-card__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.75rem;
  height: 2.75rem;
  border-radius: 9999px;
  margin-bottom: 0.25rem;
}

.chat-welcome-card__icon span.material-symbols-outlined {
  font-size: 26px;
}

.chat-welcome-card__icon--indigo {
  background: rgb(238 242 255); /* indigo-50 */
  color: rgb(79 70 229); /* indigo-600 */
}

.chat-welcome-card__icon--purple {
  background: rgb(250 232 255); /* purple-50 */
  color: rgb(126 34 206); /* purple-700 */
}

.chat-welcome-card__icon--teal {
  background: rgb(204 251 241); /* teal-100 */
  color: rgb(15 118 110); /* teal-700 */
}

.chat-welcome-card__title {
  font-size: 0.95rem;
  font-weight: 700;
  color: rgb(15 23 42); /* slate-900 */
  margin: 0;
}

.chat-welcome-card__desc {
  font-size: 0.8rem;
  line-height: 1.5;
  color: rgb(71 85 105); /* slate-600 */
  margin: 0;
}

.chat-welcome-card__hint {
  display: flex;
  align-items: flex-start;
  gap: 0.375rem;
  margin-top: auto;
  padding-top: 0.5rem;
  font-size: 0.72rem;
  color: rgb(99 102 241); /* indigo-600 */
  font-weight: 600;
}

.chat-welcome-card__hint span.material-symbols-outlined {
  font-size: 15px;
  flex-shrink: 0;
  margin-top: 1px;
}

.chat-welcome-card__hint--muted {
  color: rgb(148 163 184); /* slate-400 */
  font-weight: 500;
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

/* Small "citation" badge on citation-finder assistant bubble timestamps.
   Mirrors .wiki-badge but in teal so the two sources are visually distinct. */
.citation-badge {
  display: inline-flex;
  align-items: center;
  padding: 0.0625rem 0.375rem;
  border-radius: 9999px;
  background-color: rgb(204 251 241); /* teal-100 */
  color: rgb(15 118 110); /* teal-800 */
  font-size: 0.55rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

/* Citation Finder toggle button (3rd toggle, mirrors .wiki-toggle). */
.citation-toggle {
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

.citation-toggle:hover:not(.citation-toggle--active) {
  background-color: rgb(238 242 255); /* indigo-50 */
  border-color: rgb(165 180 252); /* indigo-300 */
}

.citation-toggle--active {
  background-color: rgb(99 102 241); /* indigo-600 */
  border-color: rgb(79 70 229); /* indigo-700 */
  color: #fff;
  box-shadow:
    0 0 0 3px rgb(199 210 254 / 0.9),
    0 1px 2px rgb(15 23 42 / 0.08);
}

/* Citation Finder input area (replaces article-context pills + single-line
 * input when isCitationMode is true). */
.citation-input-area {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin-bottom: 0.75rem;
}

/* Close (X) button - exits citation mode. Sits at the same level as the
   Citation Style dropdown (inside Row 1) and is pushed to the right edge with
   margin-left:auto so it introduces no extra top whitespace. Mirrors the
   wiki-reader close button's muted slate styling. Aligned to center so it
   lines up with the row's other controls regardless of label height. */
.citation-input-area__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.75rem;
  height: 1.75rem;
  margin-left: auto;
  align-self: center;
  border-radius: 0.375rem;
  border: none;
  background: transparent;
  color: rgb(100 116 139); /* slate-500 */
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.citation-input-area__close:hover {
  background-color: rgb(241 245 249); /* slate-100 */
  color: rgb(15 23 42); /* slate-900 */
}

/* Inline variant of the progress block: constrains the width so it occupies
   the Find Citations button's column (instead of spanning the full row). */
.citation-progress--inline {
  flex-shrink: 0;
  min-width: 9rem;
  max-width: 12rem;
  justify-content: center;
}

.citation-input-area__row {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.citation-input-area__field {
  display: flex;
  flex-direction: column;
  gap: 0.1875rem;
}

.citation-input-area__label {
  font-size: 0.625rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: rgb(100 116 139); /* slate-500 */
}

.citation-input-area__select {
  padding: 0.3125rem 0.5rem;
  border: 1px solid rgb(203 213 225); /* slate-300 */
  border-radius: 0.375rem;
  background: #fff;
  font-size: 0.75rem;
  color: rgb(15 23 42); /* slate-900 */
  cursor: pointer;
}

.citation-input-area__mode {
  display: inline-flex;
  border: 1px solid rgb(203 213 225);
  border-radius: 0.375rem;
  overflow: hidden;
}

.citation-input-area__mode-btn {
  padding: 0.3125rem 0.625rem;
  border: none;
  background: #fff;
  font-size: 0.6875rem;
  font-weight: 600;
  color: rgb(71 85 105); /* slate-600 */
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.citation-input-area__mode-btn:not(.citation-input-area__mode-btn--active):hover {
  background: rgb(241 245 249); /* slate-100 */
}

.citation-input-area__mode-btn--active {
  background: rgb(99 102 241); /* indigo-600 */
  color: #fff;
}

.citation-input-area__statuses {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.625rem;
}

.citation-input-area__checkbox {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 0.6875rem;
  color: rgb(71 85 105); /* slate-600 */
  cursor: pointer;
}

.citation-input-area__checkbox input {
  accent-color: rgb(99 102 241); /* indigo-600 */
}

.citation-input-area__statuses-hint {
  font-size: 0.625rem;
  color: rgb(148 163 184); /* slate-400 */
  font-style: italic;
}

.citation-input-area__prose-row {
  display: flex;
  gap: 0.5rem;
  align-items: stretch;
}

.citation-input-area__textarea {
  flex: 1;
  padding: 0.5rem 0.625rem;
  border: 1px solid rgb(203 213 225); /* slate-300 */
  border-radius: 0.5rem;
  font-size: 0.8rem;
  line-height: 1.4;
  color: rgb(15 23 42);
  resize: vertical;
  min-height: 4.5rem;
  font-family: inherit;
}

.citation-input-area__textarea:focus {
  outline: none;
  border-color: rgb(99 102 241); /* indigo-600 */
  box-shadow: 0 0 0 2px rgb(99 102 241 / 0.2);
}

.citation-input-area__find-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.5rem 0.875rem;
  border: none;
  border-radius: 0.5rem;
  background: rgb(99 102 241); /* indigo-600 */
  color: #fff;
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.15s;
  flex-shrink: 0;
}

.citation-input-area__find-btn:hover:not(:disabled) {
  background: rgb(79 70 229); /* indigo-700 */
}

.citation-input-area__find-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* Live progress bar + Cancel button. */
.citation-progress {
  display: flex;
  flex-direction: column;
  gap: 0.3125rem;
  padding: 0.5rem 0.625rem;
  background: rgb(248 250 252); /* slate-50 */
  border: 1px solid rgb(226 232 240); /* slate-200 */
  border-radius: 0.375rem;
}

.citation-progress__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.citation-progress__message {
  font-size: 0.6875rem;
  font-weight: 500;
  color: rgb(71 85 105); /* slate-600 */
}

.citation-progress__cancel {
  display: inline-flex;
  align-items: center;
  gap: 0.1875rem;
  padding: 0.1875rem 0.4375rem;
  border: 1px solid rgb(254 202 202); /* red-200 */
  border-radius: 0.25rem;
  background: #fff;
  color: rgb(220 38 38); /* red-600 */
  font-size: 0.625rem;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.15s;
}

.citation-progress__cancel:hover:not(:disabled) {
  background: rgb(254 226 226); /* red-100 */
}

.citation-progress__cancel:disabled {
  opacity: 0.6;
  cursor: default;
}

/* Small spinner shown next to "Cancelling…" while the backend drains the
 * in-flight LLM call + emits the terminal `citation:error`. */
.citation-progress__cancel-spinner {
  display: inline-block;
  width: 0.75rem;
  height: 0.75rem;
  border: 1.5px solid rgb(220 38 38 / 0.3); /* red-600 @ 30% */
  border-top-color: rgb(220 38 38); /* red-600 */
  border-radius: 9999px;
  animation: citation-cancel-spin 0.7s linear infinite;
}

@keyframes citation-cancel-spin {
  to {
    transform: rotate(360deg);
  }
}

.citation-progress__bar-track {
  width: 100%;
  height: 0.25rem;
  background: rgb(226 232 240); /* slate-200 */
  border-radius: 9999px;
  overflow: hidden;
}

.citation-progress__bar-fill {
  height: 100%;
  background: rgb(99 102 241); /* indigo-600 */
  border-radius: 9999px;
  transition: width 0.2s ease;
}

.citation-progress__bar-fill--indeterminate {
  animation: citation-progress-indeterminate 1.4s ease-in-out infinite;
}

@keyframes citation-progress-indeterminate {
  0% {
    transform: translateX(-100%);
  }
  50% {
    transform: translateX(0%);
  }
  100% {
    transform: translateX(100%);
  }
}

/* Citation results bubble: stacks CitationResultCard components. */
.citation-bubble {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  width: 100%;
  min-width: 0;
}

.citation-bubble__summary {
  font-size: 0.75rem;
  color: rgb(100 116 139); /* slate-500 */
  margin: 0 0 0.25rem 0;
}

.citation-bubble__group {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.citation-bubble__claim-heading {
  font-size: 0.7rem;
  font-weight: 700;
  color: rgb(67 56 202); /* indigo-800 */
  background: rgb(238 242 255); /* indigo-50 */
  padding: 0.1875rem 0.375rem;
  border-radius: 0.25rem;
  margin: 0.25rem 0 0 0;
}

/* Per-statement claim-group collapse toggle (replaces the static <h4>). */
.citation-bubble__claim-toggle {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  width: 100%;
  text-align: left;
  border: none;
  border-radius: 0.25rem;
  background: rgb(238 242 255); /* indigo-50 */
  padding: 0.25rem 0.5rem;
  cursor: pointer;
  transition: background-color 0.15s;
  font-family: inherit;
}

.citation-bubble__claim-toggle:hover {
  background: rgb(224 231 255); /* indigo-100 */
}

.citation-bubble__claim-count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 1.25rem;
  height: 1.25rem;
  padding: 0 0.3125rem;
  border-radius: 9999px;
  background: rgb(99 102 241); /* indigo-600 */
  color: #fff;
  font-size: 0.625rem;
  font-weight: 700;
  line-height: 1;
  flex-shrink: 0;
}

.citation-bubble__claim-text {
  flex: 1;
  min-width: 0;
  font-size: 0.7rem;
  font-weight: 700;
  color: rgb(67 56 202); /* indigo-800 */
  /* Long claims wrap; the toggle grows vertically. */
  word-break: break-word;
}

.citation-bubble__claim-caret {
  font-size: 16px;
  color: rgb(99 102 241); /* indigo-600 */
  transition: transform 0.15s ease;
  flex-shrink: 0;
}

/* Collapsed -> caret points right (rotated -90deg). Expanded -> points down. */
.citation-bubble__claim-caret--collapsed {
  transform: rotate(-90deg);
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

/* Synthesis-styled wikilink chip (from [^art-uuid]: definition lines). */
.markdown-content :deep(.wikilink--synthesis) {
  display: inline-block;
  background: rgb(168 85 247 / 0.12); /* purple-500 @ 12% */
  color: rgb(126 34 206); /* purple-800 */
  border: 1px solid rgb(168 85 247 / 0.3);
  padding: 0.0625rem 0.375rem;
  border-radius: 0.25rem;
  font-size: 0.8em;
  font-weight: 500;
  text-decoration: none;
  cursor: pointer;
}

.markdown-content :deep(.wikilink--synthesis:hover) {
  background: rgb(168 85 247 / 0.2);
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

/* T2.3 Phase 3: muted section-provenance badge rendered after a wikilink
 * when the citation carries a `(§Section)` suffix (e.g. `[[slug]] (§Methods)`).
 * Mirrors the wiki-page-viewer badge styling so chat + wiki stay consistent. */
.markdown-content :deep(.section-badge) {
  display: inline-block;
  margin-left: 0.25rem;
  padding: 0.0625rem 0.3125rem;
  font-size: 0.7em;
  font-weight: 500;
  color: rgb(100 116 139);
  background: rgb(241 245 249);
  border: 1px solid rgb(226 232 240);
  border-radius: 0.25rem;
  vertical-align: baseline;
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
