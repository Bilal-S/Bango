/**
 * Shared wiki Markdown renderer.
 *
 * Converts the two wiki-specific conventions into clickable spans before
 * running the text through `marked`:
 * 1. `[^art-{id}]` footnotes -> article source references (`.art-ref`).
 * 2. `[[slug]]` and `[[slug|alias]]` -> wiki page links (`.wikilink`).
 *
 * Also strips `/raw/...md` artifact lines that the LLM sometimes emits.
 *
 * Used by both `wiki-page-viewer.vue` (with a populated sources map) and the
 * chat bubbles in `chat-view.vue` (sources optional). Keeping the transform in
 * one place guarantees identical click targets and styling hooks.
 */
import { marked } from 'marked';
import type { WikiSourceInfo } from '@/types/wiki';

export interface RenderWikiMarkdownOptions {
  /** Source metadata map for `[^art-id]` resolution. If absent, refs render as
   * short IDs with the `art-ref--missing` class. */
  sources?: Map<string, WikiSourceInfo>;
  /** Wiki page slug-to-title map. When a bare UUID matches a wiki page slug,
   * the renderer emits a synthesis-styled chip with the page's title instead
   * of showing the raw UUID. Takes priority over `sources` for bare UUIDs
   * because a bare UUID in wiki prose is most likely a cross-reference to
   * another wiki page. */
  pageTitles?: Map<string, string>;
}

/**
 * HTML entity strings. These are assembled at runtime (split across two
 * literals) so the raw entity sequence never appears in source - some editors
 * / tooling decode literal entities and would silently turn the escapers into
 * no-ops.
 */
const ENT_AMP = '&' + 'amp;';
const ENT_LT = '&' + 'lt;';
const ENT_GT = '&' + 'gt;';
const ENT_QUOT = '&' + 'quot;';

/** Format an article reference label: "Title (Year)". Truncates long titles. */
export function formatArtRefLabel(source: WikiSourceInfo): string {
  const year = source.year ? ` (${source.year})` : '';
  const title = source.title.length > 60 ? source.title.slice(0, 57) + '...' : source.title;
  return `${title}${year}`;
}

/** Escape a string for safe embedding in an HTML attribute value. */
function escapeAttr(value: string): string {
  return value
    .replace(/&/g, ENT_AMP)
    .replace(/"/g, ENT_QUOT)
    .replace(/</g, ENT_LT)
    .replace(/>/g, ENT_GT);
}

/** Escape a string for safe embedding as visible HTML text. */
function escapeText(value: string): string {
  return value.replace(/&/g, ENT_AMP).replace(/</g, ENT_LT).replace(/>/g, ENT_GT);
}

/**
 * Render wiki Markdown to an HTML string.
 *
 * The returned HTML contains `.wikilink` and `.art-ref` anchors with
 * `data-slug` / `data-art-id` attributes. Callers attach a single delegated
 * click handler that reads those attributes (see `wiki-page-viewer.vue` and
 * `chat-view.vue`).
 */
export function renderWikiMarkdown(text: string, opts?: RenderWikiMarkdownOptions): string {
  if (!text) return '';
  const sources = opts?.sources;
  const pageTitles = opts?.pageTitles;
  let out = text;

  // 0. Bare UUID -> synthesis wikilink chip, [[uuid|Title]], or [[uuid]].
  //    The LLM sometimes emits bare article UUIDs in prose (e.g. "see changes
  //    f399a079-...-43 and obesity 6d2ec462-...-f1") without the `[[...]]`
  //    bracket syntax. This pass detects those bare UUIDs and converts them:
  //    - If the UUID matches a wiki page slug (pageTitles), emit a
  //      synthesis-styled chip with the page's human-readable title (a bare
  //      UUID in wiki prose is most likely a cross-reference to another wiki
  //      page, not a redundant article link).
  //    - Else if source metadata is available, emit [[uuid|Title (Year)]] so
  //      step 2 produces a regular wikilink with a human-readable alias.
  //    - Else emit [[uuid]] (clickable, visible UUID).
  //    The lookbehinds exclude UUIDs that are already inside `[[...]]`
  //    (preceded by `[` or `|`) or inside `[^art-...]` (preceded by `art-`).
  out = out.replace(
    /(?<![[|])(?<!art-)\b([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\b/gi,
    (_match: string, uuid: string) => {
      // Priority 1: wiki page title -> synthesis chip.
      const pageTitle = pageTitles?.get(uuid);
      if (pageTitle) {
        const label = escapeText(pageTitle);
        const safeSlug = escapeAttr(uuid);
        return `<a class="wikilink wikilink--synthesis" data-slug="${safeSlug}">${label}</a>`;
      }
      // Priority 2: article source metadata -> wikilink with alias.
      const source = sources?.get(uuid);
      if (source) {
        return `[[${uuid}|${formatArtRefLabel(source)}]]`;
      }
      // Priority 3: plain wikilink (clickable, visible UUID).
      return `[[${uuid}]]`;
    }
  );

  // 0.5. [^art-{uuid}]: definition lines -> synthesis-colored wikilinks.
  //      The LLM emits footnote definitions at the bottom of each page like
  //      `[^art-uuid]: citation text`. The inline ref (step 1) already
  //      produces the article link, so the definition is redundant for
  //      article access — but the UUID also names a synthesis wiki page,
  //      which is NOT redundant. Convert each definition line into a
  //      synthesis-styled wikilink chip that opens the wiki page, using
  //      the source title as the visible label when available. Runs before
  //      step 1 so the `[^art-uuid]` prefix is consumed here (not matched
  //      again as an inline ref).
  out = out.replace(/^\[\^art-([a-f0-9-]+)\]:[^\n]*$/gim, (_m, artId: string) => {
    const source = sources?.get(artId);
    const label = source ? escapeText(formatArtRefLabel(source)) : escapeText(artId);
    return `<a class="wikilink wikilink--synthesis" data-slug="${escapeAttr(artId)}">${label}</a>`;
  });

  // 1. [^art-{id}] footnotes -> clickable source references.
  out = out.replace(/\[\^art-([a-f0-9-]+)\]/g, (_match, artId: string) => {
    const source = sources?.get(artId);
    if (source) {
      const label = escapeText(formatArtRefLabel(source));
      const titleAttr = escapeAttr(source.title);
      return `<a class="art-ref" data-art-id="${artId}" title="${titleAttr}">${label}</a>`;
    }
    const shortId = artId.slice(0, 8);
    return `<a class="art-ref art-ref--missing" data-art-id="${artId}">[${shortId}]</a>`;
  });

  // 2. [[slug]] and [[slug|alias]] -> wikilinks.
  //    The `data-slug` is normalized to lowercase so that Title-Cased links
  //    like `[[Sugar-Reduction]]` resolve to the real page (slug
  //    `sugar-reduction`) when the consumer looks it up by exact slug. The
  //    visible link text preserves the original casing/alias.
  out = out.replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_match, slug: string, alias?: string) => {
    const linkText = alias?.trim() || slug.trim();
    const safeSlug = escapeAttr(slug.trim().toLowerCase());
    return `<a class="wikilink" data-slug="${safeSlug}">${escapeText(linkText)}</a>`;
  });

  // 3. Strip lines containing /raw/ file paths (LLM artifact, not user-facing).
  out = out.replace(/^.*\/raw\/[^\s)]+\.md.*$/gim, '');

  return marked.parse(out) as string;
}
