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

  it('uses source title as alias for a bare UUID when sources map has it', () => {
    const uuid = 'f399a079-dbe4-589b-84ff-057638871f43';
    const sources = new Map([[uuid, src(uuid, 'Sugar Levy Study', 2020)]]);
    const out = renderWikiMarkdown(`See ${uuid}.`, { sources });
    expect(out).toContain(`data-slug="${uuid}"`);
    expect(out).toContain('>Sugar Levy Study (2020)<');
    // Raw UUID should not appear as visible text.
    expect(out).not.toContain(`>${uuid}<`);
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

  it('does not transform short hex-like text that is not a UUID', () => {
    // "deadbeef" is 8 hex chars but lacks the UUID dashes/structure.
    const out = renderWikiMarkdown('Short hex deadbeef and partial 12345678-1234.');
    expect(out).not.toContain('class="wikilink"');
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
