import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// Hoisted shared mock for the Tauri IPC boundary. Mirrors the
// use-startup-upgrade.test.ts pattern to avoid inter-test leakage.
const mockTauriCommand = vi.fn();

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: (...args: unknown[]) => mockTauriCommand(...args),
}));

import { useWiki } from '@/composables/use-wiki';

describe('use-wiki', () => {
  beforeEach(() => {
    mockTauriCommand.mockReset();
    // Reset the singleton state between tests.
    useWiki().resetState();
  });

  afterEach(() => {
    mockTauriCommand.mockReset();
  });

  it('exports the expected surface', () => {
    const wiki = useWiki();
    expect(typeof wiki.refreshStatus).toBe('function');
    expect(typeof wiki.getRootDir).toBe('function');
    expect(typeof wiki.setRootDir).toBe('function');
    expect(typeof wiki.initWiki).toBe('function');
    expect(typeof wiki.exportRaw).toBe('function');
    expect(typeof wiki.addRawFile).toBe('function');
    expect(typeof wiki.listRawFiles).toBe('function');
    expect(typeof wiki.searchWiki).toBe('function');
    expect(typeof wiki.lintWiki).toBe('function');
    expect(typeof wiki.getPage).toBe('function');
    expect(typeof wiki.updatePage).toBe('function');
    expect(typeof wiki.deletePage).toBe('function');
    expect(typeof wiki.deleteWiki).toBe('function');
    expect(typeof wiki.chatWiki).toBe('function');
    expect(typeof wiki.getGraph).toBe('function');
    expect(typeof wiki.ingestWiki).toBe('function');
    expect(typeof wiki.listPages).toBe('function');
  });

  describe('refreshStatus', () => {
    it('fetches wiki_get_status and stores it in status ref', async () => {
      const sample = {
        configured: true,
        rootDir: '/tmp/wiki-root',
        isCustom: false,
        defaultPath: '/tmp/wiki-root',
        rawCount: 3,
        pageCount: 0,
        needsRefresh: false,
        includedArticleCount: 5,
        initialized: true,
      };
      mockTauriCommand.mockResolvedValue(sample);
      const wiki = useWiki();
      await wiki.refreshStatus();
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_get_status');
      expect(wiki.status.value).toEqual(sample);
      expect(wiki.loading.value).toBe(false);
      expect(wiki.error.value).toBeNull();
    });

    it('captures errors and clears loading', async () => {
      mockTauriCommand.mockRejectedValue(new Error('ipc failed'));
      const wiki = useWiki();
      await wiki.refreshStatus();
      expect(wiki.status.value).toBeNull();
      expect(wiki.error.value).toBe('ipc failed');
      expect(wiki.loading.value).toBe(false);
    });
  });

  describe('initWiki', () => {
    it('calls wiki_init and refreshes status after success', async () => {
      mockTauriCommand
        .mockResolvedValueOnce({ rootDir: '/tmp/wiki-root', created: true }) // wiki_init
        .mockResolvedValueOnce({ configured: true }); // refreshStatus -> wiki_get_status
      const wiki = useWiki();
      const result = await wiki.initWiki();
      expect(result.created).toBe(true);
      // First call: wiki_init, second: wiki_get_status
      expect(mockTauriCommand).toHaveBeenNthCalledWith(1, 'wiki_init');
      expect(mockTauriCommand).toHaveBeenNthCalledWith(2, 'wiki_get_status');
      expect(wiki.initializing.value).toBe(false);
    });

    it('captures errors and rethrows after clearing initializing flag', async () => {
      mockTauriCommand.mockRejectedValue(new Error('init failed'));
      const wiki = useWiki();
      await expect(wiki.initWiki()).rejects.toThrow('init failed');
      expect(wiki.initializing.value).toBe(false);
      expect(wiki.error.value).toBe('init failed');
    });
  });

  describe('exportRaw', () => {
    it('calls wiki_export_raw and returns the report', async () => {
      const report = {
        articlesWritten: 2,
        articlesSkipped: 1,
        userFilesWritten: 0,
        userFilesSkipped: 0,
        userFilesUnsupported: [],
      };
      mockTauriCommand.mockResolvedValue(report);
      const wiki = useWiki();
      const result = await wiki.exportRaw();
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_export_raw');
      expect(result).toEqual(report);
    });
  });

  describe('addRawFile', () => {
    it('calls wiki_add_raw_file with the file path, then refreshes status', async () => {
      mockTauriCommand
        .mockResolvedValueOnce('/tmp/wiki-root/raw/user-notes.md') // wiki_add_raw_file
        .mockResolvedValueOnce({ configured: true }); // refreshStatus
      const wiki = useWiki();
      const companionPath = await wiki.addRawFile('/home/me/notes.txt');
      expect(companionPath).toBe('/tmp/wiki-root/raw/user-notes.md');
      expect(mockTauriCommand).toHaveBeenNthCalledWith(1, 'wiki_add_raw_file', {
        filePath: '/home/me/notes.txt',
      });
    });
  });

  describe('listRawFiles', () => {
    it('calls wiki_list_raw_files and returns the entries', async () => {
      const entries = [
        {
          path: '/tmp/wiki-root/raw/art-1.md',
          title: 'Article 1',
          slug: 'art-1',
          sourceKind: '',
          sourceFile: null,
          status: 'draft',
        },
      ];
      mockTauriCommand.mockResolvedValue(entries);
      const wiki = useWiki();
      const result = await wiki.listRawFiles();
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_list_raw_files');
      expect(result).toHaveLength(1);
      expect(result[0]?.title).toBe('Article 1');
    });
  });

  describe('setRootDir', () => {
    it('calls wiki_set_root_dir with the path, then refreshes status', async () => {
      mockTauriCommand
        .mockResolvedValueOnce({
          effectivePath: '/custom/wiki-root',
          isCustom: true,
          defaultPath: '/tmp/wiki-root',
        })
        .mockResolvedValueOnce({ configured: true }); // refreshStatus
      const wiki = useWiki();
      await wiki.setRootDir('/custom/wiki-root');
      expect(mockTauriCommand).toHaveBeenNthCalledWith(1, 'wiki_set_root_dir', {
        path: '/custom/wiki-root',
      });
    });
  });

  describe('searchWiki', () => {
    it('calls wiki_search with query and limit', async () => {
      const hits = [
        {
          slug: 'sugar-tax',
          title: 'Sugar Tax',
          summary: '',
          pageType: 'concept',
          sourceArticles: '[]',
          filePath: 'wiki/concepts/sugar-tax.md',
          rank: -3.5,
        },
      ];
      mockTauriCommand.mockResolvedValue(hits);
      const wiki = useWiki();
      const result = await wiki.searchWiki('sugar', 5);
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_search', { query: 'sugar', limit: 5 });
      expect(result).toHaveLength(1);
      expect(result[0]?.slug).toBe('sugar-tax');
    });

    it('defaults limit to 10 when omitted', async () => {
      mockTauriCommand.mockResolvedValue([]);
      const wiki = useWiki();
      await wiki.searchWiki('anything');
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_search', {
        query: 'anything',
        limit: 10,
      });
    });
  });

  describe('lintWiki', () => {
    it('calls wiki_lint and returns the report', async () => {
      const report = {
        pageCount: 3,
        issueCount: 1,
        errors: 0,
        warnings: 1,
        infos: 0,
        issues: [
          {
            page: 'alpha.md',
            slug: 'alpha',
            severity: 'warning',
            kind: 'broken-link',
            message: '[[bad]] points to a non-existent page',
          },
        ],
        slugs: ['alpha', 'beta', 'gamma'],
      };
      mockTauriCommand.mockResolvedValue(report);
      const wiki = useWiki();
      const result = await wiki.lintWiki();
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_lint');
      expect(result.pageCount).toBe(3);
      expect(result.issues).toHaveLength(1);
      expect(result.issues[0]?.kind).toBe('broken-link');
    });
  });

  describe('getPage', () => {
    it('calls wiki_get_page with the slug', async () => {
      const page = {
        slug: 'alpha',
        title: 'Alpha',
        pageType: 'concept',
        status: 'draft',
        summary: '',
        body: '# Alpha',
        filePath: 'wiki/concepts/alpha.md',
        sourceArticles: null,
      };
      mockTauriCommand.mockResolvedValue(page);
      const wiki = useWiki();
      const result = await wiki.getPage('alpha');
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_get_page', { slug: 'alpha' });
      expect(result?.title).toBe('Alpha');
    });

    it('returns null when the page is not found', async () => {
      mockTauriCommand.mockResolvedValue(null);
      const wiki = useWiki();
      const result = await wiki.getPage('missing');
      expect(result).toBeNull();
    });
  });

  describe('updatePage', () => {
    it('calls wiki_update_page with slug, title, summary, body', async () => {
      const updated = {
        slug: 'alpha',
        title: 'Alpha v2',
        pageType: 'concept',
        status: 'draft',
        summary: 'new summary',
        body: '# Alpha v2',
        filePath: 'wiki/concepts/alpha.md',
        sourceArticles: null,
      };
      mockTauriCommand.mockResolvedValue(updated);
      const wiki = useWiki();
      const result = await wiki.updatePage('alpha', 'Alpha v2', 'new summary', '# Alpha v2');
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_update_page', {
        slug: 'alpha',
        title: 'Alpha v2',
        summary: 'new summary',
        body: '# Alpha v2',
      });
      expect(result.title).toBe('Alpha v2');
    });
  });

  describe('deletePage', () => {
    it('calls wiki_delete_page and refreshes status on success', async () => {
      mockTauriCommand
        .mockResolvedValueOnce(true) // wiki_delete_page
        .mockResolvedValueOnce({ configured: true }); // refreshStatus
      const wiki = useWiki();
      const deleted = await wiki.deletePage('alpha');
      expect(deleted).toBe(true);
      expect(mockTauriCommand).toHaveBeenNthCalledWith(1, 'wiki_delete_page', { slug: 'alpha' });
    });

    it('returns false without refreshing when page not found', async () => {
      mockTauriCommand.mockResolvedValueOnce(false);
      const wiki = useWiki();
      const deleted = await wiki.deletePage('missing');
      expect(deleted).toBe(false);
    });
  });

  describe('deleteWiki', () => {
    it('calls wiki_delete_wiki and refreshes status', async () => {
      mockTauriCommand
        .mockResolvedValueOnce(undefined) // wiki_delete_wiki
        .mockResolvedValueOnce({ configured: true }); // refreshStatus
      const wiki = useWiki();
      await wiki.deleteWiki();
      expect(mockTauriCommand).toHaveBeenNthCalledWith(1, 'wiki_delete_wiki');
      expect(mockTauriCommand).toHaveBeenNthCalledWith(2, 'wiki_get_status');
    });
  });

  describe('chatWiki', () => {
    it('calls wiki_chat with question and history', async () => {
      mockTauriCommand.mockResolvedValue('Based on [[sugar-tax]], the levy reduced consumption.');
      const wiki = useWiki();
      const history = [
        { role: 'user' as const, content: 'What is the sugar tax?' },
        { role: 'assistant' as const, content: 'A levy on sugary drinks.' },
      ];
      const result = await wiki.chatWiki('Did it work?', history);
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_chat', {
        question: 'Did it work?',
        history,
      });
      expect(result).toContain('[[sugar-tax]]');
    });

    it('works with empty history', async () => {
      mockTauriCommand.mockResolvedValue('No wiki pages found.');
      const wiki = useWiki();
      const result = await wiki.chatWiki('anything', []);
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_chat', {
        question: 'anything',
        history: [],
      });
      expect(typeof result).toBe('string');
    });
  });

  describe('getGraph', () => {
    it('calls wiki_get_graph and returns the graph', async () => {
      const graph = {
        nodes: [
          { slug: 'alpha', title: 'Alpha', pageType: 'concept', inbound: 1, outbound: 1 },
          { slug: 'beta', title: 'Beta', pageType: 'concept', inbound: 1, outbound: 1 },
        ],
        edges: [{ source: 'alpha', target: 'beta' }],
        orphanCount: 0,
      };
      mockTauriCommand.mockResolvedValue(graph);
      const wiki = useWiki();
      const result = await wiki.getGraph();
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_get_graph');
      expect(result.nodes).toHaveLength(2);
      expect(result.edges).toHaveLength(1);
      expect(result.orphanCount).toBe(0);
    });

    it('returns empty graph when wiki has no pages', async () => {
      mockTauriCommand.mockResolvedValue({ nodes: [], edges: [], orphanCount: 0 });
      const wiki = useWiki();
      const result = await wiki.getGraph();
      expect(result.nodes).toHaveLength(0);
    });
  });

  describe('listPages', () => {
    it('calls wiki_list_pages and returns page summaries', async () => {
      const summaries = [
        {
          slug: 'alpha',
          title: 'Alpha',
          pageType: 'concept',
          status: 'draft',
          summary: 'A concept page',
        },
        {
          slug: 'jane-doe',
          title: 'Jane Doe',
          pageType: 'author',
          status: 'reviewed',
          summary: 'An author',
        },
      ];
      mockTauriCommand.mockResolvedValue(summaries);
      const wiki = useWiki();
      const result = await wiki.listPages();
      expect(mockTauriCommand).toHaveBeenCalledWith('wiki_list_pages');
      expect(result).toHaveLength(2);
      expect(result[0]?.slug).toBe('alpha');
      expect(result[0]?.pageType).toBe('concept');
    });

    it('returns empty array when no pages exist', async () => {
      mockTauriCommand.mockResolvedValue([]);
      const wiki = useWiki();
      const result = await wiki.listPages();
      expect(result).toHaveLength(0);
    });
  });
});
