import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import TagLabelPanel from '@/components/tag-label-panel.vue';
import type { TagWithCount } from '@/types';

function makeTag(overrides: Partial<TagWithCount> = {}): TagWithCount {
  return {
    id: 't1',
    name: 'machine-learning',
    source: 'user_created',
    color: '#3b82f6',
    articleCount: 5,
    ...overrides,
  };
}

function mountPanel(props: Partial<InstanceType<typeof TagLabelPanel>['$props']> = {}) {
  return mount(TagLabelPanel, {
    props: { kind: 'tag', items: [makeTag()], ...props } as never,
    global: {
      stubs: { TagChip: true, LabelChip: true },
    },
  });
}

describe('tag-label-panel.vue', () => {
  it('renders the panel title and count badge', () => {
    const wrapper = mountPanel({ items: [makeTag(), makeTag({ id: 't2' })] });
    expect(wrapper.text()).toContain('Tags');
    expect(wrapper.text()).toContain('2 Total');
  });

  it('renders the subtitle for the tag kind', () => {
    const wrapper = mountPanel({ kind: 'tag', items: [] });
    expect(wrapper.text()).toContain('Content-category labels');
  });

  it('renders the subtitle for the label kind', () => {
    const wrapper = mountPanel({ kind: 'label', items: [] });
    expect(wrapper.text()).toContain('Workflow markers');
  });

  it('shows the Suggest with AI button in the header', () => {
    const wrapper = mountPanel();
    const suggestBtn = wrapper.find('.ai-btn');
    expect(suggestBtn.exists()).toBe(true);
    expect(suggestBtn.text()).toContain('Suggest with AI');
  });

  it('shows the Suggesting state when suggesting is true', () => {
    const wrapper = mountPanel({ suggesting: true });
    expect(wrapper.find('.ai-btn').text()).toContain('Suggesting');
    expect(wrapper.find('.ai-btn').attributes('disabled')).toBeDefined();
  });

  it('emits "suggest" when the Suggest button is clicked', async () => {
    const wrapper = mountPanel();
    await wrapper.find('.ai-btn').trigger('click');
    expect(wrapper.emitted('suggest')).toBeTruthy();
    expect(wrapper.emitted('suggest')!.length).toBe(1);
  });

  it('renders an Add button next to the input, disabled when input is empty', () => {
    const wrapper = mountPanel();
    const addBtn = wrapper.find('button.btn-primary-sm');
    expect(addBtn.exists()).toBe(true);
    expect(addBtn.text()).toBe('Add');
    expect(addBtn.attributes('disabled')).toBeDefined();
  });

  it('emits "create" with the trimmed name when Add is clicked', async () => {
    const wrapper = mountPanel();
    const input = wrapper.find('input[type="text"]');
    await input.setValue('  new-tag  ');
    await wrapper.find('button.btn-primary-sm').trigger('click');
    const emitted = wrapper.emitted('create');
    expect(emitted).toBeTruthy();
    expect(emitted![0]![0]).toBe('new-tag');
    // Input is cleared after commit.
    expect((input.element as HTMLInputElement).value).toBe('');
  });

  it('emits "create" on Enter key in the input', async () => {
    const wrapper = mountPanel();
    const input = wrapper.find('input[type="text"]');
    await input.setValue('enter-tag');
    await input.trigger('keyup.enter');
    expect(wrapper.emitted('create')![0]![0]).toBe('enter-tag');
  });

  it('does not emit "create" for empty/whitespace input', async () => {
    const wrapper = mountPanel();
    const input = wrapper.find('input[type="text"]');
    await input.setValue('   ');
    await input.trigger('keyup.enter');
    expect(wrapper.emitted('create')).toBeFalsy();
  });

  it('renders the empty-state message when items is empty', () => {
    const wrapper = mountPanel({ kind: 'label', items: [] });
    expect(wrapper.text()).toContain('No labels yet.');
  });

  it('emits "delete" when the delete button is clicked', async () => {
    const wrapper = mountPanel({ items: [makeTag({ id: 't1' })] });
    // The delete button is the last button with title "Delete tag".
    const deleteBtn = wrapper.find('button[aria-label="Delete tag"]');
    expect(deleteBtn.exists()).toBe(true);
    await deleteBtn.trigger('click');
    expect(wrapper.emitted('delete')![0]![0]).toBe('t1');
  });

  it('emits "filter" when the filter button is clicked and articleCount > 0', async () => {
    const wrapper = mountPanel({ items: [makeTag({ id: 't1', articleCount: 3 })] });
    const filterBtn = wrapper.find('button[title="see assigned"]');
    await filterBtn.trigger('click');
    expect(wrapper.emitted('filter')![0]![0]).toBe('t1');
  });

  it('disables the filter button when articleCount is 0', () => {
    const wrapper = mountPanel({ items: [makeTag({ id: 't1', articleCount: 0 })] });
    const filterBtn = wrapper.find('button[title="not assigned"]');
    expect(filterBtn.attributes('disabled')).toBeDefined();
  });

  it('enters edit mode on double-click and emits "rename" on commit', async () => {
    const wrapper = mountPanel({ items: [makeTag({ id: 't1', name: 'old-name' })] });
    // Double-click the chip wrapper span to enter edit mode.
    await wrapper.find('[title="Double-click to edit"]').trigger('dblclick');
    // The edit input is selectable by its dedicated class.
    const editEl = wrapper.find('input.tlp-edit-input');
    expect(editEl.exists()).toBe(true);
    await editEl.setValue('new-name');
    await editEl.trigger('keyup.enter');
    expect(wrapper.emitted('rename')![0]).toEqual(['t1', 'new-name']);
  });

  it('cancels edit on Escape without emitting rename', async () => {
    const wrapper = mountPanel({ items: [makeTag({ id: 't1', name: 'old-name' })] });
    await wrapper.find('[title="Double-click to edit"]').trigger('dblclick');
    const editEl = wrapper.find('input.tlp-edit-input');
    await editEl.setValue('discarded');
    await editEl.trigger('keyup.escape');
    expect(wrapper.emitted('rename')).toBeFalsy();
  });
});
