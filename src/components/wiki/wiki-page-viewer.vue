<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { useWiki } from '@/composables/use-wiki';
import { renderWikiMarkdown } from '@/utils/wiki-markdown';
import type { WikiPage, WikiSourceInfo } from '@/types/wiki';

const props = defineProps<{
  slug: string | null;
}>();

const emit = defineEmits<{
  navigate: [slug: string];
  close: [];
  viewArticle: [articleId: string];
}>();

const { getPage, listSources, listPages } = useWiki();

const page = ref<WikiPage | null>(null);
const sources = ref<Map<string, WikiSourceInfo>>(new Map());
const pageTitles = ref<Map<string, string>>(new Map());
const loading = ref(false);
const error = ref<string | null>(null);

/** Load source metadata once for [^art-id] resolution. */
async function loadSources(): Promise<void> {
  if (sources.value.size > 0) return;
  try {
    const list = await listSources();
    const map = new Map<string, WikiSourceInfo>();
    for (const s of list) {
      map.set(s.id, s);
    }
    sources.value = map;
  } catch {
    // Non-fatal: references will show as raw IDs.
  }
}

/** Load wiki page titles once so bare UUIDs in prose can render as
 *  synthesis-styled chips with human-readable titles instead of raw UUIDs. */
async function loadPageTitles(): Promise<void> {
  if (pageTitles.value.size > 0) return;
  try {
    const pages = await listPages();
    const map = new Map<string, string>();
    for (const p of pages) {
      map.set(p.slug, p.title);
    }
    pageTitles.value = map;
  } catch {
    // Non-fatal: bare UUIDs fall back to source labels or raw UUIDs.
  }
}

/** Parse the source_articles JSON array from frontmatter. */
function parseSourceArticles(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const arr = JSON.parse(raw);
    return Array.isArray(arr) ? (arr as string[]) : [];
  } catch {
    return [];
  }
}

/**
 * Render Markdown body with [[wikilinks]] and [^art-id] references converted to
 * clickable spans. Delegated to the shared renderer so the chat view and the
 * wiki viewer produce identical click targets and styling hooks.
 */
const renderedBody = computed(() => {
  if (!page.value) return '';
  return renderWikiMarkdown(page.value.body, {
    sources: sources.value,
    pageTitles: pageTitles.value,
    // Author pages: each publication's [^art-{uuid}] ref should open the wiki
    // synthesis page (slug = uuid) rather than the article detail, since the
    // synthesis page already links to the source and the "Sources" footer at
    // the bottom of the author page covers direct article access.
    linkArtRefsToSynthesis: page.value.pageType === 'author',
  });
});

/** Load the page when the slug changes. */
async function loadPage(): Promise<void> {
  if (!props.slug || typeof props.slug !== 'string') {
    page.value = null;
    return;
  }
  loading.value = true;
  error.value = null;
  try {
    await Promise.all([loadSources(), loadPageTitles()]);
    page.value = await getPage(props.slug);
    if (!page.value) {
      error.value = `Page "${props.slug}" not found.`;
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

/** Handle clicks on wikilinks and article references inside the rendered content. */
function handleClick(event: MouseEvent): void {
  const target = event.target as HTMLElement;
  if (target.classList.contains('wikilink')) {
    const slug = target.getAttribute('data-slug');
    if (slug) {
      emit('navigate', slug);
    }
  } else if (target.classList.contains('art-ref')) {
    const artId = target.getAttribute('data-art-id');
    if (artId) {
      emit('viewArticle', artId);
    }
  }
}

watch(() => props.slug, loadPage, { immediate: true });
</script>

<template>
  <div class="wiki-page-viewer">
    <button v-if="page || error" class="wiki-page-viewer__close" @click="emit('close')">
      <span class="material-symbols-outlined text-[18px]">close</span>
    </button>

    <div v-if="loading" class="wiki-page-viewer__loading">
      <span class="material-symbols-outlined spin">progress_activity</span>
      <span>Loading page...</span>
    </div>

    <div v-else-if="error" class="wiki-page-viewer__error">
      <span class="material-symbols-outlined text-[24px]">error</span>
      <p>{{ error }}</p>
    </div>

    <div v-else-if="!page" class="wiki-page-viewer__empty">
      <span class="material-symbols-outlined text-[48px] text-slate-300">article</span>
      <p>Select a page to read.</p>
    </div>

    <article v-else class="wiki-page-viewer__content">
      <header class="wiki-page-viewer__header">
        <h1>{{ page.title }}</h1>
        <div class="wiki-page-viewer__meta">
          <span class="badge badge--type">{{ page.pageType }}</span>
          <span v-if="page.status === 'reviewed'" class="badge badge--reviewed">reviewed</span>
          <span v-if="page.summary" class="wiki-page-viewer__summary">{{ page.summary }}</span>
        </div>
      </header>
      <!-- eslint-disable-next-line vue/no-v-html -- wikilinks + art-refs are sanitized to data attributes, content is from local wiki -->
      <div class="markdown-content" @click="handleClick" v-html="renderedBody" />

      <footer v-if="page.sourceArticles" class="wiki-page-viewer__sources">
        <h3>Sources</h3>
        <ul>
          <li
            v-for="artId in parseSourceArticles(page.sourceArticles)"
            :key="artId"
            class="wiki-page-viewer__source"
            @click="emit('viewArticle', artId)"
          >
            <span class="material-symbols-outlined text-[14px]">description</span>
            <span>{{ sources.get(artId)?.title ?? artId }}</span>
          </li>
        </ul>
      </footer>
    </article>
  </div>
</template>

<style scoped>
.wiki-page-viewer {
  position: relative;
  height: 100%;
  overflow-y: auto;
  padding: 1.5rem;
  background: #fff;
}

.wiki-page-viewer__close {
  position: absolute;
  top: 0.75rem;
  right: 0.75rem;
  display: inline-flex;
  align-items: center;
  padding: 0.25rem;
  border: none;
  background: transparent;
  color: rgb(100 116 139);
  cursor: pointer;
  border-radius: 0.375rem;
}

.wiki-page-viewer__close:hover {
  background: rgb(241 245 249);
}

.wiki-page-viewer__loading,
.wiki-page-viewer__error,
.wiki-page-viewer__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  height: 100%;
  color: rgb(148 163 184);
  font-size: 0.875rem;
}

.wiki-page-viewer__error {
  color: rgb(239 68 68);
}

.wiki-page-viewer__content {
  max-width: 48rem;
  margin: 0 auto;
}

.wiki-page-viewer__header {
  margin-bottom: 1.5rem;
  border-bottom: 1px solid rgb(226 232 240);
  padding-bottom: 1rem;
}

.wiki-page-viewer__header h1 {
  font-size: 1.5rem;
  font-weight: 700;
  color: rgb(15 23 42);
  margin-bottom: 0.5rem;
}

.wiki-page-viewer__meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.wiki-page-viewer__summary {
  font-size: 0.8rem;
  color: rgb(71 85 105);
  font-style: italic;
}

.badge {
  display: inline-block;
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  font-size: 0.65rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.025em;
}

.badge--type {
  background: rgb(219 234 254);
  color: rgb(30 64 175);
}

.badge--reviewed {
  background: rgb(220 252 231);
  color: rgb(22 101 52);
}

.wiki-page-viewer :deep(.wikilink) {
  color: rgb(79 70 229);
  text-decoration: underline;
  cursor: pointer;
  text-decoration-style: dotted;
}

.wiki-page-viewer :deep(.wikilink:hover) {
  text-decoration-style: solid;
}

/* Synthesis-styled wikilink chip (from [^art-uuid]: definition lines). */
.wiki-page-viewer :deep(.wikilink--synthesis) {
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

.wiki-page-viewer :deep(.wikilink--synthesis:hover) {
  background: rgb(168 85 247 / 0.2);
}

.wiki-page-viewer :deep(.art-ref) {
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

.wiki-page-viewer :deep(.art-ref:hover) {
  background: rgb(220 252 231);
}

.wiki-page-viewer :deep(.art-ref--missing) {
  color: rgb(148 163 184);
  background: rgb(241 245 249);
  border-color: rgb(226 232 240);
}

.wiki-page-viewer__sources {
  margin-top: 2rem;
  padding-top: 1rem;
  border-top: 1px solid rgb(226 232 240);
}

.wiki-page-viewer__sources h3 {
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.055em;
  color: rgb(100 116 139);
  margin-bottom: 0.5rem;
}

.wiki-page-viewer__sources ul {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.wiki-page-viewer__source {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  font-size: 0.75rem;
  color: rgb(71 85 105);
  cursor: pointer;
  padding: 0.25rem 0.375rem;
  border-radius: 0.25rem;
}

.wiki-page-viewer__source:hover {
  background: rgb(241 245 249);
  color: rgb(79 70 229);
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
