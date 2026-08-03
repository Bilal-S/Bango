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
    expect(wrapper.text()).toContain('Content labels for grouping');
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

  it('shows a confirmation dialog on delete, then emits "delete" on confirm', async () => {
    const wrapper = mountPanel({ items: [makeTag({ id: 't1' })] });
    // Clicking the row delete button opens the confirmation dialog instead
    // of emitting immediately.
    const deleteBtn = wrapper.find('button[aria-label="Delete tag"]');
    expect(deleteBtn.exists()).toBe(true);
    await deleteBtn.trigger('click');
    expect(wrapper.emitted('delete')).toBeFalsy();
    // The dialog is Teleported to document.body, so query there.
    expect(document.body.querySelector('.dialog__danger-box')).toBeTruthy();
    const confirmBtn = document.body.querySelector<HTMLButtonElement>('button.btn--danger');
    expect(confirmBtn).toBeTruthy();
    confirmBtn!.click();
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('delete')![0]![0]).toBe('t1');
  });

  it('cancels the delete confirmation without emitting', async () => {
    // Use a non-zero count so the confirmation dialog opens (zero-count
    // deletes skip the dialog - see the next test).
    const wrapper = mountPanel({ items: [makeTag({ id: 't1', articleCount: 5 })] });
    await wrapper.find('button[aria-label="Delete tag"]').trigger('click');
    // Cancel via the outline Cancel button (Teleported to body).
    const cancelBtn = document.body.querySelector<HTMLButtonElement>('button.btn--outline');
    expect(cancelBtn).toBeTruthy();
    cancelBtn!.click();
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('delete')).toBeFalsy();
    // Dialog is dismissed.
    expect(document.body.querySelector('.dialog__danger-box')).toBeNull();
  });

  it('skips the confirmation dialog and deletes immediately when articleCount is 0', async () => {
    const wrapper = mountPanel({ items: [makeTag({ id: 't1', articleCount: 0 })] });
    await wrapper.find('button[aria-label="Delete tag"]').trigger('click');
    // Emits immediately - no dialog opens.
    expect(wrapper.emitted('delete')![0]![0]).toBe('t1');
    expect(document.body.querySelector('.dialog__danger-box')).toBeNull();
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

  // ── Filter / sort sub-bar (Option A) ─────────────────────────────────
  describe('filter/sort sub-bar', () => {
    function makeTags(): TagWithCount[] {
      return [
        makeTag({ id: 'a', name: 'apple', articleCount: 5 }),
        makeTag({ id: 'b', name: 'banana', articleCount: 10 }),
        makeTag({ id: 'c', name: 'cherry', articleCount: 5 }),
        makeTag({ id: 'd', name: 'date', articleCount: 1 }),
      ];
    }

    it('renders the sticky sub-bar with both sort buttons and a caret', () => {
      const wrapper = mountPanel({ items: makeTags() });
      // Filter toggle, alpha sort, frequency sort, caret all present.
      expect(wrapper.find('button[aria-label="Filter tags"]').exists()).toBe(true);
      expect(wrapper.find('button[aria-pressed="true"]').exists()).toBe(true); // alpha default-active
      expect(wrapper.find('button[title="Sort by frequency (1-100)"]').exists()).toBe(true);
      expect(wrapper.find('button[aria-label="Expand filter"]').exists()).toBe(true);
    });

    it('the filter input is hidden until the bar is expanded', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      // No filter input rendered initially.
      expect(wrapper.find('input[placeholder="Filter tags..."]').exists()).toBe(false);
      await wrapper.find('button[aria-label="Expand filter"]').trigger('click');
      expect(wrapper.find('input[placeholder="Filter tags..."]').exists()).toBe(true);
    });

    it('typing in the filter input narrows the chip list', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      // Expand the filter row, type "an" -> only banana matches.
      await wrapper.find('button[aria-label="Expand filter"]').trigger('click');
      const input = wrapper.find('input[placeholder="Filter tags..."]');
      await input.setValue('an');
      // Stubbed TagChip renders nothing, so assert on the row count via the
      // rendered names. The chips are stubbed, but the editable wrapper span
      // carries the dblclick title; the underlying text is gone. Instead,
      // count the v-for rows by keying off the hover action bar rows.
      const rows = wrapper.findAll('.group.p-2');
      expect(rows).toHaveLength(1);
    });

    it('shows "No matching tags." when the filter matches nothing', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      await wrapper.find('button[aria-label="Expand filter"]').trigger('click');
      await wrapper.find('input[placeholder="Filter tags..."]').setValue('zzz');
      expect(wrapper.text()).toContain('No matching tags.');
    });

    it('keeps the header count badge at the total (4 Total) even when filtering', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      await wrapper.find('button[aria-label="Expand filter"]').trigger('click');
      await wrapper.find('input[placeholder="Filter tags..."]').setValue('an');
      // Header badge stays "4 Total".
      expect(wrapper.text()).toContain('4 Total');
      // The in-bar count line reflects the filtered count.
      expect(wrapper.text()).toContain('Showing 1 of 4');
    });

    it('shows "Showing X of N" only while a filter query is active', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      await wrapper.find('button[aria-label="Expand filter"]').trigger('click');
      expect(wrapper.text()).not.toContain('Showing');
      await wrapper.find('input[placeholder="Filter tags..."]').setValue('a');
      expect(wrapper.text()).toContain('Showing 3 of 4');
    });

    it('clearing the filter input restores all items', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      await wrapper.find('button[aria-label="Expand filter"]').trigger('click');
      const input = wrapper.find('input[placeholder="Filter tags..."]');
      await input.setValue('an');
      expect(wrapper.findAll('.group.p-2')).toHaveLength(1);
      // Click the ClearableInput "x" clear button.
      await wrapper.find('button[aria-label="Clear"]').trigger('click');
      expect(wrapper.findAll('.group.p-2')).toHaveLength(4);
    });

    it('clicking the active sort (alpha) flips direction A-Z -> Z-A', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      // Default: alpha asc ("Sorted A-Z. Click to reverse.").
      const alphaBtn = wrapper.find('button[title="Sorted A-Z. Click to reverse."]');
      expect(alphaBtn.exists()).toBe(true);
      await alphaBtn.trigger('click');
      // After toggle: desc ("Sorted Z-A. Click to reverse.").
      expect(wrapper.find('button[title="Sorted Z-A. Click to reverse."]').exists()).toBe(true);
    });

    it('clicking the inactive sort (frequency) switches active and resets to asc', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      const freqBtn = wrapper.find('button[title="Sort by frequency (1-100)"]');
      await freqBtn.trigger('click');
      // Now frequency is active at asc.
      expect(
        wrapper.find('button[title="Sorted 1-100 (smallest first). Click to reverse."]').exists()
      ).toBe(true);
      // Alpha is no longer the active sort - its title reverts to "Sort A-Z".
      expect(wrapper.find('button[title="Sort A-Z"]').exists()).toBe(true);
    });

    it('only one sort is active at a time (frequency active -> alpha inactive)', async () => {
      const wrapper = mountPanel({ items: makeTags() });
      await wrapper.find('button[title="Sort by frequency (1-100)"]').trigger('click');
      // Exactly one pressed sort button.
      const pressed = wrapper.findAll('button[aria-pressed="true"]');
      expect(pressed).toHaveLength(1);
    });

    it('uses label-specific copy ("No matching labels.", "Filter labels...") for kind=label', async () => {
      const wrapper = mountPanel({ kind: 'label', items: [] });
      // Empty state is the "no labels yet" copy when there are no items at all.
      expect(wrapper.text()).toContain('No labels yet.');
      // Provide items via remount to exercise the no-match path.
      const wrapper2 = mountPanel({
        kind: 'label',
        items: [makeTag({ id: 'l1', name: 'priority-read', articleCount: 3 }) as never],
      });
      await wrapper2.find('button[aria-label="Expand filter"]').trigger('click');
      await wrapper2.find('input[placeholder="Filter labels..."]').setValue('zzz');
      expect(wrapper2.text()).toContain('No matching labels.');
    });
  });
});
