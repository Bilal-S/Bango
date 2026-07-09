import { describe, it, expect } from 'vitest';
import {
  buildSearchIndex,
  makeSlugToHref,
  pageDepth,
  renderArticleStub,
  slugifyFilename,
  wrapPageHtml,
} from '@/utils/wiki-site-export';
import type { WikiPageSummary, WikiSourceInfo } from '@/types/wiki';

function page(slug: string, title: string, pageType: string, summary = ''): WikiPageSummary {
  return { slug, title, pageType, status: 'draft', summary };
}

function source(
  id: string,
  title: string,
  year: number | null,
  extra: Partial<WikiSourceInfo> = {}
): WikiSourceInfo {
  return {
    id,
    title,
    authors: [],
    year,
    doi: null,
    abstractText: '',
    journal: null,
    ...extra,
  };
}

describe('wiki-site-export helpers', () => {
  it('build_search_index_includes_all_pages: one entry per page', () => {
    const pages = [
      page('sugar-tax', 'Sugar Tax', 'concepts', 'A concept page.'),
      page('jane-smith', 'Jane Smith', 'authors', 'An author.'),
    ];
    const bodies = new Map<string, string>([
      ['sugar-tax', 'Body text about the sugar tax.'],
      ['jane-smith', 'Jane published many papers.'],
    ]);
    const index = buildSearchIndex(pages, bodies);
    expect(index).toHaveLength(2);
    expect(index[0]?.slug).toBe('sugar-tax');
    expect(index[0]?.title).toBe('Sugar Tax');
    expect(index[0]?.bodyExcerpt).toContain('Body text about the sugar tax.');
    expect(index[1]?.slug).toBe('jane-smith');
  });

  it('render_article_stub_has_metadata_no_full_text: has DOI/journal, no full text leak', () => {
    const src = source('art-123', 'Sugar Reduction Study', 2024, {
      authors: ['Doe, J.'],
      doi: '10.1001/test',
      journal: 'Nature',
      abstractText: 'We studied sugar reduction.',
    });
    const html = renderArticleStub(src, []);
    // Metadata present.
    expect(html).toContain('Sugar Reduction Study');
    expect(html).toContain('Doe, J.');
    expect(html).toContain('10.1001/test');
    expect(html).toContain('Nature');
    expect(html).toContain('We studied sugar reduction.');
    // No full-text column leaks into the stub.
    expect(html).not.toContain('full_text');
    expect(html).not.toContain('content_source');
  });

  it('slug_to_href_is_depth_aware: depth 2 emits ../../ prefix (subpages)', () => {
    // Wiki pages live at pages/{type}/{slug}.html - TWO directories deep.
    // Links from a subpage need `../../` to reach the root, then the path.
    const pages = [page('sugar-tax', 'Sugar Tax', 'concept')];
    const resolver = makeSlugToHref(pages, 2);
    expect(resolver('sugar-tax')).toBe('../../pages/concept/sugar-tax.html');
    // Depth 0 (index) emits root-relative (no prefix).
    const resolver0 = makeSlugToHref(pages, 0);
    expect(resolver0('sugar-tax')).toBe('pages/concept/sugar-tax.html');
    // Missing slug returns null.
    expect(resolver('missing')).toBeNull();
  });

  it('wrapPageHtml_subpage_emits_correct_depth_prefix: ../../style.css', () => {
    // A wiki page at pages/concept/sugar-tax.html (depth 2) must reference
    // the root-level stylesheet via `../../style.css`.
    const html = wrapPageHtml('Sugar Tax', '<p>body</p>', 'concept', 'sugar-tax', 2);
    expect(html).toContain('href="../../style.css"');
    expect(html).toContain('href="../../index.html"');
    // The body is wrapped in `.markdown-content` (matching the in-app viewer).
    expect(html).toContain('class="markdown-content"');
    // Index page (depth 0) references style.css directly.
    const indexHtml = wrapPageHtml('Home', '<p>body</p>', 'index', 'index', 0);
    expect(indexHtml).toContain('href="style.css"');
  });

  it('pageDepth: index is 0, subpages are 2', () => {
    expect(pageDepth('index')).toBe(0);
    expect(pageDepth('concept')).toBe(2);
    expect(pageDepth('synthesis')).toBe(2);
    expect(pageDepth('author')).toBe(2);
  });

  it('slugifyFilename: normalizes to kebab-case', () => {
    expect(slugifyFilename('Sugar Tax Study!')).toBe('sugar-tax-study');
    expect(slugifyFilename('  multiple   spaces  ')).toBe('multiple-spaces');
    expect(slugifyFilename('')).toBe('wiki');
  });
});
