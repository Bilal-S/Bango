<script setup lang="ts">
/**
 * Chat view: the /chat route. A thin orchestration shell over per-concern
 * composables + components - the citation-finder orchestration lives in
 * `use-citation-finder-chat`, wiki mode in `use-chat-wiki`, article context
 * selection in `use-chat-article-context`, and transcript scrolling in
 * `use-chat-transcript`. The heavy markup lives in dedicated components
 * (welcome cards, message list, selected-articles bar, selector modal,
 * citation input area, the mismatch + local-embeddings dialogs).
 */
import { ref, onMounted, onUnmounted, computed, watch } from 'vue';
import { useRouter } from 'vue-router';
import { storeToRefs } from 'pinia';
import { useChatStore } from '@/stores/chat';
import { useToast } from '@/composables/use-toast';
import { useLlmConfigured } from '@/composables/use-llm-configured';
import { useLlmConfigStore } from '@/stores/llm-config';
import { stopCitationListeners } from '@/composables/use-citation-finder';
import { useLocalEmbeddings } from '@/composables/use-local-embeddings';
import { useArticleSearch } from '@/composables/use-article-search';
import { useScreening } from '@/composables/use-screening';
import { useFullTextAttachment } from '@/composables/use-full-text-attachment';
import { useArticleDelete } from '@/composables/use-article-delete';
import { useClearAiReasoning } from '@/composables/use-clear-ai-reasoning';
import { useChatArticleContext } from '@/composables/use-chat-article-context';
import { useChatWiki } from '@/composables/use-chat-wiki';
import { useChatTranscript } from '@/composables/use-chat-transcript';
import { useCitationFinderChat } from '@/composables/use-citation-finder-chat';
import type { CitationStatusFlags } from '@/types/citation-finder';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import WikiPageViewer from '@/components/wiki/wiki-page-viewer.vue';
import ChatWelcomeCards from '@/components/chat-welcome-cards.vue';
import ChatMessageList from '@/components/chat-message-list.vue';
import SelectedArticlesBar from '@/components/selected-articles-bar.vue';
import ArticleSelectorModal from '@/components/article-selector-modal.vue';
import CitationInputArea from '@/components/citation-input-area.vue';
import CitationMismatchDialog from '@/components/citation-mismatch-dialog.vue';
import CitationLocalEmbeddingsDialog from '@/components/citation-local-embeddings-dialog.vue';

const router = useRouter();
const toast = useToast();
const chatStore = useChatStore();
const llmConfigStore = useLlmConfigStore();

/** Reactive LLM-configured gate from the canonical composable (src/AGENTS.md). */
const isLlmConfigured = useLlmConfigured();
/** True while the LLM config store is loading for the first time. Prevents
 *  flashing the unconfigured card before bootstrap resolves. */
const checkingLlm = computed(() => !llmConfigStore.initialized);

const isDetailFullScreen = ref(false);

/* ── Article detail panel + wiki reader slide-overs ──────────────────────── */

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

/* Article delete orchestration centralized in `useArticleDelete`. Composable
 * nulls `detailArticle` (hides panel via `v-if`); `onDeleted` hook resets
 * the fullscreen flag. */
const { handleDeleteArticle } = useArticleDelete({
  deleteArticle,
  onDeleted: () => {
    isDetailFullScreen.value = false;
  },
});

// Full-text attach + AI-reasoning clear orchestration live in shared
// composables (used by the other detail-panel host views too).
const { handleAttachFullText } = useFullTextAttachment({ attachFullText });
const { handleClearAiReasoning } = useClearAiReasoning({ clearAiReasoning });

/* ── Per-concern composables ─────────────────────────────────────────────── */

const { selectedArticleIds, source: chatSource, wikiReady, messages } = storeToRefs(chatStore);

const { articles, showSelector, selectedArticles, loadArticles, toggleArticleSelection } =
  useChatArticleContext({
    selectedArticleIds,
    addSelectedArticle: chatStore.addSelectedArticle,
    removeSelectedArticle: chatStore.removeSelectedArticle,
  });

const {
  wikiPanelOpen,
  wikiNavStack,
  wikiSlug,
  wikiPageTitles,
  wikiSources,
  checkWikiStatus,
  onToggleWiki,
  openWikiPage,
  navigateWiki,
  goBackWiki,
  closeWikiPanel,
} = useChatWiki({
  source: chatSource,
  wikiReady,
  setWikiReady: chatStore.setWikiReady,
  setSource: chatStore.setSource,
  toggleWikiMode: chatStore.toggleWikiMode,
  articles,
  /* The reader and the article panel are mutually exclusive slide-overs. */
  onOpenWikiReader: () => {
    detailArticle.value = null;
  },
});

/** The scrollable transcript container (bound via the template ref). */
const chatScrollContainer = ref<HTMLElement | null>(null);

const { scrollToBottom } = useChatTranscript({ messages, chatScrollContainer });

const localEmbeddings = useLocalEmbeddings();

const citation = useCitationFinderChat({
  chatStore,
  isLlmConfigured,
  localEmbeddings,
  /* Deferred self-reference: the arrows below only run after `citation`
   * itself is initialized, so the late binding is safe. */
  checkReadiness: () => citation.checkCitationFinderReadiness(),
  runSearch: async (text: string) => {
    await chatStore.sendCitationSearch(text, citation.citationStatusFilter.value);
    scrollToBottom();
  },
});

const {
  citationStatuses,
  isCitationMode,
  citationToggleState,
  citationToggleTitle,
  checkCitationFinderReadiness,
  onSetCitationMode,
  onToggleCitationFinder,
  handleCitationSend,
  mismatchDialog,
  regenerating,
  regeneratingProgress,
  confirmMismatchRegenerate,
  continueMismatchSearch,
  cancelMismatchDialog,
  localPromptOpen,
  confirmLocalDownload,
  confirmLocalUseCloud,
  cancelLocalPrompt,
  handleCopyCitation,
} = citation;

/** Status-checkbox updates arrive as fresh objects from the input area. */
function onStatusesChange(next: CitationStatusFlags): void {
  citation.setCitationStatuses(next);
}

/* Reactively re-check readiness when LLM config changes (provider switch,
 * Test Connection). Deep watch because the config object is mutated in
 * place by Settings auto-save. */
citation.watchLlmConfig(computed(() => llmConfigStore.config));

/* Synchronize updates from the detail view back into the chat's article list. */
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

/** Open the article detail slide-over (closes the wiki reader so only one
 *  slide-over is visible at a time). */
async function openArticleDetail(articleId: string) {
  closeWikiPanel();
  try {
    await selectArticle(articleId);
  } catch {
    toast.show('Failed to load article details', 'error');
  }
}

/** Send an article/wiki chat message through the store, then scroll. */
async function handleSend() {
  if (!chatStore.inputDraft.trim() || chatStore.loading) return;
  const msg = chatStore.inputDraft;
  chatStore.inputDraft = '';
  await chatStore.sendMessage(msg);
  scrollToBottom();
}

/* ── Mode-rail + welcome-card hint actions ──────────────────────────────────
 * The rail icons are mode activators: always active whenever their
 * background conditions permit (wiki present, LLM provider supports
 * embeddings, ...). The empty-transcript welcome cards' hint lines reuse
 * the same handlers. */

/**
 * (+) rail button + article card hint: always active in every mode. From
 * wiki/citation-finder mode it first switches chat back into article mode
 * (`setSource('articles')` - the same exit path as the citation area's
 * close), then opens the article picker.
 */
function onAddContext(): void {
  if (chatStore.source !== 'articles') chatStore.setSource('articles');
  showSelector.value = true;
}

/** Wiki card hint (wiki ready): switch chat into wiki mode. */
function onWelcomeToggleWiki(): void {
  if (chatStore.source !== 'wiki') onToggleWiki();
}

/** Wiki card hint (wiki not ready): jump to the Wiki screen to initialize it. */
function onWelcomeOpenWikiScreen(): void {
  router.push('/wiki');
}

/** Citation card hint: switch chat into Citation Finder mode. */
function onWelcomeActivateCitation(): void {
  if (!isCitationMode.value) onToggleCitationFinder();
}

/** Citation card hint (provider-blocked): jump to Settings. */
function onWelcomeOpenSettings(): void {
  router.push('/settings');
}

onMounted(async () => {
  /* LLM-configured gate is reactive (no IPC probe needed). Kick off wiki
   * status + citation readiness loads so toggle visibility is correct. */
  await Promise.all([loadArticles(), checkWikiStatus(), checkCitationFinderReadiness()]);
  scrollToBottom();
});

// Tear down citation:* listeners on unmount so navigating away from Chat
// does not leave dangling event subscriptions.
onUnmounted(() => {
  stopCitationListeners();
});
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
          <!-- Welcome state: three-column overview of the three chat modes.
               The hint lines are clickable shortcuts into each mode; the
               events are routed by the handlers below. -->
          <ChatWelcomeCards
            v-if="chatStore.messages.length === 0"
            :wiki-ready="chatStore.wikiReady"
            :citation-toggle-state="citationToggleState"
            @open-article-picker="onAddContext"
            @toggle-wiki="onWelcomeToggleWiki"
            @open-wiki-screen="onWelcomeOpenWikiScreen"
            @activate-citation="onWelcomeActivateCitation"
            @open-settings="onWelcomeOpenSettings"
          />

          <!-- Transcript: message bubbles + citation stacks + thinking dots.
               The scroll-anchor querySelector targets [data-msg-idx] inside
               this container, which still reaches the child component's DOM. -->
          <ChatMessageList
            v-else
            :messages="chatStore.messages"
            :wiki-sources="wikiSources"
            :wiki-page-titles="wikiPageTitles"
            :loading="chatStore.loading"
            :source="chatStore.source"
            :citation-mode="isCitationMode"
            @copy="handleCopyCitation"
            @open-wiki="openWikiPage"
            @open-article="openArticleDetail"
          />
        </div>

        <!-- Bottom interaction composer: two columns inside the panel.
             Left: the persistent mode rail (the three round mode toggles,
             always mounted so every mode can be entered and exited from any
             mode - the old chat bar hid the (+) in wiki mode and the whole
             bar in citation mode, which made exits undiscoverable).
             Right: the per-mode context strip + input, separated from the
             rail by a thin vertical divider (the column's left border).
             Outer layout is CSS Grid: auto (fixed-width rail) +
             minmax(0, 1fr) (content track). -->
        <div class="border-t border-slate-200 bg-white p-4">
          <div class="grid grid-cols-[auto_minmax(0,1fr)] items-start gap-4">
            <!-- Mode rail (left column): the same round buttons from the old
                 chat bar - moved, not recreated. The icons are mode
                 activators, always active whenever their background
                 conditions permit (wiki present, LLM provider supports
                 embeddings); the (+) activates article mode from any mode
                 and opens the article-context picker. -->
            <div
              class="mode-rail"
              role="toolbar"
              aria-label="Chat modes"
              aria-orientation="vertical"
            >
              <!-- Plus button: an always-active mode activator. From
                   wiki/citation-finder mode it switches chat back into
                   article mode first, then opens the article-context
                   picker. In article mode it carries the selected
                   treatment (inverse colors), mirroring the other rail
                   toggles' active state. -->
              <button
                class="add-context-toggle flex items-center justify-center w-11 h-11 rounded-full border border-slate-200 bg-white text-indigo-600 transition-all active:scale-95 flex-shrink-0 hover:bg-slate-50"
                :class="{ 'add-context-toggle--active': chatStore.source === 'articles' }"
                title="Add article context"
                @click="onAddContext"
              >
                <span class="material-symbols-outlined text-[24px]">add</span>
              </button>

              <!-- Wiki toggle button. Halo + indigo fill when active. -->
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

              <!-- Citation Finder toggle button (3rd toggle). Visible-but-
                   disabled on known-unsupported providers; hidden only when
                   readiness has not loaded OR the LLM is not configured. -->
              <button
                v-if="citationToggleState !== 'hidden'"
                class="citation-toggle"
                :class="{
                  'citation-toggle--active': isCitationMode,
                  'citation-toggle--disabled': citationToggleState === 'disabled',
                }"
                :title="citationToggleTitle"
                :aria-pressed="isCitationMode"
                :disabled="citationToggleState === 'disabled'"
                @click="onToggleCitationFinder"
              >
                <span class="material-symbols-outlined text-[24px]">quick_reference_all</span>
              </button>
            </div>

            <!-- Main content column (right): fills the remaining horizontal
                 space. The thin vertical divider is this column's left
                 border; min-w-0 keeps long content (pills, banners, prose)
                 from overflowing the grid track. Content keeps its existing
                 order: context strip first, then the active input. -->
            <div class="min-w-0 border-l border-slate-200 pl-5">
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
                   single-line input). -->
              <CitationInputArea
                v-else-if="isCitationMode"
                :readiness="chatStore.citationReadiness"
                :toggle-state="citationToggleState"
                :style-value="chatStore.citationStyle"
                :mode="chatStore.citationFinderMode"
                :statuses="citationStatuses"
                :draft="chatStore.citationDraft"
                :progress="chatStore.citationProgress"
                :loading="chatStore.loading"
                :cancelling="chatStore.cancelling"
                @update:style="chatStore.setCitationStyle"
                @update:mode="onSetCitationMode"
                @update:statuses="onStatusesChange"
                @update:draft="chatStore.citationDraft = $event"
                @send="handleCitationSend"
                @cancel="chatStore.cancelCitationSearch()"
                @close="onToggleCitationFinder"
                @open-settings="router.push('/settings')"
              />

              <!-- Selected articles panel (article mode only) -->
              <SelectedArticlesBar
                v-else
                :articles="selectedArticles"
                @open-detail="openArticleDetail"
                @remove="chatStore.removeSelectedArticle"
                @clear="chatStore.clearSelectedArticles"
              />

              <!-- Chat input row. HIDDEN in citation-finder mode: the
                   citation input area above owns the active input (prose
                   textarea + Find/progress). Wiki + article modes keep the
                   single-line input. The mode toggles live in the rail. -->
              <div v-if="!isCitationMode" class="flex items-center gap-3">
                <!-- Input field -->
                <div class="flex-1 relative">
                  <input
                    v-model="chatStore.inputDraft"
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
                    :disabled="!chatStore.inputDraft.trim() || chatStore.loading"
                    @click="handleSend"
                  >
                    <span class="material-symbols-outlined text-[18px]">send</span>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

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

    <!-- Citation Finder embedding model-mismatch dialog. -->
    <CitationMismatchDialog
      :mismatch="mismatchDialog"
      :regenerating="regenerating"
      :regenerating-progress="regeneratingProgress"
      @regenerate="confirmMismatchRegenerate"
      @continue="continueMismatchSearch"
      @cancel="cancelMismatchDialog"
    />

    <!-- T7: contextual Bango Local download prompt (local backend selected
         but components not installed: Download / Use Configured Provider /
         Cancel). -->
    <CitationLocalEmbeddingsDialog
      v-if="localPromptOpen"
      :status="localEmbeddings.status.value"
      :progress="localEmbeddings.progress.value"
      :installing="localEmbeddings.installing.value"
      :error="localEmbeddings.error.value"
      :chat-provider-supports-embeddings="
        chatStore.citationReadiness?.chatProviderSupportsEmbeddings ?? false
      "
      @download="confirmLocalDownload"
      @use-cloud="confirmLocalUseCloud"
      @cancel="cancelLocalPrompt"
    />

    <!-- Article Selection Modal -->
    <ArticleSelectorModal
      :open="showSelector"
      :articles="articles"
      :selected-ids="chatStore.selectedArticleIds"
      @toggle="toggleArticleSelection"
      @close="showSelector = false"
      @done="showSelector = false"
    />
  </div>
</template>

<style scoped>
@keyframes fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.animate-fade-in {
  animation: fade-in 0.3s ease-out forwards;
}

/* Persistent mode rail (the composer's left column): the three round mode
   toggles stacked vertically, top-aligned, horizontally centered, equal
   12px gaps. Fixed width (the grid's auto column) as the panel resizes. */
.mode-rail {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.75rem; /* 12px button gaps */
  flex-shrink: 0;
}

/* Article-mode selected state: mirrors the wiki/citation active treatment
   (inverse colors - indigo fill + white icon + halo) so the rail always
   shows which mode is active. Doubled selector + :hover guard so it wins
   over the button's inline Tailwind hover utilities. */
.add-context-toggle.add-context-toggle--active,
.add-context-toggle.add-context-toggle--active:hover {
  background-color: rgb(99 102 241); /* indigo-600 */
  border-color: rgb(79 70 229); /* indigo-700 */
  color: #fff;
  box-shadow:
    0 0 0 3px rgb(199 210 254 / 0.9),
    0 1px 2px rgb(15 23 42 / 0.08);
}

/* Wiki mode toggle button (mode rail). Halo + indigo fill when active.
   44px round to match the rail's (+) button. */
.wiki-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.75rem;
  height: 2.75rem;
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

/* Citation Finder toggle button (3rd rail toggle, mirrors .wiki-toggle;
   44px round to match the rail's (+) button). */
.citation-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.75rem;
  height: 2.75rem;
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

/* Disabled state: known-unsupported provider (Anthropic, Z.AI). The toggle
   stays visible (so the user can see the feature exists + the tooltip tells
   them to switch providers) but is muted + non-interactive. */
.citation-toggle--disabled,
.citation-toggle--disabled:hover {
  background-color: rgb(241 245 249); /* slate-100 */
  border-color: rgb(226 232 240); /* slate-200 */
  color: rgb(148 163 184); /* slate-400 */
  cursor: not-allowed;
  box-shadow: none;
  opacity: 0.7;
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
