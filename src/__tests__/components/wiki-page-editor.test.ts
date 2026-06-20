import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

const mockTauriCommand = vi.fn();
vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: (...args: unknown[]) => mockTauriCommand(...args),
}));

// Mock marked to keep preview assertions simple.
vi.mock('marked', () => ({
  marked: {
    parse: (input: string) => `<p>${input}</p>`,
  },
}));

import WikiPageEditor from '@/components/wiki/wiki-page-editor.vue';
import type { WikiPage } from '@/types/wiki';

function makePage(overrides: Partial<WikiPage> = {}): WikiPage {
  return {
    slug: 'alpha',
    title: 'Alpha',
    pageType: 'concept',
    status: 'draft',
    summary: 'alpha summary',
    body: '# Alpha\n\nInitial body.',
    filePath: 'wiki/concepts/alpha.md',
    sourceArticles: null,
    ...overrides,
  };
}

describe('wiki-page-editor.vue', () => {
  beforeEach(() => {
    mockTauriCommand.mockReset();
  });

  afterEach(() => {
    mockTauriCommand.mockReset();
  });

  it('shows empty state when slug is null', async () => {
    const wrapper = mount(WikiPageEditor, { props: { slug: null } });
    await flushPromises();
    expect(wrapper.text()).toContain('No page selected.');
  });

  it('loads the page fields on mount', async () => {
    const page = makePage();
    mockTauriCommand.mockResolvedValue(page);
    const wrapper = mount(WikiPageEditor, { props: { slug: 'alpha' } });
    await flushPromises();
    // Title + summary inputs carry the loaded values. happy-dom exposes the
    // bound value via the element's `value` property (not the `value` attr).
    const titleInput = wrapper.find('#wiki-title').element as HTMLInputElement;
    const summaryInput = wrapper.find('#wiki-summary').element as HTMLInputElement;
    expect(titleInput.value).toBe('Alpha');
    expect(summaryInput.value).toBe('alpha summary');
    // Toolbar shows the loaded title.
    expect(wrapper.text()).toContain('Edit: Alpha');
  });

  it('shows a not-found error when getPage returns null', async () => {
    mockTauriCommand.mockResolvedValue(null);
    const wrapper = mount(WikiPageEditor, { props: { slug: 'missing' } });
    await flushPromises();
    expect(wrapper.text()).toContain('not found');
  });

  it('shows error message when load fails', async () => {
    mockTauriCommand.mockRejectedValue(new Error('boom'));
    const wrapper = mount(WikiPageEditor, { props: { slug: 'bad' } });
    await flushPromises();
    expect(wrapper.text()).toContain('boom');
  });

  it('disables Save when the form is clean (no changes)', async () => {
    const page = makePage();
    mockTauriCommand.mockResolvedValue(page);
    const wrapper = mount(WikiPageEditor, { props: { slug: 'alpha' } });
    await flushPromises();
    const saveBtn = wrapper.find('.btn--primary');
    expect(saveBtn.exists()).toBe(true);
    expect(saveBtn.attributes('disabled')).toBeDefined();
  });

  it('enables Save when the body is dirty', async () => {
    const page = makePage();
    mockTauriCommand.mockResolvedValue(page);
    const wrapper = mount(WikiPageEditor, { props: { slug: 'alpha' } });
    await flushPromises();
    const saveBtn = wrapper.find('.btn--primary');
    expect(saveBtn.attributes('disabled')).toBeDefined();

    // Mutate the body textarea; Save should become enabled.
    await wrapper.find('textarea').setValue('# Alpha v2\n\nEdited body.');
    expect(saveBtn.attributes('disabled')).toBeUndefined();
  });

  it('calls wiki_update_page and emits saved on Save click', async () => {
    const original = makePage();
    const updated = makePage({ title: 'Alpha v2', summary: 'new summary', body: '# Alpha v2' });
    // 1st call: getPage on mount. 2nd call: updatePage on save.
    mockTauriCommand.mockResolvedValueOnce(original).mockResolvedValueOnce(updated);

    const wrapper = mount(WikiPageEditor, { props: { slug: 'alpha' } });
    await flushPromises();

    // Edit the title and summary so the form is dirty.
    await wrapper.find('#wiki-title').setValue('Alpha v2');
    await wrapper.find('#wiki-summary').setValue('new summary');
    await wrapper.find('textarea').setValue('# Alpha v2');

    await wrapper.find('.btn--primary').trigger('click');
    await flushPromises();

    expect(mockTauriCommand).toHaveBeenLastCalledWith('wiki_update_page', {
      slug: 'alpha',
      title: 'Alpha v2',
      summary: 'new summary',
      body: '# Alpha v2',
    });
    const saved = wrapper.emitted('saved');
    expect(saved).toBeTruthy();
    expect((saved?.[0]?.[0] as WikiPage).title).toBe('Alpha v2');
  });

  it('emits cancel when Cancel is clicked', async () => {
    mockTauriCommand.mockResolvedValue(makePage());
    const wrapper = mount(WikiPageEditor, { props: { slug: 'alpha' } });
    await flushPromises();
    await wrapper.find('.btn--secondary').trigger('click');
    expect(wrapper.emitted('cancel')).toBeTruthy();
  });

  it('shows error when save fails', async () => {
    mockTauriCommand
      .mockResolvedValueOnce(makePage())
      .mockRejectedValueOnce(new Error('save fail'));
    const wrapper = mount(WikiPageEditor, { props: { slug: 'alpha' } });
    await flushPromises();
    // Make the form dirty.
    await wrapper.find('textarea').setValue('changed body');
    await wrapper.find('.btn--primary').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('save fail');
  });

  it('reloads when the slug prop changes', async () => {
    const page1 = makePage({ slug: 'alpha', title: 'Alpha' });
    const page2 = makePage({ slug: 'beta', title: 'Beta' });
    let current = 'alpha';
    mockTauriCommand.mockImplementation(() => Promise.resolve(current === 'alpha' ? page1 : page2));

    const wrapper = mount(WikiPageEditor, { props: { slug: 'alpha' } });
    await flushPromises();
    expect(wrapper.text()).toContain('Edit: Alpha');

    current = 'beta';
    await wrapper.setProps({ slug: 'beta' });
    await flushPromises();
    expect(wrapper.text()).toContain('Edit: Beta');
  });

  it('renders the preview pane from the body via marked', async () => {
    mockTauriCommand.mockResolvedValue(makePage({ body: 'preview me' }));
    const wrapper = mount(WikiPageEditor, { props: { slug: 'alpha' } });
    await flushPromises();
    // The mocked marked wraps the body in <p>...</p>.
    expect(wrapper.html()).toContain('preview me');
  });
});
