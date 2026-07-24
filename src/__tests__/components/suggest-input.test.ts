import { describe, it, expect } from 'vitest';
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
  clearOnSelect = true
) {
  let currentValue = initialValue;
  const wrapper = mount(SuggestInput, {
    props: {
      modelValue: currentValue,
      suggestions,
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

  // ── Reset-to-dropdown-open behavior (the core change) ────────────

  it('clears the input and keeps the dropdown open after selecting a suggestion', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.find('input').setValue('Beta');
    await syncModel();

    // Click the "Beta" suggestion
    await wrapper.findAll('li')[0]!.trigger('mousedown');
    await syncModel();

    // The select event should have fired with "Beta"...
    expect(wrapper.emitted('select')).toEqual([['Beta']]);

    // The input should be cleared (last update:modelValue is '')...
    const updates = wrapper.emitted('update:modelValue');
    expect(updates).toBeDefined();
    const lastUpdate = updates![updates!.length - 1];
    expect(lastUpdate).toEqual(['']);

    // The dropdown should still be open (3 suggestions visible).
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(3);
  });

  it('clears the input and keeps the dropdown open after pressing Enter', async () => {
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

    // The dropdown should still be open.
    await flushPromises();
    expect(wrapper.findAll('li')).toHaveLength(3);
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

  it('shows all suggestions when the input is cleared after a selection', async () => {
    const { wrapper, syncModel } = mountWithVModel();
    await wrapper.find('input').trigger('focus');
    await flushPromises();
    await wrapper.find('input').setValue('Alpha');
    await syncModel();
    await wrapper.findAll('li')[0]!.trigger('mousedown');
    await syncModel();

    // All 3 suggestions should be visible (the input is empty, so no filter)
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

    // The select event should have fired with "Beta"...
    expect(wrapper.emitted('select')).toEqual([['Beta']]);

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
});
