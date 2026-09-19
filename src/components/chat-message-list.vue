<script setup lang="ts">
/**
 * Chat transcript: the message bubbles (user/assistant), the Citation Finder
 * result stacks (per-statement claim groups + whole-block flat list), the
 * thinking indicator, and delegated bubble-click routing for wiki links and
 * article references. Owns the per-claim collapse state and the IEEE card
 * flattening; rendering of markdown bodies goes through the wiki renderer
 * for wiki-sourced messages and plain `marked` otherwise.
 */
import { ref } from 'vue';
import { marked } from 'marked';
import { renderWikiMarkdown } from '@/utils/wiki-markdown';
import CitationResultCard from '@/components/citation-result-card.vue';
import type { ChatMessage } from '@/stores/chat';
import type { CitationResult, CitationStyle } from '@/types/citation-finder';
import type { WikiSourceInfo } from '@/types/wiki';

const props = defineProps<{
  /** The transcript (from the chat store). */
  messages: ChatMessage[];
  /** Article id -> source metadata, for wiki-rendered article chips. */
  wikiSources: Map<string, WikiSourceInfo>;
  /** Wiki slug -> title, for human-readable wiki chips. */
  wikiPageTitles: Map<string, string>;
  /** True while an article/wiki chat response is pending (thinking dots). */
  loading: boolean;
  /** Active retrieval source (drives the thinking-indicator label). */
  source: string;
  /** True while Citation Finder mode owns the input (hides the dots). */
  citationMode: boolean;
}>();

const emit = defineEmits<{
  /** Copy a formatted citation string to the clipboard (parent toasts). */
  copy: [text: string];
  /** A [[wikilink]] inside an assistant bubble was clicked. */
  openWiki: [slug: string];
  /** An article reference chip inside an assistant bubble was clicked. */
  openArticle: [articleId: string];
}>();

/**
 * Render an assistant message body. Wiki-sourced messages go through the
 * shared wiki renderer so `[[slug]]` citations become clickable
 * `.wikilink` spans; article-sourced messages use plain `marked` (no
 * wikilink interpretation, so bracketed text in article content is never
 * misinterpreted).
 */
function renderMessage(msg: ChatMessage): string {
  if (msg.source === 'wiki') {
    return renderWikiMarkdown(msg.content, {
      sources: props.wikiSources,
      pageTitles: props.wikiPageTitles,
      /* Chat view: articles win over wiki pages for bare UUID resolution.
       * Article UUID renders as green art-ref even when synthesis page
       * exists. */
      articlePriority: true,
    });
  }
  return marked.parse(msg.content) as string;
}

/** Delegated click handler for assistant bubbles: detect wiki links and
 * article references and route them to the right slide-over. */
function handleBubbleClick(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (target.classList.contains('wikilink')) {
    const slug = target.getAttribute('data-slug');
    if (slug) emit('openWiki', slug);
  } else if (target.classList.contains('art-ref')) {
    const artId = target.getAttribute('data-art-id');
    if (artId) emit('openArticle', artId);
  }
}

/** One flattened card entry with its 1-based IEEE index. */
interface FlattenedCard {
  match: CitationResult['matches'][number];
  ieeeIndex: number;
  claim: string | null;
}

/** Flatten a `CitationResult[]` into a single card list for IEEE `[N]`
 * numbering across the whole bubble (per-bubble numbering). */
function flattenForIeee(results: CitationResult[]): FlattenedCard[] {
  const out: FlattenedCard[] = [];
  let idx = 1;
  for (const group of results) {
    for (const match of group.matches) {
      out.push({ match, ieeeIndex: idx, claim: group.claim });
      idx += 1;
    }
  }
  return out;
}

/* Per-statement claim-group collapse state. Each claim heading is a caret
 * toggle. Default expanded. Keyed by `${msgIdx}::${claim}` so re-searches
 * stay independent; state survives as long as message list is append-only. */
const collapsedClaims = ref<Set<string>>(new Set());

function claimKey(msgIdx: number, claim: string): string {
  return `${msgIdx}::${claim}`;
}

function isClaimCollapsed(msgIdx: number, claim: string): boolean {
  return collapsedClaims.value.has(claimKey(msgIdx, claim));
}

/** Toggle a claim's collapse state. Mutating a `Set` in place doesn't
 * trigger reactivity, so the ref is reassigned to a fresh `Set`. */
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

/** Count the cards under a given claim (for the count badge). Reuses the
 * same filter predicate the template uses so the number always matches. */
function claimCardCount(results: CitationResult[], claim: string): number {
  return flattenForIeee(results).filter((c) => c.claim === claim).length;
}

/** Fallback style guard for legacy bubbles frozen before a style existed. */
function bubbleStyle(msg: ChatMessage): CitationStyle {
  return msg.citationStyle ?? 'APA';
}
</script>

<template>
  <template v-for="(msg, idx) in messages" :key="idx">
    <div
      :data-msg-idx="idx"
      class="flex flex-col max-w-[80%]"
      :class="
        msg.role === 'user'
          ? 'self-end items-end animate-slide-in-up'
          : 'self-start items-start animate-slide-in-left'
      "
    >
      <!-- Sender details -->
      <span class="text-[11px] text-slate-400 mb-1 font-medium px-1 flex items-center gap-1">
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
        <!-- Citation Finder results: render the card stack instead of the
             Markdown body. Per-bubble style frozen at submit time; IEEE [N]
             numbering is the flattened card order across the whole bubble
             (per-statement groups render claim headings). -->
        <template v-else-if="msg.citations">
          <div class="citation-bubble">
            <p v-if="msg.content" class="citation-bubble__summary">{{ msg.content }}</p>
            <template v-if="msg.citations.some((g) => g.claim !== null)">
              <!-- Per-statement: group cards under claim headings. The
                   predicate is "any group carries a non-null claim", NOT
                   `length > 1`: per-statement mode that produces exactly 1
                   claim still has `claim: Some`, and the claim heading must
                   render (whole-block always has `claim: null`). -->
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
                      'citation-bubble__claim-caret--collapsed': isClaimCollapsed(idx, group.claim),
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
                  :style="bubbleStyle(msg)"
                  :ieee-index="card.ieeeIndex"
                  @copy="emit('copy', $event)"
                  @view="emit('openArticle', $event)"
                />
              </div>
            </template>
            <template v-else>
              <!-- Whole-block: flat card list (every group has `claim: null`). -->
              <CitationResultCard
                v-for="card in flattenForIeee(msg.citations)"
                :key="card.match.articleId + '-' + card.ieeeIndex"
                :match="card.match"
                :style="bubbleStyle(msg)"
                :ieee-index="card.ieeeIndex"
                @copy="emit('copy', $event)"
                @view="emit('openArticle', $event)"
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

  <!-- Loading / Thinking indicator. HIDDEN in citation-finder mode: the
       citation-progress bar in the input area already communicates Phase B/C
       status (with a Cancel button + per-phase message), so the generic
       "Analyzing article context..." text would be stale, misleading, and
       redundant. Wiki + article modes keep the thinking dots. -->
  <div
    v-if="loading && !citationMode"
    class="flex flex-col items-start max-w-[80%] self-start animate-pulse"
  >
    <span class="text-[11px] text-slate-400 mb-1 font-medium px-1 flex items-center gap-1">
      Assistant &bull; Thinking
      <span v-if="source === 'wiki'" class="wiki-badge">wiki</span>
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
        source === 'wiki' ? 'Searching wiki pages...' : 'Analyzing article context...'
      }}</span>
    </div>
  </div>
</template>

<style scoped>
/* Bubble entrance animations: user bubbles rise, assistant bubbles enter
 * from the left - the natural chat idiom. */
@keyframes chat-slide-in-up {
  from {
    transform: translateY(18px);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}

@keyframes chat-slide-in-left {
  from {
    transform: translateX(-12px);
    opacity: 0;
  }
  to {
    transform: translateX(0);
    opacity: 1;
  }
}

.animate-slide-in-up {
  animation: chat-slide-in-up 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

.animate-slide-in-left {
  animation: chat-slide-in-left 0.25s cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

.dot-1,
.dot-2,
.dot-3 {
  animation: chat-bounce 1.4s infinite ease-in-out both;
}
.dot-1 {
  animation-delay: -0.32s;
}
.dot-2 {
  animation-delay: -0.16s;
}

@keyframes chat-bounce {
  0%,
  80%,
  100% {
    transform: scale(0);
  }
  40% {
    transform: scale(1);
  }
}

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

/* Per-statement claim-group collapse toggle. */
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

/* Wiki link styling inside assistant bubbles. Mirrors wiki-page-viewer.vue
   so clicks feel consistent. The synthesis chip is excluded: its shared
   rules live in styles/markdown.css and must not have to out-specify these
   scoped (0,3,0) base rules. */
.markdown-content :deep(.wikilink:not(.wikilink--synthesis)) {
  color: rgb(79 70 229);
  text-decoration: underline;
  cursor: pointer;
  text-decoration-style: dotted;
}
.markdown-content :deep(.wikilink:not(.wikilink--synthesis):hover) {
  text-decoration-style: solid;
}
</style>
