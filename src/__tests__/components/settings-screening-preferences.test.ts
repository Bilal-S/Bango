import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { shimLocalStorage } from '../helpers/fixtures';

// Mock @tauri-apps/api/core so the component's `invoke` calls are captured.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock isTauri() to return true so the component exercises the invoke path
// (otherwise it short-circuits and keeps defaults for the unit-test env).
vi.mock('@/composables/use-tauri-command', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/composables/use-tauri-command')>();
  return {
    ...actual,
    isTauri: () => true,
  };
});

import SettingsScreeningPreferences from '@/components/settings/settings-screening-preferences.vue';

function mountCard() {
  setActivePinia(createPinia());
  return mount(SettingsScreeningPreferences, {
    global: { plugins: [createPinia()] },
  });
}

describe('settings-screening-preferences.vue', () => {
  beforeEach(() => {
    // happy-dom's localStorage lacks removeItem/clear; install a full shim so
    // the auto-navigate toggle read works without throwing.
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    mockInvoke.mockReset();
    // Default: abstract mode, no full-text articles.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_screening_mode') return Promise.resolve('abstract');
      if (cmd === 'get_full_text_article_count') return Promise.resolve(0);
      if (cmd === 'set_screening_mode') return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });
  });

  it('renders all three mode options as enabled when no full text is attached', async () => {
    const wrapper = mountCard();
    await flushPromises();

    const options = wrapper.findAll('option');
    expect(options).toHaveLength(3);
    // None of the options should carry the disabled attribute - modes are
    // always selectable regardless of attachments.
    for (const opt of options) {
      expect(opt.attributes('disabled')).toBeUndefined();
    }
    const values = options.map((o) => o.attributes('value'));
    expect(values).toEqual(['abstract', 'enhanced', 'two_stage']);
  });

  it('selecting Enhanced with zero full-text articles calls set_screening_mode (not blocked)', async () => {
    const wrapper = mountCard();
    await flushPromises();

    // Simulate the user picking Enhanced from the dropdown.
    await wrapper.find('select.mode-select').setValue('enhanced');
    await flushPromises();

    expect(mockInvoke).toHaveBeenCalledWith('set_screening_mode', { mode: 'enhanced' });
  });

  it('selecting Two-stage with zero full-text articles calls set_screening_mode (not blocked)', async () => {
    const wrapper = mountCard();
    await flushPromises();

    await wrapper.find('select.mode-select').setValue('two_stage');
    await flushPromises();

    expect(mockInvoke).toHaveBeenCalledWith('set_screening_mode', { mode: 'two_stage' });
  });

  it('shows the fallback notice when an advanced mode is active but no full text exists', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_screening_mode') return Promise.resolve('enhanced');
      if (cmd === 'get_full_text_article_count') return Promise.resolve(0);
      return Promise.resolve(undefined);
    });

    const wrapper = mountCard();
    await flushPromises();

    const fallback = wrapper.find('.mode-select__fallback');
    expect(fallback.exists()).toBe(true);
    expect(fallback.text().toLowerCase()).toContain('fall back to abstract-only');
    // The active notice must NOT also render.
    expect(wrapper.find('.mode-select__active').exists()).toBe(false);
  });

  it('shows the active notice when an advanced mode is active and full text exists', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_screening_mode') return Promise.resolve('two_stage');
      if (cmd === 'get_full_text_article_count') return Promise.resolve(3);
      return Promise.resolve(undefined);
    });

    const wrapper = mountCard();
    await flushPromises();

    const active = wrapper.find('.mode-select__active');
    expect(active.exists()).toBe(true);
    expect(active.text()).toContain('3');
    expect(active.text().toLowerCase()).toContain('evidence retrieval is active');
    // The fallback notice must NOT render when full text is present.
    expect(wrapper.find('.mode-select__fallback').exists()).toBe(false);
  });

  it('shows neither notice when abstract mode is active', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_screening_mode') return Promise.resolve('abstract');
      if (cmd === 'get_full_text_article_count') return Promise.resolve(0);
      return Promise.resolve(undefined);
    });

    const wrapper = mountCard();
    await flushPromises();

    expect(wrapper.find('.mode-select__fallback').exists()).toBe(false);
    expect(wrapper.find('.mode-select__active').exists()).toBe(false);
  });
});
