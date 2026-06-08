import { describe, it, expect, vi, beforeEach } from 'vitest';
import { flattenRawReferences, type RawReference } from '@/utils/reference-flatten';

// ─── Sample data ────────────────────────────────────────────────

function makeRawRef(overrides: Partial<RawReference> = {}): RawReference {
  return {
    linkId: 'link-1',
    parentArticleId: 'parent-1',
    referenceType: 'reference',
    paper: {
      id: 'paper-1',
      title: 'A Study of Testing Patterns',
      abstractText: 'Abstract about testing.',
      authors: ['Alice Smith', 'Bob Jones'],
      publicationYear: 2024,
      doi: '10.1234/test.2024',
      journal: 'Journal of Software Testing',
      volume: '12',
      issue: '3',
      startPage: '45',
      endPage: '67',
      keywords: ['testing', 'patterns'],
      url: 'https://example.com/paper1',
      language: 'en',
      publisher: 'TestPub',
      matchStatus: 'matched',
      citationCount: 42,
      referenceCount: 15,
      hasFullText: true,
      fullTextFileName: 'paper1.pdf',
      importSource: 'ris',
      createdAt: '2024-01-15T10:30:00Z',
    },
    ...overrides,
  };
}

// ─── flattenRawReferences ───────────────────────────────────────

describe('flattenRawReferences', () => {
  it('flattens nested { linkId, referenceType, paper: {...} } to flat ArticleReference', () => {
    const raw = [makeRawRef()];
    const result = flattenRawReferences(raw);

    expect(result).toHaveLength(1);
    const ref = result[0]!;

    // Top-level fields from the link
    expect(ref.id).toBe('paper-1');
    expect(ref.referenceType).toBe('reference');
    expect(ref.parentId).toBe('parent-1');

    // Flattened paper fields
    expect(ref.title).toBe('A Study of Testing Patterns');
    expect(ref.authors).toEqual(['Alice Smith', 'Bob Jones']);
    expect(ref.publicationYear).toBe(2024);
    expect(ref.doi).toBe('10.1234/test.2024');
    expect(ref.journal).toBe('Journal of Software Testing');
    expect(ref.matchStatus).toBe('matched');
  });

  it('maps citationCount → numCited, referenceCount → numReferences', () => {
    const raw = [makeRawRef()];
    const result = flattenRawReferences(raw);

    expect(result[0]!.numCited).toBe(42);
    expect(result[0]!.numReferences).toBe(15);
  });

  it('maps createdAt → importedAt', () => {
    const raw = [makeRawRef()];
    const result = flattenRawReferences(raw);

    expect(result[0]!.importedAt).toBe('2024-01-15T10:30:00Z');
  });

  it('falls back id to linkId when paper.id is missing', () => {
    const raw = [
      makeRawRef({
        paper: {
          title: 'No ID Paper',
          authors: [],
        },
      }),
    ];
    const result = flattenRawReferences(raw);

    expect(result[0]!.id).toBe('link-1');
  });

  it('defaults matchStatus to "unmatched" when missing', () => {
    const raw = [
      makeRawRef({
        paper: { title: 'No Match Status', authors: [] },
      }),
    ];
    const result = flattenRawReferences(raw);

    expect(result[0]!.matchStatus).toBe('unmatched');
  });

  it('preserves matchStatus from paper when present', () => {
    const raw = [
      makeRawRef({
        paper: { title: 'Matched', authors: [], matchStatus: 'not_in_library' },
      }),
    ];
    const result = flattenRawReferences(raw);

    expect(result[0]!.matchStatus).toBe('not_in_library');
  });

  it('defaults authors to empty array when null', () => {
    const raw = [
      makeRawRef({
        paper: { title: 'No Authors', authors: null },
      }),
    ];
    const result = flattenRawReferences(raw);

    expect(result[0]!.authors).toEqual([]);
  });

  it('handles null paper gracefully', () => {
    const raw = [
      makeRawRef({
        linkId: 'link-orphan',
        parentArticleId: 'parent-1',
        referenceType: 'citation',
        paper: null,
      }),
    ];
    const result = flattenRawReferences(raw);

    expect(result).toHaveLength(1);
    expect(result[0]!.id).toBe('link-orphan');
    expect(result[0]!.referenceType).toBe('citation');
    expect(result[0]!.title).toBeNull();
    expect(result[0]!.authors).toEqual([]);
    expect(result[0]!.matchStatus).toBe('unmatched');
    expect(result[0]!.numCited).toBeNull();
    expect(result[0]!.numReferences).toBeNull();
    expect(result[0]!.importedAt).toBe('');
  });

  it('handles missing optional fields with defaults', () => {
    const raw = [
      makeRawRef({
        paper: {
          id: 'paper-sparse',
          title: null,
          authors: null,
          publicationYear: null,
          doi: null,
          journal: null,
          keywords: null,
          citationCount: null,
          referenceCount: null,
          hasFullText: null,
          createdAt: null,
        },
      }),
    ];
    const result = flattenRawReferences(raw);

    const ref = result[0]!;
    expect(ref.title).toBeNull();
    expect(ref.authors).toEqual([]);
    expect(ref.publicationYear).toBeNull();
    expect(ref.doi).toBeNull();
    expect(ref.journal).toBeNull();
    expect(ref.keywords).toEqual([]);
    expect(ref.numCited).toBeNull();
    expect(ref.numReferences).toBeNull();
    expect(ref.hasFullText).toBe(false);
    expect(ref.importedAt).toBe('');
  });

  it('returns empty array for empty input', () => {
    expect(flattenRawReferences([])).toEqual([]);
  });

  it('handles multiple references correctly', () => {
    const raw = [
      makeRawRef({
        linkId: 'link-1',
        referenceType: 'reference',
        paper: { title: 'Ref A', authors: ['A'] },
      }),
      makeRawRef({
        linkId: 'link-2',
        referenceType: 'citation',
        paper: { title: 'Ref B', authors: ['B'] },
      }),
      makeRawRef({ linkId: 'link-3', referenceType: 'reference', paper: null }),
    ];
    const result = flattenRawReferences(raw);

    expect(result).toHaveLength(3);
    expect(result[0]!.referenceType).toBe('reference');
    expect(result[0]!.title).toBe('Ref A');
    expect(result[1]!.referenceType).toBe('citation');
    expect(result[1]!.title).toBe('Ref B');
    expect(result[2]!.title).toBeNull();
  });

  it('converts hasFullText to boolean', () => {
    const raw = [
      makeRawRef({ paper: { title: 'With FT', authors: [], hasFullText: true } }),
      makeRawRef({ paper: { title: 'Without FT', authors: [], hasFullText: false } }),
      makeRawRef({ paper: { title: 'Null FT', authors: [], hasFullText: null } }),
    ];
    const result = flattenRawReferences(raw);

    expect(result[0]!.hasFullText).toBe(true);
    expect(result[1]!.hasFullText).toBe(false);
    expect(result[2]!.hasFullText).toBe(false);
  });
});

// ─── useReferences composable ───────────────────────────────────

// Mock tauri command
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { useReferences } from '@/composables/use-references';

describe('useReferences', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('getArticleReferences', () => {
    it('calls get_article_references with articleId', async () => {
      vi.mocked(tauriCommand).mockResolvedValue([]);

      const { getArticleReferences } = useReferences();
      await getArticleReferences('article-1');

      expect(tauriCommand).toHaveBeenCalledWith('get_article_references', {
        articleId: 'article-1',
      });
    });

    it('passes refType when provided', async () => {
      vi.mocked(tauriCommand).mockResolvedValue([]);

      const { getArticleReferences } = useReferences();
      await getArticleReferences('article-1', 'citation');

      expect(tauriCommand).toHaveBeenCalledWith('get_article_references', {
        articleId: 'article-1',
        refType: 'citation',
      });
    });

    it('returns empty array on error', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('DB error'));

      const { getArticleReferences, error } = useReferences();
      const result = await getArticleReferences('article-1');

      expect(result).toEqual([]);
      expect(error.value).toBe('DB error');
    });

    it('sets loading state correctly', async () => {
      let resolve!: (v: unknown) => void;
      const promise = new Promise((r) => (resolve = r));
      vi.mocked(tauriCommand).mockReturnValue(promise);

      const { getArticleReferences, loading } = useReferences();

      const call = getArticleReferences('article-1');
      expect(loading.value).toBe(true);

      resolve([]);
      await call;

      expect(loading.value).toBe(false);
    });
  });

  describe('extractCrReferences', () => {
    it('calls extract_cr_references with correct payload', async () => {
      vi.mocked(tauriCommand).mockResolvedValue({
        papersCreated: 5,
        linksCreated: 5,
        errors: [],
      });

      const { extractCrReferences } = useReferences();
      const result = await extractCrReferences('article-1', { CR: ['Smith 2020'] });

      expect(tauriCommand).toHaveBeenCalledWith('extract_cr_references', {
        payload: { articleId: 'article-1', risExtras: { CR: ['Smith 2020'] } },
      });
      expect(result!.papersCreated).toBe(5);
    });

    it('returns null on error', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('fail'));

      const { extractCrReferences, error } = useReferences();
      const result = await extractCrReferences('article-1', null);

      expect(result).toBeNull();
      expect(error.value).toBe('fail');
    });
  });

  describe('previewReferencesImport', () => {
    it('calls preview_references_import with file path', async () => {
      const preview = { papers: [], totalCount: 0, errors: [] };
      vi.mocked(tauriCommand).mockResolvedValue(preview);

      const { previewReferencesImport } = useReferences();
      const result = await previewReferencesImport('/path/to/refs.ris');

      expect(tauriCommand).toHaveBeenCalledWith('preview_references_import', {
        filePath: '/path/to/refs.ris',
      });
      expect(result).toEqual(preview);
    });
  });

  describe('importReferencesForArticle', () => {
    it('calls import_references_for_article with correct payload', async () => {
      vi.mocked(tauriCommand).mockResolvedValue({
        papersCreated: 3,
        linksCreated: 3,
        errors: [],
      });

      const { importReferencesForArticle } = useReferences();
      const result = await importReferencesForArticle('art-1', '/path/refs.ris', 'reference');

      expect(tauriCommand).toHaveBeenCalledWith('import_references_for_article', {
        payload: {
          articleId: 'art-1',
          filePath: '/path/refs.ris',
          refType: 'reference',
        },
      });
      expect(result!.linksCreated).toBe(3);
    });
  });
});
