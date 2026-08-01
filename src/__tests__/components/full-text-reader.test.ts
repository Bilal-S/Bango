import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { shimLocalStorage, makeArticle } from '../helpers/fixtures';
import type { Article } from '@/types';

/* The component imports `tauriCommand` lazily inside `openFullTextView()`.
   Mock the module so the IPC calls are interceptable per-test. */
const mockTauriCommand = vi.fn();
vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: (...args: unknown[]) => mockTauriCommand(...args),
}));

/* `openPath` from the opener plugin is called by `openFileExternally`; stub
   it so the test never attempts a real OS-level open. */
vi.mock('@tauri-apps/plugin-opener', () => ({
  openPath: vi.fn().mockResolvedValue(undefined),
}));

/* `requestArticleAiSummary` is imported at module load; stub it so no IPC /
   toast side effects fire during mount. */
vi.mock('@/composables/use-ai-summary', () => ({
  requestArticleAiSummary: vi.fn(),
  parseAiSummary: vi.fn(() => null),
  pendingSummaries: { value: new Set<string>() },
}));

import FullTextReader from '@/components/full-text-reader.vue';

/** Build a base article with full-text attached (override per test). */
function makeArticleWithFullText(overrides: Partial<Article> = {}): Article {
  return makeArticle({
    id: 'a1',
    hasFullText: true,
    fullTextFileName: 'paper.pdf',
    fullText: 'Extracted body text from the database.',
    ...overrides,
  });
}

function mountReader(articleOverrides: Partial<Article> = {}) {
  return mount(FullTextReader, {
    props: {
      article: makeArticleWithFullText(articleOverrides),
      fullScreen: false,
      canRequestAiSummary: false,
      isAiSummaryPending: false,
      openReaderId: null,
    },
  });
}

describe('full-text-reader.vue - fallback banner', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    mockTauriCommand.mockReset();
  });

  it('shows the fallback banner with the expected path when the PDF is missing from disk', async () => {
    /* `get_full_text_file_path` returns the expected (but missing) path even
       when the file does not exist on disk; `read_full_text_file_bytes`
       returns null because the file is absent. */
    const expectedPath = '/home/user/Documents/Bango/fulltext/paper.pdf';
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_full_text_file_path') return Promise.resolve(expectedPath);
      if (cmd === 'read_full_text_file_bytes') return Promise.resolve(null);
      return Promise.resolve(null);
    });

    const wrapper = mountReader();
    await (wrapper.vm as unknown as { openFullTextView: () => Promise<void> }).openFullTextView();
    await flushPromises();

    const banner = wrapper.find('.bg-amber-50');
    expect(banner.exists()).toBe(true);
    expect(banner.text()).toContain('Displaying Fallback Data');
    expect(banner.text()).toContain(expectedPath);
  });

  it('does NOT show the banner when the PDF bytes load successfully', async () => {
    /* A small valid PDF byte payload (does not need to be a real PDF; the
       component only wraps it in a Blob + object URL). */
    const fakeBytes = new Uint8Array([0x25, 0x50, 0x44, 0x46]);
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_full_text_file_path') return Promise.resolve('/path/to/paper.pdf');
      if (cmd === 'read_full_text_file_bytes') return Promise.resolve(fakeBytes);
      return Promise.resolve(null);
    });

    const wrapper = mountReader();
    await (wrapper.vm as unknown as { openFullTextView: () => Promise<void> }).openFullTextView();
    await flushPromises();

    expect(wrapper.find('.bg-amber-50').exists()).toBe(false);
    /* The inline PDF iframe should be rendered instead. */
    expect(wrapper.find('iframe[title="PDF Viewer"]').exists()).toBe(true);
  });

  it('does NOT show the banner for a non-PDF (txt) attachment', async () => {
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_full_text_file_path') return Promise.resolve('/path/to/notes.txt');
      if (cmd === 'read_full_text_file_bytes') return Promise.resolve(null);
      return Promise.resolve(null);
    });

    const wrapper = mountReader({ fullTextFileName: 'notes.txt' });
    await (wrapper.vm as unknown as { openFullTextView: () => Promise<void> }).openFullTextView();
    await flushPromises();

    expect(wrapper.find('.bg-amber-50').exists()).toBe(false);
  });

  it('clears the banner when the reader is closed', async () => {
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_full_text_file_path') return Promise.resolve('/path/to/paper.pdf');
      if (cmd === 'read_full_text_file_bytes') return Promise.resolve(null);
      return Promise.resolve(null);
    });

    const wrapper = mountReader();
    const vm = wrapper.vm as unknown as { openFullTextView: () => Promise<void> };
    await vm.openFullTextView();
    await flushPromises();
    expect(wrapper.find('.bg-amber-50').exists()).toBe(true);

    /* Close the reader via the close button. */
    const closeBtn = wrapper.find('button[title="Close full text view"]');
    expect(closeBtn.exists()).toBe(true);
    await closeBtn.trigger('click');

    /* The whole overlay (including the banner) unmounts on close. */
    expect(wrapper.find('.bg-amber-50').exists()).toBe(false);
  });
});
