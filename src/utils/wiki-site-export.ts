/**
 * Wiki static-site export engine (v3 two-step process).
 *
 * Step 1 (`generateWikiExport`): gathers all wiki page data via existing Tauri
 * commands, renders each page to a standalone HTML document (reusing
 * `renderWikiMarkdown` with `staticMode`), generates article-stub pages,
 * builds an index + search + style, and passes the bundle to the
 * `wiki_generate_export` backend command which writes it to
 * `wiki-root/wiki-export/` (persistent, testable on disk).
 *
 * Step 2 (`zipWikiExport`): opens a save dialog and calls `wiki_zip_export`
 * to zip the `wiki-export/` directory into a user-chosen `.zip` file.
 *
 * **CSS**: the export stylesheet combines:
 * 1. The real `src/styles/markdown.css` (imported via Vite `?raw`) — the same
 *    typography the in-app wiki viewer uses for `.markdown-content`.
 * 2. Wiki-specific styles extracted from `wiki-page-viewer.vue` (un-scoped) —
 *    `.wikilink`, `.art-ref`, `.wikilink--synthesis`, `.section-badge`.
 * This guarantees the exported pages look identical to the in-app wiki viewer.
 */
import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '@/composables/use-tauri-command';
import { renderWikiMarkdown } from '@/utils/wiki-markdown';
import type {
  ExportFile,
  GenerateExportResult,
  SiteExportBundle,
  WikiPageSummary,
  WikiSourceInfo,
} from '@/types/wiki';

// Vite `?raw` import: embeds the full markdown.css file as a string at build
// time. This is the same CSS the in-app wiki viewer uses, so the exported
// pages have identical typography (headings, tables, code, blockquotes, etc.).
import markdownCss from '@/styles/markdown.css?raw';

/** Author slug resolution map (raw name -> author page slug). */
export interface AuthorSlugMap {
  readonly rawName: string;
  readonly slug: string;
}

/** Internal context built by `gatherExportContext`. */
interface ExportContext {
  readonly pages: WikiPageSummary[];
  readonly pageBodies: Map<string, string>;
  readonly sources: WikiSourceInfo[];
  readonly authorSlugs: AuthorSlugMap[];
}

/**
 * Wiki-specific styles extracted from `wiki-page-viewer.vue`'s scoped `<style>`.
 *
 * These are the `.wikilink`, `.art-ref`, `.wikilink--synthesis`, `.section-badge`
 * rules that style the wiki-specific anchors. In the app they use Vue's
 * `:deep()` selector; here they are plain (un-scoped) so they apply to the
 * exported HTML.
 */
const WIKI_LINK_STYLES = `
.wikilink {
  color: rgb(79 70 229);
  text-decoration: underline;
  cursor: pointer;
  text-decoration-style: dotted;
}
.wikilink:hover { text-decoration-style: solid; }
.wikilink--synthesis {
  display: inline-block;
  background: rgb(168 85 247 / 0.12);
  color: rgb(126 34 206);
  border: 1px solid rgb(168 85 247 / 0.3);
  padding: 0.0625rem 0.375rem;
  border-radius: 0.25rem;
  font-size: 0.8em;
  font-weight: 500;
  text-decoration: none;
  cursor: pointer;
}
.wikilink--synthesis:hover { background: rgb(168 85 247 / 0.2); }
.art-ref {
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
.art-ref:hover { background: rgb(220 252 231); }
.art-ref--missing {
  color: rgb(148 163 184);
  background: rgb(241 245 249);
  border-color: rgb(226 232 240);
}
.section-badge {
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
.ref-missing {
  color: rgb(148 163 184);
  font-style: italic;
  font-size: 0.85em;
}
`;

/**
 * The full export stylesheet: the real `markdown.css` (typography) + the
 * wiki-specific link/chip styles + the page chrome (layout, nav, footer).
 */
export const STATIC_SITE_CSS = `
/* === markdown.css (the real in-app wiki typography) === */
${markdownCss}

/* === Wiki link styles (from wiki-page-viewer.vue) === */
${WIKI_LINK_STYLES}

/* === Page chrome (layout, navigation, footer) === */
:root {
  color-scheme: light;
  --color-bg: #ffffff;
  --color-surface: #f8f9fa;
  --color-text: #1b1b24;
  --color-text-muted: #64748b;
  --color-primary: #3525cd;
  --color-border: #e2e8f0;
  --color-link: #3525cd;
  --color-link-hover: #4f46e5;
  --font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  --max-width: 800px;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--color-bg);
  color: var(--color-text);
  font-family: var(--font-family);
  line-height: 1.6;
}
nav.breadcrumb {
  padding: 1rem 1.5rem;
  border-bottom: 1px solid var(--color-border);
  font-size: 0.85rem;
  color: var(--color-text-muted);
}
nav.breadcrumb a {
  color: var(--color-primary);
  text-decoration: none;
}
nav.breadcrumb a:hover { text-decoration: underline; }
article.wiki-page, article.article-stub, .index-container {
  max-width: var(--max-width);
  margin: 0 auto;
  padding: 2rem 1.5rem;
}
article.wiki-page h1, article.article-stub h1 {
  margin-top: 0;
  color: var(--color-text);
}
.article-meta { margin: 1.5rem 0; }
.meta-row { margin: 0.4rem 0; font-size: 0.95rem; }
.meta-label { font-weight: 600; color: var(--color-text-muted); margin-right: 0.5rem; }
.abstract-text { margin-top: 1rem; line-height: 1.7; }
.index-header {
  max-width: var(--max-width);
  margin: 0 auto;
  padding: 2rem 1.5rem 1rem;
}
.index-header h1 {
  margin: 0 0 0.5rem;
  font-size: 1.8rem;
  color: var(--color-text);
}
.index-header .subtitle {
  color: var(--color-text-muted);
  margin: 0 0 1.5rem;
}
.search-box {
  width: 100%;
  padding: 0.6rem 0.9rem;
  border: 1px solid var(--color-border);
  border-radius: 0.5rem;
  font-size: 0.95rem;
  background: var(--color-bg);
  color: var(--color-text);
}
.search-box:focus {
  outline: none;
  border-color: var(--color-primary);
}
.page-section {
  max-width: var(--max-width);
  margin: 0 auto;
  padding: 0 1.5rem 1.5rem;
}
.page-section h2 {
  border-bottom: 1px solid var(--color-border);
  padding-bottom: 0.4rem;
  color: var(--color-text-muted);
  text-transform: uppercase;
  font-size: 0.8rem;
  letter-spacing: 0.05em;
}
.card-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 1rem;
  margin-top: 1rem;
}
.page-card {
  display: block;
  padding: 1rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 0.5rem;
  text-decoration: none;
  color: var(--color-text);
  transition: border-color 0.15s;
}
.page-card:hover { border-color: var(--color-primary); }
.page-card h3 {
  margin: 0 0 0.4rem;
  font-size: 1rem;
  color: var(--color-primary);
}
.page-card p {
  margin: 0;
  font-size: 0.85rem;
  color: var(--color-text-muted);
}
.site-footer {
  max-width: var(--max-width);
  margin: 2rem auto;
  padding: 1rem 1.5rem;
  border-top: 1px solid var(--color-border);
  text-align: center;
  color: var(--color-text-muted);
  font-size: 0.8rem;
}
.site-footer a { color: var(--color-primary); text-decoration: none; }
.site-footer a:hover { text-decoration: underline; }
`;

/** The client-side search script (~40 lines vanilla JS). */
export const SEARCH_JS = `// Client-side search filter for the static wiki site.
(async () => {
  const cards = Array.from(document.querySelectorAll('.page-card'));
  const input = document.getElementById('search-input');
  if (!input || cards.length === 0) return;
  input.addEventListener('input', () => {
    const q = input.value.toLowerCase().trim();
    for (const card of cards) {
      const title = (card.dataset.title || '').toLowerCase();
      const summary = (card.dataset.summary || '').toLowerCase();
      card.style.display = (!q || title.includes(q) || summary.includes(q)) ? '' : 'none';
    }
  });
})();
`;

const ENT_AMP = '&' + 'amp;';
const ENT_LT = '&' + 'lt;';
const ENT_GT = '&' + 'gt;';
const ENT_QUOT = '&' + 'quot;';

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, ENT_AMP)
    .replace(/</g, ENT_LT)
    .replace(/>/g, ENT_GT)
    .replace(/"/g, ENT_QUOT);
}

/** Slugify a project title into a safe filename component. */
export function slugifyFilename(title: string): string {
  return (
    title
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 60) || 'wiki'
  );
}

/**
 * Build the page-depth for a given page type.
 * Index = 0 (root-relative: "pages/...").
 * Subpages = 2 (two levels up: "../../...") because pages live at
 * `pages/{type}/{slug}.html`.
 */
export function pageDepth(pageType: string): number {
  if (pageType === 'index') return 0;
  return 2;
}

/** Build a depth-aware `slugToHref` resolver for the static export. */
export function makeSlugToHref(
  pages: readonly WikiPageSummary[],
  currentDepth: number
): (slug: string) => string | null {
  const prefix = '../'.repeat(currentDepth);
  const slugToPage = new Map(pages.map((p) => [p.slug, p]));
  return (slug: string): string | null => {
    const page = slugToPage.get(slug);
    if (!page) return null;
    return `${prefix}pages/${page.pageType}/${encodeURIComponent(page.slug)}.html`;
  };
}

/** Build a depth-aware `artIdToHref` resolver for the static export. */
export function makeArtIdToHref(
  sources: readonly WikiSourceInfo[],
  pages: readonly WikiPageSummary[],
  currentDepth: number
): (uuid: string) => string | null {
  const prefix = '../'.repeat(currentDepth);
  const synthesisSlugs = new Set(
    pages.filter((p) => p.pageType === 'synthesis').map((p) => p.slug)
  );
  const sourceIds = new Set(sources.map((s) => s.id));
  return (uuid: string): string | null => {
    if (synthesisSlugs.has(uuid)) {
      return `${prefix}pages/synthesis/${encodeURIComponent(uuid)}.html`;
    }
    if (sourceIds.has(uuid)) {
      return `${prefix}pages/articles/${encodeURIComponent(uuid)}.html`;
    }
    return null;
  };
}

/** Check whether a synthesis page exists for a given article UUID. */
export function hasSynthesisPage(uuid: string, pages: readonly WikiPageSummary[]): boolean {
  return pages.some((p) => p.pageType === 'synthesis' && p.slug === uuid);
}

/** Build a `pageTitles` map (slug -> title) from the page list. */
export function buildPageTitles(pages: readonly WikiPageSummary[]): Map<string, string> {
  return new Map(pages.map((p) => [p.slug, p.title]));
}

/** Build a `sources` map (id -> WikiSourceInfo) for the renderer. */
export function buildSourcesMap(sources: readonly WikiSourceInfo[]): Map<string, WikiSourceInfo> {
  return new Map(sources.map((s) => [s.id, s]));
}

interface SearchIndexEntry {
  readonly slug: string;
  readonly title: string;
  readonly type: string;
  readonly summary: string;
  readonly bodyExcerpt: string;
}

/** Build the search index: one entry per page with a body excerpt. */
export function buildSearchIndex(
  pages: readonly WikiPageSummary[],
  pageBodies: Map<string, string>
): SearchIndexEntry[] {
  return pages.map((page) => {
    const body = pageBodies.get(page.slug) ?? '';
    const plain = body
      .replace(/[#*`_~[\]()>|-]/g, '')
      .replace(/\s+/g, ' ')
      .trim();
    return {
      slug: page.slug,
      title: page.title,
      type: page.pageType,
      summary: page.summary,
      bodyExcerpt: plain.slice(0, 200),
    };
  });
}

/**
 * Wrap rendered HTML body in a full HTML document.
 *
 * The body is wrapped in `<div class="markdown-content">` so the markdown CSS
 * selectors apply correctly — matching the in-app wiki viewer's container.
 */
export function wrapPageHtml(
  title: string,
  bodyHtml: string,
  pageType: string,
  _slug: string,
  depth: number
): string {
  const prefix = '../'.repeat(depth);
  const breadcrumb =
    pageType === 'index'
      ? ''
      : `<nav class="breadcrumb"><a href="${prefix}index.html">Wiki Home</a></nav>`;
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${escapeHtml(title)}</title>
  <link rel="stylesheet" href="${prefix}style.css">
</head>
<body>
${breadcrumb}
  <article class="wiki-page page-type--${escapeHtml(pageType)}">
    <div class="markdown-content">
      ${bodyHtml}
    </div>
  </article>
  <footer class="site-footer">
    <small>Generated with <a href="https://github.com/Bilal-S/Bango" target="_blank" rel="noopener">Bango</a></small>
  </footer>
</body>
</html>`;
}

/** Render an article-stub HTML page (metadata + abstract only, no full text). */
export function renderArticleStub(
  source: WikiSourceInfo,
  _authorSlugs: readonly AuthorSlugMap[]
): string {
  const year = source.year ? String(source.year) : 'Unknown';
  const authors = source.authors.length > 0 ? source.authors.join(', ') : 'Unknown';
  const journal = source.journal ? `<em>${escapeHtml(source.journal)}</em>` : 'N/A';
  const doiRow = source.doi
    ? `<div class="meta-row"><span class="meta-label">DOI:</span> <a href="https://doi.org/${escapeHtml(source.doi)}" target="_blank" rel="noopener">${escapeHtml(source.doi)}</a></div>`
    : '';
  const abstract = source.abstractText || 'No abstract available.';
  const firstAuthor = source.authors[0];
  const titleSuffix = firstAuthor ? ` - ${escapeHtml(firstAuthor)} (${year})` : '';

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${escapeHtml(source.title)}${titleSuffix}</title>
  <link rel="stylesheet" href="../../style.css">
</head>
<body>
  <nav class="breadcrumb">
    <a href="../../index.html">Wiki Home</a> &rsaquo;
    <span>Articles</span>
  </nav>
  <article class="article-stub">
    <h1>${escapeHtml(source.title)}</h1>
    <div class="article-meta">
      <div class="meta-row"><span class="meta-label">Authors:</span> ${escapeHtml(authors)}</div>
      <div class="meta-row"><span class="meta-label">Year:</span> ${escapeHtml(year)}</div>
      <div class="meta-row"><span class="meta-label">Journal:</span> ${journal}</div>
      ${doiRow}
    </div>
    <h2>Abstract</h2>
    <div class="markdown-content abstract-text">${escapeHtml(abstract)}</div>
  </article>
  <footer class="site-footer">
    <small>Generated with <a href="https://github.com/Bilal-S/Bango" target="_blank" rel="noopener">Bango</a></small>
  </footer>
</body>
</html>`;
}

/** Render the index (landing) page: categorized nav + search box. */
export function renderIndexHtml(ctx: ExportContext, projectTitle: string): string {
  const grouped = new Map<string, WikiPageSummary[]>();
  for (const page of ctx.pages) {
    const list = grouped.get(page.pageType) ?? [];
    list.push(page);
    grouped.set(page.pageType, list);
  }

  const typeOrder = ['synthesis', 'concept', 'method', 'source', 'author'];
  const typeLabels: Record<string, string> = {
    concept: 'Concepts',
    author: 'Authors',
    synthesis: 'Synthesis',
    method: 'Methods',
    source: 'Sources',
  };

  const seenTypes = new Set<string>();
  const knownSections = typeOrder
    .filter((t) => grouped.has(t))
    .map((type) => {
      seenTypes.add(type);
      const pages = grouped.get(type)!;
      const label = typeLabels[type] ?? type;
      const cards = pages
        .map(
          (page) =>
            `<a class="page-card" href="pages/${encodeURIComponent(type)}/${encodeURIComponent(page.slug)}.html" data-title="${escapeHtml(page.title)}" data-summary="${escapeHtml(page.summary)}" data-slug="${escapeHtml(page.slug)}"><h3>${escapeHtml(page.title)}</h3><p>${escapeHtml(page.summary) || 'No summary.'}</p></a>`
        )
        .join('');
      return `<section class="page-section"><h2>${escapeHtml(label)} (${pages.length})</h2><div class="card-grid">${cards}</div></section>`;
    })
    .join('');

  const otherTypes = [...grouped.keys()].filter((t) => !seenTypes.has(t));
  const otherSection =
    otherTypes.length > 0
      ? otherTypes
          .map((type) => {
            const pages = grouped.get(type)!;
            const label = typeLabels[type] ?? type;
            const cards = pages
              .map(
                (page) =>
                  `<a class="page-card" href="pages/${encodeURIComponent(type)}/${encodeURIComponent(page.slug)}.html" data-title="${escapeHtml(page.title)}" data-summary="${escapeHtml(page.summary)}" data-slug="${escapeHtml(page.slug)}"><h3>${escapeHtml(page.title)}</h3><p>${escapeHtml(page.summary) || 'No summary.'}</p></a>`
              )
              .join('');
            return `<section class="page-section"><h2>${escapeHtml(label)} (${pages.length})</h2><div class="card-grid">${cards}</div></section>`;
          })
          .join('')
      : '';

  const sections = knownSections + otherSection;
  const subtitle = `${ctx.pages.length} pages from ${ctx.sources.length} articles`;

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${escapeHtml(projectTitle)}</title>
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <div class="index-header">
    <h1>${escapeHtml(projectTitle)}</h1>
    <p class="subtitle">${escapeHtml(subtitle)}</p>
    <input type="text" id="search-input" class="search-box" placeholder="Search pages..." />
  </div>
  ${sections}
  <footer class="site-footer">
    <small>Generated with <a href="https://github.com/Bilal-S/Bango" target="_blank" rel="noopener">Bango</a></small>
  </footer>
  <script src="search.js"></script>
</body>
</html>`;
}

/** Gather all export context from the backend. */
export async function gatherExportContext(): Promise<ExportContext> {
  const { useWiki } = await import('@/composables/use-wiki');
  const wiki = useWiki();

  const [pages, sources] = await Promise.all([wiki.listPages(), wiki.listSources()]);

  const pageBodyEntries = await Promise.all(
    pages.map(async (page) => {
      const full = await wiki.getPage(page.slug);
      return [page.slug, full?.body ?? ''] as const;
    })
  );
  const pageBodies = new Map(pageBodyEntries);

  const authorSlugs: AuthorSlugMap[] = [];

  return { pages, pageBodies, sources, authorSlugs };
}

/**
 * Build the complete list of export files (pure function — no IPC calls).
 *
 * Renders each wiki page to HTML with depth-aware resolvers + per-page
 * `linkArtRefsToSynthesis` (matching the in-app viewer), generates article
 * stubs, and builds the index + style + search.
 */
export function buildExportFiles(ctx: ExportContext, projectTitle: string): ExportFile[] {
  const files: ExportFile[] = [];
  const sourcesMap = buildSourcesMap(ctx.sources);
  const pageTitles = buildPageTitles(ctx.pages);

  for (const page of ctx.pages) {
    const depth = pageDepth(page.pageType);
    const slugToHref = makeSlugToHref(ctx.pages, depth);
    const artIdToHref = makeArtIdToHref(ctx.sources, ctx.pages, depth);
    const body = ctx.pageBodies.get(page.slug) ?? '';
    const html = renderWikiMarkdown(body, {
      staticMode: true,
      slugToHref,
      artIdToHref,
      sources: sourcesMap,
      pageTitles,
      linkArtRefsToSynthesis: page.pageType === 'author',
    });
    files.push({
      path: `pages/${page.pageType}/${page.slug}.html`,
      content: wrapPageHtml(page.title, html, page.pageType, page.slug, depth),
    });
  }

  for (const source of ctx.sources) {
    if (!hasSynthesisPage(source.id, ctx.pages)) {
      files.push({
        path: `pages/articles/${source.id}.html`,
        content: renderArticleStub(source, ctx.authorSlugs),
      });
    }
  }

  files.push({ path: 'index.html', content: renderIndexHtml(ctx, projectTitle) });
  files.push({ path: 'style.css', content: STATIC_SITE_CSS });
  files.push({
    path: 'search-index.json',
    content: JSON.stringify(buildSearchIndex(ctx.pages, ctx.pageBodies)),
  });
  files.push({ path: 'search.js', content: SEARCH_JS });

  return files;
}

/**
 * Step 1: Generate the wiki static site to `wiki-root/wiki-export/`.
 *
 * @returns The `GenerateExportResult` with the export directory + index path.
 */
export async function generateWikiExport(projectTitle: string): Promise<GenerateExportResult> {
  const ctx = await gatherExportContext();
  const files = buildExportFiles(ctx, projectTitle);
  const bundle: SiteExportBundle = { files, projectTitle };
  return tauriCommand<GenerateExportResult>('wiki_generate_export', { bundle });
}

/**
 * Step 2: Zip the `wiki-export/` directory into a user-chosen `.zip` file.
 *
 * @returns The destination file path, or `null` when the user cancels.
 */
export async function zipWikiExport(projectTitle: string): Promise<string | null> {
  const filenameSlug = slugifyFilename(projectTitle).slice(0, 20) || 'wiki';
  const filePath = await save({
    defaultPath: `bango-wiki-${filenameSlug}.zip`,
    filters: [{ name: 'Zip', extensions: ['zip'] }],
  });
  if (!filePath) return null;
  return tauriCommand<string>('wiki_zip_export', { path: filePath });
}
