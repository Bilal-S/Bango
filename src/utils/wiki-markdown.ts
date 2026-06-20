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
  let out = text;

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
