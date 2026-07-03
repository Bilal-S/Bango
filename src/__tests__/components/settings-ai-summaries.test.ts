import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { shimLocalStorage } from '../helpers/fixtures';

// Mock @tauri-apps/api/core so the component's `invoke` calls are captured.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock the isTauri() check so the component attempts the IPC path.
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
}));

import SettingsAiSummaries from '@/components/settings/settings-ai-summaries.vue';

function mountCard() {
  // The component uses `invoke` directly (not the store), but Pinia is
  // installed for parity with the rest of the app.
  setActivePinia(createPinia());
  return mount(SettingsAiSummaries, {
    global: { plugins: [createPinia()] },
  });
}

/** The third `.settings-card__switch` is the Auto Translate toggle. */
function getAutoTranslateSwitch(wrapper: ReturnType<typeof mount>) {
  const switches = wrapper.findAll('.settings-card__switch');
  return switches[2]!;
}

describe('settings-ai-summaries.vue', () => {
  beforeEach(() => {
    // happy-dom's localStorage lacks removeItem/clear; install a full shim so
    // the localStorage-backed toggles read/write cleanly.
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    mockInvoke.mockReset();
  });

  it('renders all three toggle labels (Auto Generate, Section Summaries, Auto Translate)', () => {
    mockInvoke.mockResolvedValue(true);
    const wrapper = mountCard();
    const text = wrapper.text();
    expect(text).toContain('Auto Generate Summaries');
    expect(text).toContain('Section Summaries');
    expect(text).toContain('Auto Translate');
  });

  it('renders the Experimental badge next to Auto Translate', () => {
    mockInvoke.mockResolvedValue(true);
    const wrapper = mountCard();
    expect(wrapper.find('.badge--experimental').exists()).toBe(true);
  });

  it('defaults the Auto Translate switch to enabled when the backend reports true', async () => {
    mockInvoke.mockResolvedValue(true);
    const wrapper = mountCard();
    await flushPromises();

    expect(mockInvoke).toHaveBeenCalledWith('get_auto_translate');
    expect(getAutoTranslateSwitch(wrapper).classes()).toContain('settings-card__switch--on');
  });

  it('reflects a disabled Auto Translate state from the backend', async () => {
    mockInvoke.mockResolvedValue(false);
    const wrapper = mountCard();
    await flushPromises();

    expect(getAutoTranslateSwitch(wrapper).classes()).not.toContain('settings-card__switch--on');
  });

  it('toggling Auto Translate invokes set_auto_translate with the new value', async () => {
    mockInvoke.mockResolvedValue(true); // load as enabled
    const wrapper = mountCard();
    await flushPromises();

    await getAutoTranslateSwitch(wrapper).trigger('click');
    await flushPromises();

    expect(mockInvoke).toHaveBeenCalledWith('set_auto_translate', { enabled: false });
  });

  it('reverts the switch and shows an error when set_auto_translate rejects', async () => {
    // Initial load returns enabled; the toggle's set call rejects; the
    // subsequent reload (triggered by the revert path) returns enabled again.
    mockInvoke.mockResolvedValueOnce(true);
    mockInvoke.mockRejectedValueOnce(new Error('DB locked'));
    mockInvoke.mockResolvedValueOnce(true);

    const wrapper = mountCard();
    await flushPromises();

    await getAutoTranslateSwitch(wrapper).trigger('click');
    await flushPromises();

    // The error message is rendered and the switch reverted to on (enabled).
    expect(wrapper.text()).toContain('DB locked');
    expect(getAutoTranslateSwitch(wrapper).classes()).toContain('settings-card__switch--on');
  });
});
