import { describe, it, expect } from 'vitest';
import { renderWikiMarkdown, formatArtRefLabel } from '@/utils/wiki-markdown';
import type { WikiSourceInfo } from '@/types/wiki';

function src(id: string, title: string, year: number | null): WikiSourceInfo {
  return { id, title, authors: [], year, doi: null };
}

describe('renderWikiMarkdown', () => {
  it('returns empty string for empty input', () => {
    expect(renderWikiMarkdown('')).toBe('');
  });

  it('passes plain markdown through marked', () => {
    const out = renderWikiMarkdown('# Heading\n\nSome **bold** text.');
    expect(out).toContain('<h1>Heading</h1>');
    expect(out).toContain('<strong>bold</strong>');
  });

  it('converts [[slug]] to a wikilink anchor with data-slug', () => {
    const out = renderWikiMarkdown('See [[sugar-tax]] for details.');
    expect(out).toContain('class="wikilink"');
    expect(out).toContain('data-slug="sugar-tax"');
    expect(out).toContain('>sugar-tax<');
  });

  it('uses the alias text when [[slug|alias]] is given but keeps data-slug', () => {
    const out = renderWikiMarkdown('See [[sugar-tax|the levy]] for details.');
    expect(out).toContain('data-slug="sugar-tax"');
    expect(out).toContain('>the levy<');
    expect(out).not.toContain('>sugar-tax<');
  });

  it('resolves [^art-id] when the source map contains the id', () => {
    const sources = new Map([['abc123', src('abc123', 'A Study', 2024)]]);
    const out = renderWikiMarkdown('Claim [^art-abc123].', { sources });
    expect(out).toContain('class="art-ref"');
    expect(out).toContain('data-art-id="abc123"');
    expect(out).toContain('A Study (2024)');
    expect(out).not.toContain('art-ref--missing');
  });

  it('renders a shortened id for unresolved [^art-id]', () => {
    const out = renderWikiMarkdown('Claim [^art-deadbeef-1234].');
    expect(out).toContain('art-ref--missing');
    expect(out).toContain('data-art-id="deadbeef-1234"');
    expect(out).toContain('[deadbeef]');
  });

  it('strips lines containing /raw/ markdown paths', () => {
    const text = 'Intro\n\nSource: /home/user/wiki-root/raw/abc.md\n\nRest';
    const out = renderWikiMarkdown(text);
    expect(out).not.toContain('/raw/abc.md');
    expect(out).toContain('Intro');
    expect(out).toContain('Rest');
  });

  it('strips /raw/ paths that contain spaces (title-based filenames)', () => {
    // Regression: an older pre-seeder used the article title as the raw
    // filename, producing paths like `/raw/Impact of the UK....md`. The strip
    // regex must not bail out at the first space.
    const text =
      'Publications\n\n[^Impact of the UK soft drinks levy]: /raw/Impact of the UK soft drinks levy.md\n\nRest';
    const out = renderWikiMarkdown(text);
    expect(out).not.toContain('/raw/');
    expect(out).not.toContain('Impact of the UK soft drinks levy.md');
    expect(out).toContain('Publications');
    expect(out).toContain('Rest');
  });

  it('collapses dangling [^<title>] refs that do not start with art-', () => {
    // Regression: an older pre-seeder emitted inline refs as `[^<title>]`
    // (no `art-` prefix). The renderer should drop the bracketed marker
    // instead of leaking literal `[^...]` text into the rendered output.
    const out = renderWikiMarkdown('See "Title One" [^Title One] and "Title Two" [^Title Two].');
    expect(out).not.toContain('[^');
    expect(out).toContain('Title One');
    expect(out).toContain('Title Two');
  });

  it('leaves [^art-{uuid}] refs intact (handled in step 1, not step 4)', () => {
    const sources = new Map([['abc123', src('abc123', 'A Study', 2024)]]);
    const out = renderWikiMarkdown('Claim [^art-abc123].', { sources });
    expect(out).toContain('class="art-ref"');
    expect(out).toContain('data-art-id="abc123"');
  });

  it('linkArtRefsToSynthesis converts [^art-uuid] to a synthesis chip when a page exists', () => {
    // Author-page use case: the publication ref should open the wiki synthesis
    // page (slug = uuid), not the article detail.
    const uuid = 'f764b86c-1516-5c8e-9997-88c61c50a683';
    const sources = new Map([[uuid, src(uuid, 'Impact of the UK soft drinks levy', 2024)]]);
    const pageTitles = new Map([[uuid, 'UK Soft Drinks Levy - Health Impact']]);
    const out = renderWikiMarkdown(`"Paper Title" [^art-${uuid}]`, {
      sources,
      pageTitles,
      linkArtRefsToSynthesis: true,
    });
    // Pink synthesis chip pointing at the wiki page, with the page title label.
    expect(out).toContain('wikilink--synthesis');
    expect(out).toContain(`data-slug="${uuid}"`);
    expect(out).toContain('>UK Soft Drinks Levy - Health Impact<');
    // It must NOT be a green art-ref (no article-detail link).
    expect(out).not.toContain('class="art-ref"');
    expect(out).not.toContain(`data-art-id="${uuid}"`);
  });

  it('linkArtRefsToSynthesis falls back to green art-ref when no synthesis page exists', () => {
    // Graceful degradation: when the LLM never created a synthesis page for an
    // article, the ref should still resolve to the article detail (green
    // art-ref) rather than disappearing or showing a raw id.
    const uuid = 'f764b86c-1516-5c8e-9997-88c61c50a683';
    const sources = new Map([[uuid, src(uuid, 'A Study', 2024)]]);
    // No pageTitles entry for this uuid -> no synthesis page.
    const out = renderWikiMarkdown(`Claim [^art-${uuid}].`, {
      sources,
      linkArtRefsToSynthesis: true,
    });
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${uuid}"`);
    expect(out).not.toContain('wikilink--synthesis');
  });

  it('default (no flag) keeps [^art-uuid] as a green art-ref even when a synthesis page exists', () => {
    // Non-author pages (concept, synthesis, method) and chat-view must keep
    // the default green art-ref behavior. The flag is opt-in.
    const uuid = 'f764b86c-1516-5c8e-9997-88c61c50a683';
    const sources = new Map([[uuid, src(uuid, 'A Study', 2024)]]);
    const pageTitles = new Map([[uuid, 'Synthesis Page Title']]);
    const out = renderWikiMarkdown(`Claim [^art-${uuid}].`, { sources, pageTitles });
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${uuid}"`);
    expect(out).not.toContain('wikilink--synthesis');
  });

  it('escapes HTML in slug / alias so attributes stay safe', () => {
    const out = renderWikiMarkdown('[[evil"slug|naughty"text]]');
    // The quote in the slug must be encoded in the attribute value.
    expect(out).toContain('data-slug="evil');
    expect(out).not.toMatch(/data-slug="evil"[^&]/);
  });

  it('handles multiple wikilinks and art-refs in one body', () => {
    const sources = new Map([['111', src('111', 'Paper One', 2020)]]);
    const out = renderWikiMarkdown('[[alpha]] and [^art-111] and [[beta|B]].', { sources });
    expect(out).toContain('data-slug="alpha"');
    expect(out).toContain('data-art-id="111"');
    expect(out).toContain('data-slug="beta"');
  });

  it('normalizes data-slug to lowercase for case-insensitive resolution', () => {
    // A Title-Cased link like [[Sugar-Reduction]] must emit data-slug in
    // lowercase so the consumer (which stores slugs lowercase) can resolve it.
    const out = renderWikiMarkdown('See [[Sugar-Reduction]] and [[OBESITY]].');
    expect(out).toContain('data-slug="sugar-reduction"');
    expect(out).toContain('data-slug="obesity"');
    expect(out).not.toContain('data-slug="Sugar-Reduction"');
    // Visible link text preserves original casing.
    expect(out).toContain('>Sugar-Reduction<');
    expect(out).toContain('>OBESITY<');
  });

  it('converts a bare UUID in prose to a wikilink', () => {
    const uuid = 'f399a079-dbe4-589b-84ff-057638871f43';
    const out = renderWikiMarkdown(`See changes ${uuid} for details.`);
    expect(out).toContain('class="wikilink"');
    expect(out).toContain(`data-slug="${uuid}"`);
    // Without source metadata the UUID is the visible link text, but it must
    // be wrapped inside an anchor (clickable), not bare text.
    expect(out).toContain(`>${uuid}</a>`);
  });

  it('emits a green art-ref for a bare UUID when the sources map has it', () => {
    const uuid = 'f399a079-dbe4-589b-84ff-057638871f43';
    const sources = new Map([[uuid, src(uuid, 'Sugar Levy Study', 2020)]]);
    const out = renderWikiMarkdown(`See ${uuid}.`, { sources });
    // Article UUIDs now render as green art-refs (article detail), not wiki links.
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${uuid}"`);
    expect(out).toContain('>Sugar Levy Study (2020)<');
    // It should NOT be a wikilink.
    expect(out).not.toContain('class="wikilink');
    // Raw UUID should not appear as visible text.
    expect(out).not.toContain(`>${uuid}<`);
  });

  it('articlePriority makes sources win over pageTitles for bare UUIDs (chat view)', () => {
    const uuid = '0e4822b6-b8bb-4ed0-8333-84336a07797b';
    // Both maps have the UUID. With articlePriority the article (green art-ref)
    // wins; without it the wiki page (pink synthesis chip) wins.
    const sources = new Map([[uuid, src(uuid, 'Article Title', 2022)]]);
    const pageTitles = new Map([[uuid, 'Wiki Page Title']]);
    const out = renderWikiMarkdown(`See ${uuid}.`, {
      sources,
      pageTitles,
      articlePriority: true,
    });
    // Green art-ref to the article, not a wiki chip.
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${uuid}"`);
    expect(out).toContain('>Article Title (2022)<');
    // Wiki chip / wiki page title must NOT appear.
    expect(out).not.toContain('wikilink--synthesis');
    expect(out).not.toContain('Wiki Page Title');
  });

  it('uses pageTitles for bare UUID as a synthesis chip (priority over sources)', () => {
    const uuid = '0e4822b6-b8bb-4ed0-8333-84336a07797b';
    // Both maps have the UUID; pageTitles should win and produce a chip.
    const sources = new Map([[uuid, src(uuid, 'Source Title', 2022)]]);
    const pageTitles = new Map([[uuid, 'Wiki Page Display Title']]);
    const out = renderWikiMarkdown(`Associated study: ${uuid}`, { sources, pageTitles });
    // Synthesis chip with the wiki page title, NOT the source label.
    expect(out).toContain('wikilink--synthesis');
    expect(out).toContain(`data-slug="${uuid}"`);
    expect(out).toContain('>Wiki Page Display Title<');
    // Source label should NOT appear (pageTitles takes priority).
    expect(out).not.toContain('Source Title');
  });

  it('renders bare UUID as synthesis chip when pageTitles has it (no sources)', () => {
    const uuid = '0e4822b6-b8bb-4ed0-8333-84336a07797b';
    const pageTitles = new Map([[uuid, 'Added Sugar in Australia']]);
    const out = renderWikiMarkdown(`See ${uuid}.`, { pageTitles });
    expect(out).toContain('wikilink--synthesis');
    expect(out).toContain('>Added Sugar in Australia<');
    expect(out).toContain(`data-slug="${uuid}"`);
    // Raw UUID hidden.
    expect(out).not.toContain(`>${uuid}<`);
  });

  it('does not double-transform UUID already inside [[...]]', () => {
    const uuid = 'f399a079-dbe4-589b-84ff-057638871f43';
    const out = renderWikiMarkdown(`See [[${uuid}]].`);
    // Should produce exactly one wikilink anchor, not nested ones.
    const linkCount = (out.match(/class="wikilink"/g) || []).length;
    expect(linkCount).toBe(1);
    expect(out).toContain(`data-slug="${uuid}"`);
  });

  it('does not transform UUID inside [^art-...]', () => {
    const uuid = 'f399a079-dbe4-589b-84ff-057638871f43';
    const out = renderWikiMarkdown(`Claim [^art-${uuid}].`);
    // Should be an art-ref anchor, not a wikilink.
    expect(out).toContain('class="art-ref');
    expect(out).not.toContain('class="wikilink"');
  });

  it('handles multiple bare UUIDs in one line (author profile pattern)', () => {
    const uuids = [
      'f399a079-dbe4-589b-84ff-057638871f43',
      '6d2ec462-d1c7-57a0-acdb-7f8fd694fdf1',
      'b1e146ea-477a-5f1d-83b7-d9331ec28e83',
      'f764b86c-1516-5c8e-9997-88c61c50a683',
    ];
    const out = renderWikiMarkdown(
      `Author on changes ${uuids[0]}, obesity ${uuids[1]}, purchases ${uuids[2]}, modelling ${uuids[3]}.`
    );
    for (const uuid of uuids) {
      expect(out).toContain(`data-slug="${uuid}"`);
    }
    const linkCount = (out.match(/class="wikilink"/g) || []).length;
    expect(linkCount).toBe(4);
  });

  it('converts [^art-uuid]: definition lines into synthesis wikilinks', () => {
    const uuid = 'f399a079-dbe4-589b-84ff-057638871f43';
    const sources = new Map([[uuid, src(uuid, 'Anticipatory changes', 2020)]]);
    const md = `Some prose [^art-${uuid}].

[^art-${uuid}]: Anticipatory changes in British household purchases (Rogers et al., 2020).`;
    const out = renderWikiMarkdown(md, { sources });
    // The definition line is replaced by a synthesis-styled wikilink chip.
    expect(out).toContain('wikilink--synthesis');
    expect(out).toContain(`data-slug="${uuid}"`);
    expect(out).toContain('>Anticipatory changes (2020)<');
    // The definition's citation text is gone (replaced by the chip).
    expect(out).not.toContain('Rogers et al.');
    expect(out).not.toContain(']:');
  });

  it('preserves inline [^art-uuid] refs alongside definition lines', () => {
    const uuid = 'f399a079-dbe4-589b-84ff-057638871f43';
    const sources = new Map([[uuid, src(uuid, 'Study Title', 2020)]]);
    const md = `See this claim [^art-${uuid}].

[^art-${uuid}]: Some citation text`;
    const out = renderWikiMarkdown(md, { sources });
    // Inline ref becomes an art-ref anchor.
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${uuid}"`);
    // Definition line becomes a synthesis wikilink.
    expect(out).toContain('wikilink--synthesis');
    // Exactly one of each.
    expect((out.match(/class="art-ref"/g) || []).length).toBe(1);
    expect((out.match(/wikilink--synthesis/g) || []).length).toBe(1);
  });

  it('resolves [^{uuid}] footnote refs without the art- prefix', () => {
    // LLM variant: `[^uuid]` (no `art-` prefix). Should resolve to a green
    // art-ref when source metadata is available, instead of being stripped by
    // step 4 or left as literal `[^...]` text.
    const uuid = '4b7d9601-6ea4-46fa-8df6-44a79c6150ac';
    const sources = new Map([[uuid, src(uuid, 'Dyes and Sugars in Kids Foods', 2015)]]);
    const out = renderWikiMarkdown(`Claim [^${uuid}].`, { sources });
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${uuid}"`);
    expect(out).toContain('>Dyes and Sugars in Kids Foods (2015)<');
    // The raw [^uuid] construct must not leak as literal text.
    expect(out).not.toContain('[^');
  });

  it('resolves [^art-UUID] case-insensitively (uppercase prefix)', () => {
    // LLM variant: `[^ART-uuid]` or `[^Art-uuid]`. The prefix match must be
    // case-insensitive so these resolve instead of surviving as literal text.
    const uuid = 'd0f4daf1-2524-4fbf-906d-f3b9cc4263e7';
    const sources = new Map([[uuid, src(uuid, 'FD&C Dyes Study', 2013)]]);
    const out = renderWikiMarkdown(`See [^ART-${uuid}] and [^Art-${uuid}].`, { sources });
    expect((out.match(/class="art-ref"/g) || []).length).toBe(2);
    expect(out).toContain(`data-art-id="${uuid}"`);
    expect(out).not.toContain('[^ART-');
    expect(out).not.toContain('[^Art-');
  });

  it('converts [^{uuid}]: definition lines (no art- prefix) into synthesis chips', () => {
    // LLM variant: footnote definition without the `art-` prefix.
    const uuid = '0e4822b6-b8bb-4ed0-8333-84336a07797b';
    const sources = new Map([[uuid, src(uuid, 'Added Sugar in Australia', 2022)]]);
    const md = `Some prose [^${uuid}].

[^${uuid}]: Cross-sectional study of Australian households.`;
    const out = renderWikiMarkdown(md, { sources });
    // Definition line becomes a synthesis chip.
    expect(out).toContain('wikilink--synthesis');
    expect(out).toContain(`data-slug="${uuid}"`);
    expect(out).toContain('>Added Sugar in Australia (2022)<');
    // The definition's citation text is gone (replaced by the chip).
    expect(out).not.toContain('Cross-sectional study');
  });

  it('resolves all UUID variants in the user-reported LLM example text', () => {
    // Exact-shape regression for the user's reported "Dietary Sources and
    // Co-exposures" page: bare UUIDs in prose + a [[wikilink]] should ALL
    // resolve to chips/links, with no raw UUIDs in the rendered output.
    const uuid1 = '4b7d9601-6ea4-46fa-8df6-44a79c6150ac';
    const uuid2 = 'd0f4daf1-2524-4fbf-906d-f3b9cc4263e7';
    const uuid3 = '0e4822b6-b8bb-4ed0-8333-84336a07797b';
    const pageTitles = new Map<string, string>([
      [uuid1, 'Dyes and Sugars in Childrens Foods'],
      [uuid2, 'FD&C Certified Food Dyes'],
      [uuid3, 'Added Sugar Purchases in Australia'],
    ]);
    const md = `Children's foods frequently contain both high sugar densities and substantial amounts of artificial food colors ${uuid1}, ${uuid2}.
Major food categories contributing to household added sugar purchases include chocolate, sweets, soft drinks, and ice cream ${uuid3}.
Low-income households purchase significantly more added sugar than high-income households ${uuid3}. This makes them benefit most from [[soft-drinks-industry-levy]].`;
    const out = renderWikiMarkdown(md, { pageTitles });
    // All three UUIDs resolved to synthesis chips.
    for (const uuid of [uuid1, uuid2, uuid3]) {
      expect(out).toContain(`data-slug="${uuid}"`);
      expect(out).not.toContain(`>${uuid}<`);
    }
    // The [[wikilink]] also resolved.
    expect(out).toContain('data-slug="soft-drinks-industry-levy"');
  });

  it('does not strip [^{uuid}] constructs in step 4 even if step 1 missed them', () => {
    // Safety net: if a future change breaks step 1's resolution of `[^uuid]`,
    // step 4 must NOT strip the construct (which would lose the UUID). The
    // UUID should survive as visible text so the user can still read it.
    const uuid = '4b7d9601-6ea4-46fa-8df6-44a79c6150ac';
    // No sources / pageTitles -> step 1 falls through to the missing-ref span
    // (NOT stripped by step 4 because the content is UUID-shaped).
    const out = renderWikiMarkdown(`Claim [^${uuid}].`);
    expect(out).toContain(uuid);
    expect(out).not.toContain(`[^${uuid}]`);
  });

  it('resolves [[{uuid}]] wikilink brackets to a synthesis chip when pageTitles has it', () => {
    // The LLM often emits article UUIDs inside [[...]] brackets (the canonical
    // wikilink syntax). Without resolution the raw UUID shows as link text,
    // which is unreadable. When pageTitles has the UUID, render a pink
    // synthesis chip with the page title instead.
    const uuid = '4b7d9601-6ea4-46fa-8df6-44a79c6150ac';
    const pageTitles = new Map([[uuid, 'Dyes and Sugars in Childrens Foods']]);
    const out = renderWikiMarkdown(`See [[${uuid}]] for details.`, { pageTitles });
    expect(out).toContain('wikilink--synthesis');
    expect(out).toContain(`data-slug="${uuid}"`);
    expect(out).toContain('>Dyes and Sugars in Childrens Foods<');
    // The raw UUID must NOT appear as visible link text.
    expect(out).not.toContain(`>${uuid}<`);
  });

  it('resolves [[{uuid}]] to a green art-ref when sources has it and articlePriority is set', () => {
    // Chat view: articlePriority makes sources win over pageTitles for UUID
    // slugs inside [[...]], matching the bare-UUID behavior.
    const uuid = '4b7d9601-6ea4-46fa-8df6-44a79c6150ac';
    const sources = new Map([[uuid, src(uuid, 'Dyes Study', 2015)]]);
    const pageTitles = new Map([[uuid, 'Wiki Page Title']]);
    const out = renderWikiMarkdown(`See [[${uuid}]].`, {
      sources,
      pageTitles,
      articlePriority: true,
    });
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${uuid}"`);
    expect(out).toContain('>Dyes Study (2015)<');
    expect(out).not.toContain('wikilink--synthesis');
  });

  it('falls back to a plain wikilink for [[{uuid}]] when no metadata matches', () => {
    // No pageTitles / sources entry: the UUID slug should still render as a
    // clickable wikilink (indigo) with the raw UUID as text, so the user can
    // navigate to it if the page exists under that slug.
    const uuid = '4b7d9601-6ea4-46fa-8df6-44a79c6150ac';
    const out = renderWikiMarkdown(`See [[${uuid}]].`);
    expect(out).toContain('class="wikilink"');
    expect(out).toContain(`data-slug="${uuid}"`);
    expect(out).toContain(`>${uuid}<`);
    expect(out).not.toContain('wikilink--synthesis');
  });

  it('preserves [[{uuid}|alias]] wikilinks (alias overrides UUID resolution)', () => {
    // When the LLM provides an explicit alias, respect it as the visible text
    // rather than resolving the UUID. This matches the [[slug|alias]] contract.
    const uuid = '4b7d9601-6ea4-46fa-8df6-44a79c6150ac';
    const pageTitles = new Map([[uuid, 'Page Title']]);
    const out = renderWikiMarkdown(`See [[${uuid}|this study]] for details.`, { pageTitles });
    expect(out).toContain('>this study<');
    expect(out).toContain(`data-slug="${uuid}"`);
    // Should NOT use the pageTitles label (alias wins).
    expect(out).not.toContain('>Page Title<');
  });

  it('does not transform short hex-like text that is not a UUID', () => {
    // "deadbeef" is 8 hex chars but lacks the UUID dashes/structure.
    const out = renderWikiMarkdown('Short hex deadbeef and partial 12345678-1234.');
    expect(out).not.toContain('class="wikilink"');
  });

  // ------------------------------------------------------------------
  // External document (user-slug) references — regression for the
  // `[^art-user-youcantbuild]` mangled-HTML bug. The id capture must accept
  // non-hex slugs (`user-...`, `author-...`) so refs to uploaded documents
  // resolve to chips instead of leaking as literal text that Markdown mangles.
  // ------------------------------------------------------------------

  it('resolves [^art-user-slug] to a synthesis chip when pageTitles has the source page', () => {
    // Regression: the live symptom was `effort ^art-user-youcantbuild.`
    // rendering as a broken `<a href="a class=...` because the regex only
    // matched hex ids. After the fix the ref resolves to a wiki chip that
    // opens the pre-seeded source page (Layer 1).
    const slug = 'user-youcantbuild';
    const pageTitles = new Map([[slug, "You Can't Build an AI Workforce"]]);
    const out = renderWikiMarkdown(`effort [^art-${slug}].`, { pageTitles });
    expect(out).toContain('wikilink--synthesis');
    expect(out).toContain(`data-slug="${slug}"`);
    // The apostrophe is HTML-escaped to &#39; by escapeText (correct, safe).
    expect(out).toContain('>You Can&#39;t Build an AI Workforce<');
    // No mangled href attribute, no literal ^art- leak.
    expect(out).not.toContain('href="a class');
    expect(out).not.toContain(`^art-${slug}`);
  });

  it('resolves [^user-slug] (no art- prefix) to a synthesis chip', () => {
    // LLM variant without the `art-` prefix should also resolve.
    const slug = 'user-report-2024';
    const pageTitles = new Map([[slug, 'Annual Report 2024']]);
    const out = renderWikiMarkdown(`See [^${slug}].`, { pageTitles });
    expect(out).toContain('wikilink--synthesis');
    expect(out).toContain(`data-slug="${slug}"`);
    expect(out).toContain('>Annual Report 2024<');
    expect(out).not.toContain(`[^${slug}]`);
  });

  it('resolves [^art-user-slug]: definition lines to synthesis chips', () => {
    // Footnote definition blocks the LLM emits at page bottom.
    const slug = 'user-youcantbuild';
    const pageTitles = new Map([[slug, 'AI Workforce Article']]);
    const md = `Some prose [^art-${slug}].

[^art-${slug}]: Full citation text for the document.`;
    const out = renderWikiMarkdown(md, { pageTitles });
    // Inline ref + definition both resolve; definition's citation text gone.
    const chips = out.match(/wikilink--synthesis/g) || [];
    expect(chips.length).toBeGreaterThanOrEqual(2);
    expect(out).toContain(`data-slug="${slug}"`);
    expect(out).not.toContain('Full citation text');
    expect(out).not.toContain(']:');
  });

  it('falls back to green art-ref for [^art-user-slug] when sources has it but no wiki page', () => {
    // No pageTitles entry but the raw source list includes the user file.
    // The click still goes somewhere useful (article/source detail).
    const slug = 'user-notes';
    const sources = new Map([[slug, src(slug, 'My Notes File', null)]]);
    const out = renderWikiMarkdown(`See [^art-${slug}].`, { sources });
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${slug}"`);
    expect(out).toContain('>My Notes File<');
    expect(out).not.toContain('wikilink--synthesis');
  });

  it('renders missing-ref span for [^art-user-slug] with no matching metadata', () => {
    // Neither pageTitles nor sources know the slug -> graceful missing span.
    const out = renderWikiMarkdown('See [^art-user-unknown].');
    expect(out).toContain('art-ref--missing');
    expect(out).toContain('data-art-id="user-unknown"');
    // No mangled HTML.
    expect(out).not.toContain('href="a class');
  });

  it('resolves [[user-slug]] wikilink to a source page (standard wikilink path)', () => {
    // A bare [[user-slug]] wikilink the LLM emits for an uploaded doc. This
    // goes through the standard wikilink resolver (step 2) and produces a
    // clickable indigo link to the source page.
    const out = renderWikiMarkdown('See [[user-youcantbuild]] for context.');
    expect(out).toContain('class="wikilink"');
    expect(out).toContain('data-slug="user-youcantbuild"');
  });

  it('does not route UUID article refs to wiki pages when source metadata exists', () => {
    // Regression guard: the new non-UUID routing must not capture real UUID
    // article refs. A UUID in sources stays a green art-ref even when
    // pageTitles also has it (default behavior, no linkArtRefsToSynthesis).
    const uuid = 'f399a079-dbe4-589b-84ff-057638871f43';
    const sources = new Map([[uuid, src(uuid, 'Article Title', 2020)]]);
    const pageTitles = new Map([[uuid, 'Synthesis Page']]);
    const out = renderWikiMarkdown(`Claim [^art-${uuid}].`, { sources, pageTitles });
    expect(out).toContain('class="art-ref"');
    expect(out).toContain(`data-art-id="${uuid}"`);
    expect(out).not.toContain('wikilink--synthesis');
  });
});

describe('formatArtRefLabel', () => {
  it('includes the year when present', () => {
    expect(formatArtRefLabel(src('1', 'Sugar Tax', 2019))).toBe('Sugar Tax (2019)');
  });

  it('omits the year when null', () => {
    expect(formatArtRefLabel(src('1', 'Sugar Tax', null))).toBe('Sugar Tax');
  });

  it('truncates titles longer than 60 chars (year is appended after the ellipsis)', () => {
    const long = 'A'.repeat(80);
    const label = formatArtRefLabel(src('1', long, 2021));
    // Format is "truncated-title... (year)"; the ellipsis sits before the year.
    expect(label).toContain('...');
    expect(label.endsWith('(2021)')).toBe(true);
    expect(label.length).toBeLessThan(long.length);
    // The truncated title segment is capped at 57 chars + "...".
    const titlePart = label.slice(0, label.indexOf('...'));
    expect(titlePart.length).toBe(57);
  });
});

// ── T2.3 Phase 3: section-provenance badge ───────────────────────────────────

describe('section badge (T2.3)', () => {
  it('renders a section badge for [[slug]] with a (§Methods) suffix', () => {
    const out = renderWikiMarkdown('See [[sugar-tax]] (§Methods) for details.');
    expect(out).toContain('class="wikilink"');
    expect(out).toContain('data-slug="sugar-tax"');
    expect(out).toContain('section-badge');
    expect(out).toContain('§Methods');
  });

  it('renders no badge when the section suffix is absent (backward compat)', () => {
    const out = renderWikiMarkdown('See [[sugar-tax]] for details.');
    expect(out).toContain('class="wikilink"');
    expect(out).not.toContain('section-badge');
  });

  it('renders the section badge after an alias ([[slug|Title]] (§Results))', () => {
    const out = renderWikiMarkdown('See [[sugar-tax|the levy]] (§Results).');
    expect(out).toContain('class="wikilink"');
    expect(out).toContain('data-slug="sugar-tax"');
    expect(out).toContain('>the levy<');
    expect(out).toContain('section-badge');
    expect(out).toContain('§Results');
  });
});
