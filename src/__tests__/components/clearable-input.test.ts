import { describe, it, expect, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import ClearableInput from '@/components/clearable-input.vue';

describe('clearable-input.vue', () => {
  beforeEach(() => {
    // No Pinia/router needed; component is pure.
  });

  it('renders the placeholder and forwards the value', () => {
    const wrapper = mount(ClearableInput, {
      props: { modelValue: 'hello', placeholder: 'Type here...' },
    });
    const input = wrapper.find('input');
    expect(input.attributes('placeholder')).toBe('Type here...');
    expect((input.element as HTMLInputElement).value).toBe('hello');
  });

  it('hides the clear button when the value is empty', () => {
    const wrapper = mount(ClearableInput, { props: { modelValue: '' } });
    expect(wrapper.find('.clearable-input__clear').exists()).toBe(false);
  });

  it('shows the clear button when the value is set', () => {
    const wrapper = mount(ClearableInput, { props: { modelValue: 'abc' } });
    expect(wrapper.find('.clearable-input__clear').exists()).toBe(true);
  });

  it('hides the clear button when disabled (DOI case)', () => {
    const wrapper = mount(ClearableInput, {
      props: { modelValue: 'abc', disabled: true },
    });
    expect(wrapper.find('.clearable-input__clear').exists()).toBe(false);
  });

  it('emits update:modelValue (with "") AND clear when the "x" is clicked', async () => {
    const wrapper = mount(ClearableInput, { props: { modelValue: 'abc' } });
    await wrapper.find('.clearable-input__clear').trigger('click');
    const updates = wrapper.emitted('update:modelValue');
    expect(updates).toBeTruthy();
    expect(updates![updates!.length - 1]!).toEqual(['']);
    expect(wrapper.emitted('clear')).toBeTruthy();
  });

  it('emits update:modelValue (with the typed value) on input, but NOT clear', async () => {
    const wrapper = mount(ClearableInput, { props: { modelValue: '' } });
    const input = wrapper.find('input');
    await input.setValue('typed');
    const updates = wrapper.emitted('update:modelValue');
    expect(updates).toBeTruthy();
    expect(updates![updates!.length - 1]!).toEqual(['typed']);
    // Typing must not fire `clear` - that is reserved for the "x" click.
    expect(wrapper.emitted('clear')).toBeFalsy();
  });

  it('forwards the enter event on keyup.enter', async () => {
    const wrapper = mount(ClearableInput, { props: { modelValue: 'x' } });
    await wrapper.find('input').trigger('keyup', { key: 'Enter' });
    expect(wrapper.emitted('enter')).toBeTruthy();
  });

  it('forwards focus and blur events', async () => {
    const wrapper = mount(ClearableInput, { props: { modelValue: 'x' } });
    const input = wrapper.find('input');
    await input.trigger('focus');
    expect(wrapper.emitted('focus')).toBeTruthy();
    await input.trigger('blur');
    expect(wrapper.emitted('blur')).toBeTruthy();
  });

  it('applies extra inputClass on the inner input', () => {
    const wrapper = mount(ClearableInput, {
      props: { modelValue: '', inputClass: 'my-extra-class' },
    });
    expect(wrapper.find('input').classes()).toContain('my-extra-class');
  });

  it('forwards type/min/max/title to the native input', () => {
    const wrapper = mount(ClearableInput, {
      props: {
        modelValue: '',
        type: 'number',
        min: 1850,
        max: 2100,
        title: 'Year hint',
      },
    });
    const input = wrapper.find('input');
    expect(input.attributes('type')).toBe('number');
    expect(input.attributes('min')).toBe('1850');
    expect(input.attributes('max')).toBe('2100');
    expect(input.attributes('title')).toBe('Year hint');
  });
});
