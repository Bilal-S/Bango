<script setup lang="ts">
/**
 * Welcome state for an empty chat transcript: a three-column overview of the
 * three chat modes (article RAG, wiki RAG, Citation Finder). The Citation
 * Finder card's hint branches on the same toggle state that drives the
 * chat-bar toggle button, so the guidance always matches the toggle. Every
 * hint line is a clickable button that emits the action it describes; the
 * host view (chat-view) routes the events (props-down/events-up, no store
 * access).
 */
defineProps<{
  /** Whether the wiki is initialized with at least one page. */
  wikiReady: boolean;
  /** The chat-bar Citation Finder toggle state driving the hint branch. */
  citationToggleState: 'enabled' | 'unknown' | 'disabled' | 'hidden';
}>();

const emit = defineEmits<{
  /** Article card hint: open the article picker dialog (the (+) flow). */
  openArticlePicker: [];
  /** Wiki card hint (wiki ready): switch chat into wiki mode. */
  toggleWiki: [];
  /** Wiki card hint (wiki not ready): navigate to the Wiki screen. */
  openWikiScreen: [];
  /** Citation card hint (enabled/unknown): switch chat into Citation Finder mode. */
  activateCitation: [];
  /** Citation card hint (disabled/hidden): navigate to Settings. */
  openSettings: [];
}>();
</script>

<template>
  <div class="my-auto py-8 w-full max-w-5xl mx-auto">
    <div class="chat-welcome-grid">
      <!-- Academic Research Chat -->
      <div class="chat-welcome-card">
        <div class="chat-welcome-card__icon chat-welcome-card__icon--indigo">
          <span class="material-symbols-outlined">chat_add_on</span>
        </div>
        <h3 class="chat-welcome-card__title">Academic Research Chat</h3>
        <p class="chat-welcome-card__desc">
          Ask questions about the articles in your library. Add articles to the context using the
          <strong>(+)</strong> button to ground the responses in specific research text.
        </p>
        <button
          type="button"
          class="chat-welcome-card__hint"
          title="Open the article picker"
          @click="emit('openArticlePicker')"
        >
          <span class="material-symbols-outlined">add_circle</span>
          <span class="chat-welcome-card__hint-text">
            Click <strong>(+)</strong> to select articles, then type your question.
          </span>
        </button>
      </div>

      <!-- Wiki Chat -->
      <div class="chat-welcome-card">
        <div class="chat-welcome-card__icon chat-welcome-card__icon--purple">
          <span class="material-symbols-outlined">local_library</span>
        </div>
        <h3 class="chat-welcome-card__title">Wiki Chat</h3>
        <p class="chat-welcome-card__desc">
          Ask questions answered from your synthesized knowledge base. The Wiki is built from your
          included articles and retrieves the most relevant pages for each question.
        </p>
        <button
          v-if="wikiReady"
          type="button"
          class="chat-welcome-card__hint"
          title="Switch to Wiki chat"
          @click="emit('toggleWiki')"
        >
          <span class="material-symbols-outlined">local_library</span>
          <span class="chat-welcome-card__hint-text">
            Toggle the <strong>Wiki</strong> icon (right of <strong>(+)</strong>) to start.
          </span>
        </button>
        <button
          v-else
          type="button"
          class="chat-welcome-card__hint chat-welcome-card__hint--muted"
          title="Open the Wiki screen"
          @click="emit('openWikiScreen')"
        >
          <span class="material-symbols-outlined">lock</span>
          <span class="chat-welcome-card__hint-text">
            Initialize the Wiki first (see the Wiki screen).
          </span>
        </button>
      </div>

      <!-- Citation Finder -->
      <div class="chat-welcome-card">
        <div class="chat-welcome-card__icon chat-welcome-card__icon--teal">
          <span class="material-symbols-outlined">quick_reference_all</span>
        </div>
        <h3 class="chat-welcome-card__title">Citation Finder</h3>
        <p class="chat-welcome-card__desc">
          Paste text you are writing and get matching citations from your library. Bango finds the
          relevant passages first, so the AI cannot invent sources: every result is grounded in your
          real articles.
        </p>
        <!-- The hint branches on the toggle state (the same computed that
             drives the toggle button). The disabled branch surfaces the
             "switch provider" message so the user on a known-unsupported
             provider (Anthropic, Z.AI) sees an actionable warning instead of
             a misleading "click to start" or a silent lock icon. All
             variants are clickable: the enabled/unknown branch switches to
             Citation Finder mode, the provider-blocked branches jump to
             Settings. -->
        <button
          v-if="citationToggleState === 'enabled' || citationToggleState === 'unknown'"
          type="button"
          class="chat-welcome-card__hint"
          title="Switch to Citation Finder mode"
          @click="emit('activateCitation')"
        >
          <span class="material-symbols-outlined">quick_reference_all</span>
          <span class="chat-welcome-card__hint-text">
            Click the <strong>Citation Finder</strong> icon to start.
          </span>
        </button>
        <button
          v-else-if="citationToggleState === 'disabled'"
          type="button"
          class="chat-welcome-card__hint chat-welcome-card__hint--warning"
          title="Open Settings"
          @click="emit('openSettings')"
        >
          <span class="material-symbols-outlined">block</span>
          <span class="chat-welcome-card__hint-text">
            Your provider does not support embeddings. Switch to OpenAI, Google or a local provider
            (Ollama, LM Studio) in Settings to use Citation Finder.
          </span>
        </button>
        <button
          v-else
          type="button"
          class="chat-welcome-card__hint chat-welcome-card__hint--muted"
          title="Open Settings"
          @click="emit('openSettings')"
        >
          <span class="material-symbols-outlined">lock</span>
          <span class="chat-welcome-card__hint-text">
            Requires an embedding-capable LLM provider (see Settings).
          </span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
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

/* Hints are actionable buttons that perform the action they describe. Tailwind
   preflight is disabled in this project, so the browser button chrome must be
   reset explicitly. */
button.chat-welcome-card__hint {
  width: 100%;
  background: none;
  border: none;
  padding: 0;
  /* Keep the base hint's spacing above the line (the reset above wins on
     specificity, so re-assert it here). */
  padding-top: 0.5rem;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
}

/* Hover/focus affordance: the text (not the icon) gains an underline. */
.chat-welcome-card__hint-text {
  text-decoration: underline;
  text-decoration-color: transparent;
  text-underline-offset: 2px;
  transition: text-decoration-color 0.15s;
}

button.chat-welcome-card__hint:hover .chat-welcome-card__hint-text,
button.chat-welcome-card__hint:focus-visible .chat-welcome-card__hint-text {
  text-decoration-color: currentcolor;
}

button.chat-welcome-card__hint:focus-visible {
  outline: 2px solid rgb(99 102 241); /* indigo-600 */
  outline-offset: 2px;
  border-radius: 0.375rem;
}

.chat-welcome-card__hint--muted {
  color: rgb(148 163 184); /* slate-400 */
  font-weight: 500;
}

/* Warning variant: known-unsupported provider (Anthropic, Z.AI). Amber
   chrome so it reads as an actionable "switch provider" warning instead of
   the muted "not available" lock. Matches the citation-disabled-banner
   palette. */
.chat-welcome-card__hint--warning {
  color: rgb(180 83 9); /* amber-700 */
  font-weight: 600;
}
</style>
