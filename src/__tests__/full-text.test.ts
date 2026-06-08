import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useArticleSearch } from '@/composables/use-article-search';
import type { Article } from '@/types';

// Mock the tauri-command composable
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const mockArticle: Article = {
  id: 'test-article-1',
  sequenceId: 1,
  status: 'working',
  title: 'Test Article',
  authors: ['Author A'],
  abstractText: 'Test abstract',
  screeningError: false,
  keywords: [],
  tags: [],
  labels: [],
  manualOverride: false,
  importSource: 'test.ris',
  importedAt: '2023-01-01',
  changedAt: '2023-01-01',
  screenedAt: null,
  publicationYear: 2023,
  doi: '10.1234/test',
  journal: 'Test Journal',
  volume: null,
  issue: null,
  startPage: null,
  endPage: null,
  url: null,
  language: null,
  publisher: null,
  publisherCity: null,
  publisherAddress: null,
  issn: null,
  referenceType: null,
  date: null,
  authorAddress: null,
  affiliation: null,
  accessionNumber: null,
  customField3: null,
  journalAbbreviation: null,
  journalIsoAbbreviation: null,
  notes: null,
  webOfScienceDb: null,
  userNotes: null,
  risExtras: null,
  duplicateOf: null,
  aiDecision: null,
  aiReasoning: null,
  aiConfidence: null,
  matchedInclusionCriteria: [],
  matchedExclusionCriteria: [],
  fullText: null,
  fullTextAiSummary: null,
  numCited: null,
  numReferences: null,
  hasCitationDetails: false,
  hasReferenceDetails: false,
  hasFullText: false,
  fullTextFileName: null,
};

const mockArticleWithFullText: Article = {
  ...mockArticle,
  hasFullText: true,
  fullText: 'This is the extracted full text content.',
  fullTextFileName: 'test-article-1_paper.pdf',
};

describe('Full Text Operations', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  describe('attachFullText', () => {
    it('calls attach_full_text command with correct arguments', async () => {
      const { attachFullText } = useArticleSearch();

      // Mock: attach command succeeds
      vi.mocked(tauriCommand).mockResolvedValueOnce(undefined);
      // Mock: selectArticle re-fetches the article with full text
      vi.mocked(tauriCommand).mockResolvedValueOnce(mockArticleWithFullText);
      // Mock: get_audit_trail
      vi.mocked(tauriCommand).mockResolvedValueOnce([]);
      // Mock: fetchCounts
      vi.mocked(tauriCommand).mockResolvedValueOnce({
        all: 1,
        duplicate: 0,
        working: 1,
        included: 0,
        rejected: 0,
        error: 0,
      });

      await attachFullText('test-article-1', '/path/to/paper.pdf');

      // Verify the attach command was called with correct args
      expect(tauriCommand).toHaveBeenCalledWith('attach_full_text', {
        articleId: 'test-article-1',
        filePath: '/path/to/paper.pdf',
      });

      // Verify article was re-fetched to get updated data
      expect(tauriCommand).toHaveBeenCalledWith('get_article', {
        id: 'test-article-1',
      });
    });

    it('handles attach errors gracefully', async () => {
      const { attachFullText } = useArticleSearch();

      vi.mocked(tauriCommand).mockRejectedValueOnce(new Error('File not found'));

      await expect(attachFullText('test-article-1', '/nonexistent.pdf')).rejects.toThrow(
        'File not found'
      );
    });
  });

  describe('deleteFullTextAttachment', () => {
    it('calls delete_full_text command and refreshes article', async () => {
      const { deleteFullTextAttachment } = useArticleSearch();

      // Mock: delete command succeeds
      vi.mocked(tauriCommand).mockResolvedValueOnce(undefined);
      // Mock: selectArticle re-fetches (now without full text)
      vi.mocked(tauriCommand).mockResolvedValueOnce(mockArticle);
      // Mock: get_audit_trail
      vi.mocked(tauriCommand).mockResolvedValueOnce([]);
      // Mock: fetchCounts
      vi.mocked(tauriCommand).mockResolvedValueOnce({
        all: 1,
        duplicate: 0,
        working: 1,
        included: 0,
        rejected: 0,
        error: 0,
      });

      await deleteFullTextAttachment('test-article-1');

      // Verify the delete command was called
      expect(tauriCommand).toHaveBeenCalledWith('delete_full_text', {
        articleId: 'test-article-1',
      });

      // Verify article was re-fetched
      expect(tauriCommand).toHaveBeenCalledWith('get_article', {
        id: 'test-article-1',
      });
    });

    it('handles delete errors gracefully', async () => {
      const { deleteFullTextAttachment } = useArticleSearch();

      vi.mocked(tauriCommand).mockRejectedValueOnce(new Error('Delete failed'));

      await expect(deleteFullTextAttachment('test-article-1')).rejects.toThrow('Delete failed');
    });
  });

  describe('readFullTextContent', () => {
    it('returns full text content for an article', async () => {
      const { readFullTextContent } = useArticleSearch();

      vi.mocked(tauriCommand).mockResolvedValueOnce('This is the extracted full text content.');

      const result = await readFullTextContent('test-article-1');

      expect(tauriCommand).toHaveBeenCalledWith('read_full_text', {
        articleId: 'test-article-1',
      });
      expect(result).toBe('This is the extracted full text content.');
    });

    it('returns null when no full text exists', async () => {
      const { readFullTextContent } = useArticleSearch();

      vi.mocked(tauriCommand).mockResolvedValueOnce(null);

      const result = await readFullTextContent('test-article-no-text');

      expect(result).toBeNull();
    });
  });

  describe('getFullTextFilePath', () => {
    it('returns the file path for an attached document', async () => {
      const { getFullTextFilePath } = useArticleSearch();

      vi.mocked(tauriCommand).mockResolvedValueOnce(
        '/home/user/.local/share/bango/documents/test-article-1_paper.pdf'
      );

      const result = await getFullTextFilePath('test-article-1');

      expect(tauriCommand).toHaveBeenCalledWith('get_full_text_file_path', {
        articleId: 'test-article-1',
      });
      expect(result).toContain('test-article-1_paper.pdf');
    });

    it('returns null when no file is attached', async () => {
      const { getFullTextFilePath } = useArticleSearch();

      vi.mocked(tauriCommand).mockResolvedValueOnce(null);

      const result = await getFullTextFilePath('test-article-no-file');

      expect(result).toBeNull();
    });
  });

  describe('Full text display logic', () => {
    it('article without full text has hasFullText=false and no fileName', () => {
      expect(mockArticle.hasFullText).toBe(false);
      expect(mockArticle.fullTextFileName).toBeNull();
      expect(mockArticle.fullText).toBeNull();
    });

    it('article with full text has hasFullText=true and fileName set', () => {
      expect(mockArticleWithFullText.hasFullText).toBe(true);
      expect(mockArticleWithFullText.fullTextFileName).toBe('test-article-1_paper.pdf');
      expect(mockArticleWithFullText.fullText).toBe('This is the extracted full text content.');
    });

    it('PDF file name is detected from fullTextFileName', () => {
      const name = mockArticleWithFullText.fullTextFileName;
      expect(name?.toLowerCase().endsWith('.pdf')).toBe(true);
    });

    it('TXT file name is detected from fullTextFileName', () => {
      const txtArticle: Article = {
        ...mockArticleWithFullText,
        fullTextFileName: 'test-article-1_notes.txt',
      };
      expect(txtArticle.fullTextFileName?.toLowerCase().endsWith('.txt')).toBe(true);
    });
  });
});
