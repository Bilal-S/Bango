<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { useWiki } from '@/composables/use-wiki';
import { renderWikiMarkdown } from '@/utils/wiki-markdown';
import { highlightSearchTerms } from '@/utils/highlight';
import type { WikiPage, WikiSourceInfo, RawFileEntry } from '@/types/wiki';

const props = defineProps<{
  slug: string | null;
  /** Optional sidebar search query. When non-empty, occurrences of the search
   *  terms in the rendered body are wrapped in `<mark class="wiki-search-highlight">`
   *  so the user can see where the term appears. Default empty (no highlight). */
  highlightQuery?: string;
}>();

const emit = defineEmits<{
  navigate: [slug: string];
  close: [];
  viewArticle: [articleId: string];
  /** Emitted when the user clicks a source that is an external document
   * (uploaded via Add Documents; `source_kind` starts with `user_`). The parent
   * resolves the original file path and opens it in the OS default viewer. */
  openSource: [slug: string];
}>();

const { getPage, listSources, listPages, listRawFiles } = useWiki();

const page = ref<WikiPage | null>(null);
const sources = ref<Map<string, WikiSourceInfo>>(new Map());
const pageTitles = ref<Map<string, string>>(new Map());
/** Raw-file metadata (slug → entry) for external-document detection. The
 * `source_kind` field (`user_pdf`, `user_text`, ...) distinguishes uploaded
 * documents from real article sources, so the Sources-footer click can branch:
 * article → ArticleDetailPanel (existing), external doc → OS viewer (new). */
const rawFiles = ref<Map<string, RawFileEntry>>(new Map());
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

/** Load raw-file metadata once for external-document detection. External docs
 * (added via Add Documents) have `source_kind: user_*`; real article sources do
 * not. Used to branch the Sources-footer click: article → ArticleDetailPanel,
 * external doc → OS default viewer via `openSource` emit. */
async function loadRawFiles(): Promise<void> {
  if (rawFiles.value.size > 0) return;
  try {
    const list = await listRawFiles();
    const map = new Map<string, RawFileEntry>();
    for (const f of list) {
      // Index by slug so source_articles entries (which use the slug as id)
      // resolve. External-doc source pages set source_articles to the slug.
      if (f.slug) map.set(f.slug, f);
    }
    rawFiles.value = map;
  } catch {
    // Non-fatal: footer falls back to the article-detail behavior.
  }
}

/** True when the source id refers to an external document (uploaded via Add
 * Documents) rather than a real DB article. Detection is data-driven: the raw
 * file's `source_kind` must start with `user_`. Real article sources have no
 * `source_kind` field (or a non-`user_` value) and fall through to the default
 * `viewArticle` path — preserving the existing ArticleDetailPanel behavior. */
function isExternalSource(artId: string): boolean {
  const entry = rawFiles.value.get(artId);
  return !!entry && entry.sourceKind.startsWith('user_');
}

/** Sources-footer click: branch between the existing article-detail panel
 * (default, unchanged for real articles) and the new external-document OS
 * viewer (for `user_*` sources only). */
function onSourceClick(artId: string): void {
  if (isExternalSource(artId)) {
    emit('openSource', artId);
  } else {
    emit('viewArticle', artId);
  }
}

/** Display name for a Sources-footer entry. For external documents, appends
 * the original file extension (from `source_file`) so the user can tell what
 * type of file will open — e.g. "notes.txt", "report.pdf". For real articles,
 * the title is used unchanged. Avoids double-appending if the title already
 * ends with the extension. */
function sourceDisplayName(artId: string): string {
  const raw = rawFiles.value.get(artId);
  if (raw && isExternalSource(artId) && raw.sourceFile) {
    const dot = raw.sourceFile.lastIndexOf('.');
    if (dot > 0) {
      const ext = raw.sourceFile.slice(dot).toLowerCase();
      const base = raw.title || artId;
      return base.toLowerCase().endsWith(ext) ? base : `${base}${ext}`;
    }
  }
  return sources.value.get(artId)?.title ?? raw?.title ?? artId;
}

/**
 * Render Markdown body with [[wikilinks]] and [^art-id] references converted to
 * clickable spans. Delegated to the shared renderer so the chat view and the
 * wiki viewer produce identical click targets and styling hooks.
 */
const renderedBody = computed(() => {
  if (!page.value) return '';
  const html = renderWikiMarkdown(page.value.body, {
    sources: sources.value,
    pageTitles: pageTitles.value,
    // Author pages: each publication's [^art-{uuid}] ref should open the wiki
    // synthesis page (slug = uuid) rather than the article detail, since the
    // synthesis page already links to the source and the "Sources" footer at
    // the bottom of the author page covers direct article access.
    linkArtRefsToSynthesis: page.value.pageType === 'author',
  });
  // Apply search-term highlighting (yellow <mark>) when a sidebar query is
  // active. Operates only on text segments - tags/attributes are untouched.
  return props.highlightQuery ? highlightSearchTerms(html, props.highlightQuery) : html;
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
    await Promise.all([loadSources(), loadPageTitles(), loadRawFiles()]);
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
            @click="onSourceClick(artId)"
          >
            <span class="material-symbols-outlined text-[14px]">description</span>
            <span>{{ sourceDisplayName(artId) }}</span>
            <!-- External documents (uploaded via Add Documents) open in the OS
                 default viewer; show the open_in_new glyph to signal that. -->
            <span
              v-if="isExternalSource(artId)"
              class="material-symbols-outlined text-[14px] wiki-page-viewer__source-external"
              >open_in_new</span
            >
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

/* T2.3 Phase 3: muted section-provenance badge rendered after a wikilink
 * when the citation carries a `(§Section)` suffix (e.g. `[[slug]] (§Methods)`).
 * Keeps the passage locator visible without crowding the chip itself. */
.wiki-page-viewer :deep(.section-badge) {
  display: inline-block;
  margin-left: 0.25rem;
  padding: 0.0625rem 0.3125rem;
  font-size: 0.7em;
  font-weight: 500;
  color: rgb(100 116 139); /* slate-500 */
  background: rgb(241 245 249); /* slate-100 */
  border: 1px solid rgb(226 232 240); /* slate-200 */
  border-radius: 0.25rem;
  vertical-align: baseline;
}

/* Search-term highlight (active sidebar search query). */
.wiki-page-viewer :deep(.wiki-search-highlight) {
  background: rgb(253 224 71); /* yellow-300 */
  color: rgb(15 23 42);
  padding: 0 0.125rem;
  border-radius: 0.125rem;
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

/* External-document indicator (open_in_new glyph at end of the source name). */
.wiki-page-viewer__source-external {
  margin-left: auto;
  color: rgb(100 116 139);
  font-size: 12px !important;
  opacity: 0.7;
}

.wiki-page-viewer__source:hover .wiki-page-viewer__source-external {
  color: rgb(79 70 229);
  opacity: 1;
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
