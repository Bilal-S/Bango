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
