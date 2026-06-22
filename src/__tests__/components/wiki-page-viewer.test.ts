import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { flushPromises } from '@vue/test-utils';

const mockTauriCommand = vi.fn();
vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: (...args: unknown[]) => mockTauriCommand(...args),
}));

// Mock marked to avoid HTML parsing complexity in tests
vi.mock('marked', () => ({
  marked: {
    parse: (input: string) => input.replace(/<!--[^>]*-->/g, '').trim(),
  },
}));

import WikiPageViewer from '@/components/wiki/wiki-page-viewer.vue';

describe('wiki-page-viewer.vue', () => {
  beforeEach(() => {
    // Default mock: route by command name. listSources returns [], others return null.
    mockTauriCommand.mockReset();
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'wiki_list_sources') return Promise.resolve([]);
      return Promise.resolve(null);
    });
  });

  afterEach(() => {
    mockTauriCommand.mockReset();
  });

  it('shows empty state when no slug is provided', async () => {
    mockTauriCommand.mockResolvedValue(null);
    const wrapper = mount(WikiPageViewer, {
      props: { slug: null },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Select a page to read.');
  });

  it('loads and displays a page', async () => {
    const page = {
      slug: 'sugar-tax',
      title: 'Sugar Tax',
      pageType: 'concept',
      status: 'draft',
      summary: 'A levy on sugary drinks',
      body: '# Sugar Tax\nA tax on beverages. See [[obesity]].',
      filePath: 'wiki/concepts/sugar-tax.md',
      sourceArticles: null,
    };
    mockTauriCommand.mockResolvedValue(page);
    const wrapper = mount(WikiPageViewer, {
      props: { slug: 'sugar-tax' },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Sugar Tax');
    expect(wrapper.text()).toContain('concept');
    expect(wrapper.text()).toContain('A levy on sugary drinks');
    // The body should be rendered (wikilink converted to a link).
    expect(wrapper.html()).toContain('wikilink');
    expect(wrapper.html()).toContain('data-slug="obesity"');
  });

  it('shows error when page is not found', async () => {
    mockTauriCommand.mockResolvedValue(null);
    const wrapper = mount(WikiPageViewer, {
      props: { slug: 'nonexistent' },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('not found');
  });

  it('shows error on command failure', async () => {
    mockTauriCommand.mockRejectedValue(new Error('IPC failed'));
    const wrapper = mount(WikiPageViewer, {
      props: { slug: 'error-page' },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('IPC failed');
  });

  it('shows reviewed badge for reviewed pages', async () => {
    const page = {
      slug: 'reviewed-page',
      title: 'Reviewed',
      pageType: 'concept',
      status: 'reviewed',
      summary: '',
      body: '# Reviewed',
      filePath: '',
      sourceArticles: null,
    };
    mockTauriCommand.mockResolvedValue(page);
    const wrapper = mount(WikiPageViewer, {
      props: { slug: 'reviewed-page' },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('reviewed');
  });

  it('emits close event when close button is clicked', async () => {
    const page = {
      slug: 'test',
      title: 'Test',
      pageType: 'concept',
      status: 'draft',
      summary: '',
      body: '# Test',
      filePath: '',
      sourceArticles: null,
    };
    mockTauriCommand.mockResolvedValue(page);
    const wrapper = mount(WikiPageViewer, {
      props: { slug: 'test' },
    });
    await flushPromises();
    const closeBtn = wrapper.find('.wiki-page-viewer__close');
    expect(closeBtn.exists()).toBe(true);
    await closeBtn.trigger('click');
    expect(wrapper.emitted('close')).toBeTruthy();
  });

  it('reloads when slug prop changes', async () => {
    const page1 = {
      slug: 'alpha',
      title: 'Alpha',
      pageType: 'concept',
      status: 'draft',
      summary: '',
      body: '# Alpha',
      filePath: '',
      sourceArticles: null,
    };
    const page2 = {
      slug: 'beta',
      title: 'Beta',
      pageType: 'concept',
      status: 'draft',
      summary: '',
      body: '# Beta',
      filePath: '',
      sourceArticles: null,
    };
    let currentSlug = 'alpha';
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'wiki_list_sources') return Promise.resolve([]);
      if (cmd === 'wiki_get_page') {
        return Promise.resolve(currentSlug === 'alpha' ? page1 : page2);
      }
      return Promise.resolve(null);
    });
    const wrapper = mount(WikiPageViewer, {
      props: { slug: 'alpha' },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Alpha');

    currentSlug = 'beta';
    await wrapper.setProps({ slug: 'beta' });
    await flushPromises();
    expect(wrapper.text()).toContain('Beta');
  });

  it('converts [[slug|alias]] wikilinks with alias text', async () => {
    const page = {
      slug: 'test',
      title: 'Test',
      pageType: 'concept',
      status: 'draft',
      summary: '',
      body: 'See [[obesity|the obesity crisis]] for details.',
      filePath: '',
      sourceArticles: null,
    };
    mockTauriCommand.mockResolvedValue(page);
    const wrapper = mount(WikiPageViewer, {
      props: { slug: 'test' },
    });
    await flushPromises();
    expect(wrapper.html()).toContain('data-slug="obesity"');
    expect(wrapper.html()).toContain('the obesity crisis');
  });
});
