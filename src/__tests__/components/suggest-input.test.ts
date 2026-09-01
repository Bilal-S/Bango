import { describe, it, expect, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import SuggestInput from '@/components/suggest-input.vue';

/**
 * Mount helper that simulates `v-model` two-way binding: when the component
 * emits `update:modelValue`, the parent (test) updates the prop so the
 * component sees its own change on the next read. Without this the
 * component's `props.modelValue` stays at the initial value and the
 * Enter / select handlers read stale state.
 */
function mountWithVModel(
  initialValue = '',
  suggestions = ['Alpha', 'Beta', 'Gamma'],
  clearOnSelect = true,
  disabledSuggestions: string[] = []
) {
  let currentValue = initialValue;
  const wrapper = mount(SuggestInput, {
    props: {
      modelValue: currentValue,
      suggestions,
      disabledSuggestions,
      placeholder: 'Add content tag…',
      clearOnSelect,
    },
    attachTo: document.body,
  });
  // Wire the v-model two-way binding: on each `update:modelValue`, advance the
  // prop so the component re-renders with the new value.
  wrapper.vm.$nextTick = wrapper.vm.$nextTick.bind(wrapper.vm);
  return {
    wrapper,
    async syncModel() {
      const emitted = wrapper.emitted('update:modelValue');
      if (emitted && emitted.length > 0) {
        const last = emitted[emitted.length - 1];
        if (last && typeof last[0] === 'string') {
          currentValue = last[0];
          await wrapper.setProps({ modelValue: currentValue });
        }
      }
    },
  };
}

describe('suggest-input.vue', () => {
  it('renders the placeholder text', () => {
    const { wrapper } = mountWithVModel();
    expect(wrapper.find('input').attributes('placeholder')).toBe('Add content tag…');
  });

  it('opens the dropdown on focus', async () => {
    const { wrapper } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(3);
  });

  it('filters suggestions as the user types', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await wrapper.find('input').setValue('alp');
    await syncModel();
    expect(wrapper.findAll('li')).toHaveLength(1);
    expect(wrapper.findAll('li')[0]!.text()).toBe('Alpha');
  });

  // ── Reset (collapse + clear) behavior on selection ────────────────

  it('clears the input and collapses the dropdown after selecting a suggestion', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.find('input').setValue('Beta');
    await syncModel();

    // Click the "Beta" suggestion
    await wrapper.findAll('li')[0]!.trigger('mousedown');
    await syncModel();

    // The select event should have fired with "Beta" (string mode emits
    // `[name, undefined]`; the second arg is the structured option, absent here).
    expect(wrapper.emitted('select')).toEqual([['Beta', undefined]]);

    // The input should be cleared (last update:modelValue is '')...
    const updates = wrapper.emitted('update:modelValue');
    expect(updates).toBeDefined();
    const lastUpdate = updates![updates!.length - 1];
    expect(lastUpdate).toEqual(['']);

    // The dropdown should be collapsed so it never obscures surrounding UI.
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(0);
  });

  it('clears the input and collapses the dropdown after pressing Enter', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.find('input').setValue('Gamma');
    await syncModel();

    await wrapper.find('input').trigger('keydown', { key: 'Enter' });
    await syncModel();

    // The enter event should have fired with "Gamma"...
    expect(wrapper.emitted('enter')).toEqual([['Gamma']]);

    // The input should be cleared (last update:modelValue is '')...
    const updates = wrapper.emitted('update:modelValue');
    expect(updates).toBeDefined();
    const lastUpdate = updates![updates!.length - 1];
    expect(lastUpdate).toEqual(['']);

    // The dropdown should be collapsed.
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(0);
  });

  it('reopens the dropdown on the next input after a collapsed selection', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.findAll('li')[0]!.trigger('mousedown');
    await syncModel();
    expect(wrapper.findAll('li')).toHaveLength(0);

    // Focus persists in the input; typing reopens for the next pick.
    await wrapper.find('input').setValue('alp');
    await syncModel();
    expect(wrapper.findAll('li')).toHaveLength(1);
  });

  // ── Auto-flip: open upward when space below is insufficient ──────

  it('opens the dropdown upward when there is not enough space below the input', async () => {
    vi.stubGlobal('innerHeight', 768);
    const { wrapper } = mountWithVModel();
    const input = wrapper.find('input').element;
    // Input near the viewport bottom: 28px below, 700px above -> flip up.
    const rectSpy = vi.spyOn(input, 'getBoundingClientRect').mockReturnValue({
      top: 700,
      bottom: 740,
      height: 40,
      width: 200,
      left: 0,
      right: 200,
      x: 0,
      y: 700,
      toJSON: () => ({}),
    } as DOMRect);
    await wrapper.find('input').trigger('focus');
    expect(wrapper.find('ul').classes()).toContain('bottom-[calc(100%+4px)]');
    rectSpy.mockRestore();
    vi.unstubAllGlobals();
  });

  it('opens the dropdown downward when there is enough space below the input', async () => {
    vi.stubGlobal('innerHeight', 768);
    const { wrapper } = mountWithVModel();
    const input = wrapper.find('input').element;
    // Input near the top: 628px below -> stays down.
    const rectSpy = vi.spyOn(input, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 140,
      height: 40,
      width: 200,
      left: 0,
      right: 200,
      x: 0,
      y: 100,
      toJSON: () => ({}),
    } as DOMRect);
    await wrapper.find('input').trigger('focus');
    expect(wrapper.find('ul').classes()).not.toContain('bottom-[calc(100%+4px)]');
    rectSpy.mockRestore();
    vi.unstubAllGlobals();
  });

  it('does not fire enter when the input is empty', async () => {
    const { wrapper } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await wrapper.find('input').trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('enter')).toBeUndefined();
  });

  it('closes the dropdown on Escape', async () => {
    const { wrapper } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(3);

    await wrapper.find('input').trigger('keydown', { key: 'Escape' });
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(0);
  });

  it('closes the dropdown when clicking outside', async () => {
    const { wrapper } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(3);

    // Simulate a click outside the component (on the body element).
    const outside = document.createElement('div');
    document.body.appendChild(outside);
    outside.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(0);
    document.body.removeChild(outside);
  });

  it('shows all suggestions when the dropdown reopens after a selection', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.find('input').setValue('Alpha');
    await syncModel();
    await wrapper.findAll('li')[0]!.trigger('mousedown');
    await syncModel();

    // Selection collapses the dropdown (reset behavior)...
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(0);

    // ...and clicking the still-empty input reopens the full list (no filter).
    await wrapper.find('input').trigger('click');
    await flushPromises();
    const items = wrapper.findAll('li');
    expect(items).toHaveLength(3);
    expect(items[0]!.text()).toBe('Alpha');
  });

  // ── Single-select mode (`clearOnSelect: false`) ──────────────────
  // Used by the bulk add-tag / add-label dialogs in `article-list.vue`,
  // where the user picks exactly one value, sees it in the input, then
  // confirms via a separate action button gated on `modelValue.trim()`.

  it('with clearOnSelect false, selecting a suggestion populates the input and closes the dropdown', async () => {
    const { wrapper, syncModel } = mountWithVModel('', undefined, false);
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.find('input').setValue('Beta');
    await syncModel();

    await wrapper.findAll('li')[0]!.trigger('mousedown');
    await syncModel();

    // The select event should have fired with "Beta" (string mode: second arg undefined).
    expect(wrapper.emitted('select')).toEqual([['Beta', undefined]]);

    // The input should now hold the selected value (NOT cleared)...
    const updates = wrapper.emitted('update:modelValue');
    expect(updates).toBeDefined();
    const lastUpdate = updates![updates!.length - 1];
    expect(lastUpdate).toEqual(['Beta']);

    // The dropdown should be closed (0 <li> rendered).
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(0);
  });

  it('with clearOnSelect false, Enter does not clear the input', async () => {
    const { wrapper, syncModel } = mountWithVModel('', undefined, false);
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.find('input').setValue('Gamma');
    await syncModel();

    await wrapper.find('input').trigger('keydown', { key: 'Enter' });
    await syncModel();

    // The enter event should have fired with "Gamma"...
    expect(wrapper.emitted('enter')).toEqual([['Gamma']]);

    // The input should still hold "Gamma" (NOT cleared) so the parent's
    // confirm button - typically gated on `modelValue.trim()` - stays enabled.
    const updates = wrapper.emitted('update:modelValue');
    expect(updates).toBeDefined();
    const lastUpdate = updates![updates!.length - 1];
    expect(lastUpdate).toEqual(['Gamma']);
  });

  // ── Matched-substring `<mark>` highlighting ────────────────────────
  // Typing a substring wraps the matched portion of each visible suggestion
  // in an indigo `<mark>` so the user can see *which* part of the name
  // matched. Reinforces that matching is substring-based (any part).

  it('wraps the matched substring in a <mark> element when typing', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    // "lph" matches inside "Alpha" (not a prefix) -> the substring feature
    // is what we are exercising here.
    await wrapper.find('input').setValue('lph');
    await syncModel();

    const items = wrapper.findAll('li');
    expect(items).toHaveLength(1);
    const mark = items[0]!.find('mark');
    expect(mark.exists()).toBe(true);
    expect(mark.text()).toBe('lph');
  });

  it('does not render a <mark> when there is no query', async () => {
    const { wrapper } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    // No query typed -> plain text, no <mark> anywhere.
    expect(wrapper.findAll('mark')).toHaveLength(0);
  });

  it('highlights the match case-insensitively', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    // Uppercase query still matches lowercase suggestion text; the <mark>
    // preserves the original casing of the suggestion.
    await wrapper.find('input').setValue('BET');
    await syncModel();

    const items = wrapper.findAll('li');
    expect(items).toHaveLength(1);
    const mark = items[0]!.find('mark');
    expect(mark.exists()).toBe(true);
    // The matched span keeps the suggestion's original casing ("Bet").
    expect(mark.text()).toBe('Bet');
  });

  // ── Disabled (already-assigned) rows ───────────────────────────────
  // Values in `disabledSuggestions` render in the dropdown but greyed out
  // and unselectable, so the user can see they exist without being able to
  // re-add them.

  it('renders disabled rows with the disabled styling and check icon', async () => {
    const { wrapper } = mountWithVModel('', ['Alpha', 'Beta', 'Gamma'], true, ['Beta']);
    await wrapper.find('input').trigger('focus');
    await flushPromises();

    const items = wrapper.findAll('li');
    expect(items).toHaveLength(3);
    // The "Beta" row carries the disabled cursor class.
    const betaItem = items[1]!;
    expect(betaItem.classes()).toContain('cursor-not-allowed');
    expect(betaItem.classes()).toContain('text-slate-400');
    // A check icon is rendered to signal "already added".
    expect(betaItem.find('.material-symbols-outlined').exists()).toBe(true);
    // The non-disabled rows stay interactive.
    expect(items[0]!.classes()).toContain('cursor-pointer');
    expect(items[2]!.classes()).toContain('cursor-pointer');
  });

  it('does not emit select when a disabled row is clicked', async () => {
    const { wrapper } = mountWithVModel('', ['Alpha', 'Beta', 'Gamma'], true, ['Beta']);
    await wrapper.find('input').trigger('focus');
    await flushPromises();

    const items = wrapper.findAll('li');
    await items[1]!.trigger('mousedown');
    await flushPromises();

    // No select event should fire for the disabled row.
    expect(wrapper.emitted('select')).toBeUndefined();
  });

  it('still emits select when a non-disabled row is clicked alongside a disabled one', async () => {
    const { wrapper } = mountWithVModel('', ['Alpha', 'Beta', 'Gamma'], true, ['Beta']);
    await wrapper.find('input').trigger('focus');
    await flushPromises();

    const items = wrapper.findAll('li');
    await items[0]!.trigger('mousedown');
    await flushPromises();

    expect(wrapper.emitted('select')).toEqual([['Alpha', undefined]]);
  });

  it('shows the "Already added." hint when the typed value matches a disabled row', async () => {
    const { wrapper, syncModel } = mountWithVModel('', ['Alpha', 'Beta', 'Gamma'], true, ['Beta']);
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.find('input').setValue('Beta');
    await syncModel();

    // The italic hint paragraph should now be present.
    const hint = wrapper.find('p.italic');
    expect(hint.exists()).toBe(true);
    expect(hint.text()).toContain('Already added.');
  });
});

/**
 * Structured-options mode (`options` prop) - used by the journal autocomplete.
 * Renders richer rows (label + sublabel + badge) and emits the full
 * `SuggestOption` object as the second `select` argument so the parent can
 * read the `id`.
 */
describe('suggest-input.vue (structured options mode)', () => {
  const journalOptions = [
    { id: 'j1', label: 'Nature', sublabel: 'Springer', badge: '1234-5678' },
    { id: 'j2', label: 'Nature Methods', sublabel: 'Nature Portfolio', badge: '1548-1404' },
  ];

  function mountWithOptions(initialValue = '', clearOnSelect = false) {
    let currentValue = initialValue;
    const wrapper = mount(SuggestInput, {
      props: {
        modelValue: currentValue,
        options: journalOptions,
        placeholder: 'Journal name',
        clearOnSelect,
      },
      attachTo: document.body,
    });
    return {
      wrapper,
      async syncModel() {
        const emitted = wrapper.emitted('update:modelValue');
        if (emitted && emitted.length > 0) {
          const last = emitted[emitted.length - 1];
          if (last && typeof last[0] === 'string') {
            currentValue = last[0];
            await wrapper.setProps({ modelValue: currentValue });
          }
        }
      },
    };
  }

  it('renders the structured rows with label, sublabel, and badge', async () => {
    const { wrapper } = mountWithOptions();
    await wrapper.find('input').trigger('focus');
    await flushPromises();

    const items = wrapper.findAll('li');
    expect(items).toHaveLength(2);
    // First row: Nature / Springer / 1234-5678
    expect(items[0]!.text()).toContain('Nature');
    expect(items[0]!.text()).toContain('Springer');
    expect(items[0]!.text()).toContain('1234-5678');
  });

  it('emits the full option object (with id) as the second select argument', async () => {
    const { wrapper } = mountWithOptions();
    await wrapper.find('input').trigger('focus');
    await flushPromises();

    const items = wrapper.findAll('li');
    await items[0]!.trigger('mousedown');
    await flushPromises();

    const selectEvents = wrapper.emitted('select');
    expect(selectEvents).toBeDefined();
    expect(selectEvents).toHaveLength(1);
    // First arg: label string; second arg: the option object.
    expect(selectEvents![0]![0]).toBe('Nature');
    expect(selectEvents![0]![1]).toMatchObject({ id: 'j1', label: 'Nature' });
  });

  it('filters options by label substring', async () => {
    const { wrapper, syncModel } = mountWithOptions();
    await wrapper.find('input').trigger('focus');
    await wrapper.find('input').setValue('Methods');
    await syncModel();
    await flushPromises();

    const items = wrapper.findAll('li');
    expect(items).toHaveLength(1);
    expect(items[0]!.text()).toContain('Nature Methods');
  });

  it('returns an empty list when no option matches the query', async () => {
    const { wrapper, syncModel } = mountWithOptions();
    await wrapper.find('input').trigger('focus');
    await wrapper.find('input').setValue('Quantum');
    await syncModel();
    await flushPromises();

    expect(wrapper.findAll('li')).toHaveLength(0);
  });
});
