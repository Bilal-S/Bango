/**
 * Shared wiki Markdown renderer.
 *
 * Converts the two wiki-specific conventions into clickable spans before
 * running the text through `marked`:
 * 1. `[^art-{id}]` footnotes -> article source references (`.art-ref`).
 * 2. `[[slug]]` and `[[slug|alias]]` -> wiki page links (`.wikilink`).
 *
 * Also detects bare article UUIDs in prose and converts them to clickable
 * `.art-ref` (article detail) or `.wikilink--synthesis` (wiki reader) anchors
 * depending on which metadata map they match.
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
  /** Source metadata map for `[^art-id]` resolution and bare-UUID article
   * links. If absent, refs render as short IDs with the `art-ref--missing`
   * class. When a bare UUID matches a `sources` entry, the renderer emits a
   * green `.art-ref` anchor (article detail) instead of a wiki link. */
  sources?: Map<string, WikiSourceInfo>;
  /** Wiki page slug-to-title map. When a bare UUID matches a wiki page slug,
   * the renderer emits a synthesis-styled chip with the page's title instead
   * of showing the raw UUID. */
  pageTitles?: Map<string, string>;
  /** Bare-UUID resolution priority for the chat view. When `true`, `sources`
   * (articles) is checked before `pageTitles` (wiki pages) so a UUID that
   * exists in both renders as a green `.art-ref` (article detail) rather than
   * a pink `.wikilink--synthesis` (wiki reader). Defaults to `false`, which
   * keeps the wiki-page-viewer behavior where wiki pages win for bare UUIDs. */
  articlePriority?: boolean;
  /** When `true`, `[^art-{uuid}]` refs render as a pink `.wikilink--synthesis`
   * chip that opens the wiki synthesis page (slug = uuid) instead of a green
   * `.art-ref` that opens the article detail. Used by the author-page viewer so
   * each publication links to its synthesis page (which itself links to the
   * source). Falls back to a green `.art-ref` when no synthesis page exists in
   * `pageTitles` for the uuid. Defaults to `false` (green art-refs everywhere). */
  linkArtRefsToSynthesis?: boolean;
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

/** Build a green `.art-ref` anchor for an article UUID with its source label. */
function makeArtRef(uuid: string, source: WikiSourceInfo): string {
  const label = escapeText(formatArtRefLabel(source));
  const titleAttr = escapeAttr(source.title);
  return `<a class="art-ref" data-art-id="${uuid}" title="${titleAttr}">${label}</a>`;
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
  const articlePriority = opts?.articlePriority === true;
  let out = text;

  // 0. Bare UUID -> green art-ref, pink synthesis chip, or indigo wikilink.
  //    The LLM sometimes emits bare article UUIDs in prose (e.g. "see changes
  //    f399a079-...-43 and obesity 6d2ec462-...-f1") without the `[[...]]`
  //    bracket syntax. This pass detects those bare UUIDs and converts them:
  //    - If `articlePriority` is set (chat view) and the UUID matches a source
  //      article, emit a green `.art-ref` (opens article detail).
  //    - Else if the UUID matches a wiki page slug (pageTitles), emit a pink
  //      synthesis-styled chip (opens wiki reader).
  //    - Else if source metadata is available, emit a green `.art-ref` (opens
  //      article detail). This branch replaced the former `[[uuid|Title]]`
  //      emission so article UUIDs render as article links, not wiki links.
  //    - Else emit [[uuid]] (indigo wikilink, clickable, visible UUID).
  //    The lookbehinds exclude UUIDs that are already inside `[[...]]`
  //    (preceded by `[` or `|`), inside `[^art-...]` (preceded by `art-`), or
  //    inside `[^uuid]` footnote refs (preceded by `^`). The `^` exclusion is
  //    critical: without it, step 0 would convert the bare UUID inside
  //    `[^uuid]` into `[[uuid]]` before step 1 can resolve the whole `[^uuid]`
  //    construct, causing step 4 to strip it and lose the UUID entirely.
  out = out.replace(
    /(?<![[|])(?<!\^)(?<!art-)\b([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\b/gi,
    (_match: string, uuid: string) => {
      const source = sources?.get(uuid);
      const pageTitle = pageTitles?.get(uuid);
      // Chat view: articles take priority over wiki pages.
      if (articlePriority && source) {
        return makeArtRef(uuid, source);
      }
      // Default / wiki viewer: wiki page title -> synthesis chip (pink).
      if (pageTitle) {
        const label = escapeText(pageTitle);
        const safeSlug = escapeAttr(uuid);
        return `<a class="wikilink wikilink--synthesis" data-slug="${safeSlug}">${label}</a>`;
      }
      // Source metadata -> green art-ref (article detail).
      if (source) {
        return makeArtRef(uuid, source);
      }
      // Fallback: plain wikilink (indigo, clickable, visible UUID).
      return `[[${uuid}]]`;
    }
  );

  // 0.5. [^{uuid}]: and [^art-{uuid}]: definition lines -> synthesis chips.
  //      The LLM emits footnote definitions at the bottom of each page like
  //      `[^art-uuid]: citation text` (sometimes `[^uuid]:` without the
  //      `art-` prefix). The inline ref (step 1) already produces the article
  //      link, so the definition is redundant for article access — but the UUID
  //      also names a synthesis wiki page, which is NOT redundant. Convert each
  //      definition line into a synthesis-styled wikilink chip that opens the
  //      wiki page, using the source title as the visible label when available.
  //      Runs before step 1 so the `[^...]` prefix is consumed here (not
  //      matched again as an inline ref). Case-insensitive so `[^ART-uuid]:`
  //      is also caught.
  out = out.replace(/^\[\^(?:art-)?([a-f0-9-]+)\]:[^\n]*$/gim, (_m, artId: string) => {
    const source = sources?.get(artId);
    const label = source ? escapeText(formatArtRefLabel(source)) : escapeText(artId);
    return `<a class="wikilink wikilink--synthesis" data-slug="${escapeAttr(artId)}">${label}</a>`;
  });

  // 1. [^art-{uuid}] and [^{uuid}] footnotes -> clickable source references,
  //    OR synthesis chips. Accepts both the `[^art-uuid]` (canonical) and
  //    `[^uuid]` (LLM variant, no prefix) forms, case-insensitively
  //    (`[^ART-uuid]` also matches). When `linkArtRefsToSynthesis` is set
  //    (author-page viewer) and a wiki synthesis page exists for the uuid
  //    (pageTitles), emit a pink synthesis chip so the ref opens the synthesis
  //    page. Otherwise emit the default green `.art-ref` (article detail).
  //    Falls through to the missing-ref span when no source metadata is
  //    available either. Runs before step 4 so these constructs are resolved
  //    rather than stripped as dangling refs.
  const linkToSynthesis = opts?.linkArtRefsToSynthesis === true;
  out = out.replace(/\[\^(?:art-)?([a-f0-9-]+)\]/gi, (_match, artId: string) => {
    const source = sources?.get(artId);
    const pageTitle = pageTitles?.get(artId);
    if (linkToSynthesis && pageTitle) {
      const label = escapeText(pageTitle);
      const safeSlug = escapeAttr(artId);
      return `<a class="wikilink wikilink--synthesis" data-slug="${safeSlug}">${label}</a>`;
    }
    if (source) {
      return makeArtRef(artId, source);
    }
    const shortId = artId.slice(0, 8);
    return `<a class="art-ref art-ref--missing" data-art-id="${artId}">[${shortId}]</a>`;
  });

  // 2. [[slug]] and [[slug|alias]] -> wikilinks.
  //    The `data-slug` is normalized to lowercase so that Title-Cased links
  //    like `[[Sugar-Reduction]]` resolve to the real page (slug
  //    `sugar-reduction`) when the consumer looks it up by exact slug. The
  //    visible link text preserves the original casing/alias.
  //
  //    UUID-shaped slugs (`[[{uuid}]]`) get special treatment: the LLM often
  //    emits article UUIDs inside wikilink brackets, and showing the raw UUID
  //    as link text is unreadable. When the slug is a UUID, resolve it against
  //    `pageTitles` (synthesis chip) or `sources` (green art-ref) to produce a
  //    human-readable label — mirroring the bare-UUID resolution in step 0.
  //    If the UUID matches neither map, it falls through to the default
  //    wikilink (indigo, visible UUID) so the user can still click it.
  const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  out = out.replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_match, slug: string, alias?: string) => {
    const trimmedSlug = slug.trim();
    // UUID-shaped slug: resolve to a synthesis chip or art-ref with a label.
    if (UUID_RE.test(trimmedSlug) && !alias) {
      const uuid = trimmedSlug;
      const pageTitle = pageTitles?.get(uuid);
      const source = sources?.get(uuid);
      // Default / wiki viewer: pageTitles wins (pink synthesis chip).
      if (!articlePriority && pageTitle) {
        return `<a class="wikilink wikilink--synthesis" data-slug="${escapeAttr(uuid)}">${escapeText(pageTitle)}</a>`;
      }
      // Chat view with articlePriority: sources win (green art-ref).
      if (source) {
        return makeArtRef(uuid, source);
      }
      // pageTitles fallback (when articlePriority is true but no source).
      if (pageTitle) {
        return `<a class="wikilink wikilink--synthesis" data-slug="${escapeAttr(uuid)}">${escapeText(pageTitle)}</a>`;
      }
      // No metadata: default wikilink with the raw UUID as text (still clickable).
    }
    const linkText = alias?.trim() || trimmedSlug;
    const safeSlug = escapeAttr(trimmedSlug.toLowerCase());
    return `<a class="wikilink" data-slug="${safeSlug}">${escapeText(linkText)}</a>`;
  });

  // 3. Strip lines containing /raw/ file paths (artifact, not user-facing).
  //    The path may contain spaces when an older pre-seeder used the article
  //    title as the filename (e.g. `/raw/Impact of the UK....md`), so the char
  //    class allows anything but a newline or `)` (which closes Markdown
  //    links). UUID-based paths (no spaces) still match.
  out = out.replace(/^.*\/raw\/[^\n)]+\.md.*$/gim, '');

  // 4. Collapse dangling footnote refs that don't start with `art-` and aren't
  //    UUID-shaped. The renderer resolves `[^art-{uuid}]` and `[^{uuid}]` in
  //    step 1; step 0 already converted any bare UUIDs inside `[^...]`. Other
  //    forms - `[^<title>]` from an older pre-seeder, or `[^1]` numeric
  //    Markdown footnotes whose definition was stripped above - would
  //    otherwise render as literal `[^...]` text. Drop the bracketed marker,
  //    leaving the surrounding prose clean. The second lookahead
  //    `(?![0-9a-f]{8}-)` is a safety net so a UUID-shaped ref that somehow
  //    escaped steps 0/1 is NOT stripped (it would leave a visible UUID that
  //    the user can still read). Runs last so resolved refs above are
  //    already converted and only truly dangling refs remain.
  out = out.replace(/\[\^(?!art-)(?![0-9a-f-])[^\]\n]+\]/gi, '');

  return marked.parse(out) as string;
}
