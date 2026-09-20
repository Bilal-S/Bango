import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import type { Ref } from 'vue';

const mocked = vi.hoisted(() => ({
  state: {} as Record<string, unknown>,
  selectBackend: vi.fn(async () => undefined),
}));

vi.mock('@/composables/use-bango-ai', async () => {
  const { ref } = await import('vue');
  mocked.state = {
    backend: ref('configured_provider'),
    status: ref({ state: 'ready', supportedTarget: true, model: 'Qwen3.5 2B' }),
    switching: ref(false),
    installing: ref(false),
    error: ref<string | null>(null),
    load: vi.fn(async () => undefined),
    selectBackend: mocked.selectBackend,
    install: vi.fn(async () => undefined),
  };
  return { useBangoAi: () => mocked.state };
});

vi.mock('@/components/settings/settings-provider-card.vue', () => ({
  default: { template: '<div class="provider-stub" />' },
}));

vi.mock('@/components/settings/settings-bango-ai-card.vue', () => ({
  default: { template: '<div class="bango-stub" />' },
}));

import SettingsAiSection from '@/components/settings/settings-ai-section.vue';

function backendRef(): Ref<string> {
  return mocked.state.backend as Ref<string>;
}

beforeEach(() => {
  vi.clearAllMocks();
  backendRef().value = 'configured_provider';
  (mocked.state.installing as Ref<boolean>).value = false;
  (mocked.state.error as Ref<string | null>).value = null;
  (mocked.state.status as Ref<unknown>).value = {
    state: 'ready',
    supportedTarget: true,
    model: 'Qwen3.5 2B',
  };
});

describe('settings-backend-selection', () => {
  it('selection_header_switches_provider_and_bango_ai', async () => {
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    const radios = wrapper.findAll('input[type="radio"]');

    await radios[1]!.trigger('change');
    expect(mocked.selectBackend).toHaveBeenCalledWith('bango_ai');

    await radios[0]!.trigger('change');
    expect(mocked.selectBackend).toHaveBeenCalledWith('configured_provider');
  });

  it('provider_selection_persists_configured_provider', async () => {
    backendRef().value = 'bango_ai';
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    await wrapper.findAll('input[type="radio"]')[0]!.trigger('change');
    expect(mocked.selectBackend).toHaveBeenCalledWith('configured_provider');
  });

  it('consent_install_shows_progress_card_before_activation', async () => {
    backendRef().value = 'configured_provider';
    // A consent-triggered install mounts the progress card even though the
    // backend stays configured_provider until the self-test succeeds.
    (mocked.state.installing as Ref<boolean>).value = true;
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    expect(wrapper.find('.bango-stub').exists()).toBe(true);

    // A failure surfaces through the section error line once the card unmounts.
    (mocked.state.error as Ref<string | null>).value = 'download failed';
    (mocked.state.installing as Ref<boolean>).value = false;
    await flushPromises();
    expect(wrapper.find('.bango-stub').exists()).toBe(false);
    expect(wrapper.text()).toContain('download failed');
  });

  it('selecting_bango_ai_hides_the_provider_configuration', async () => {
    backendRef().value = 'bango_ai';
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    expect(wrapper.find('.bango-stub').exists()).toBe(true);
    expect(wrapper.find('.provider-stub').exists()).toBe(false);
  });

  it('consent_install_hides_the_provider_configuration_and_keeps_bango_selected', async () => {
    backendRef().value = 'configured_provider';
    (mocked.state.installing as Ref<boolean>).value = true;
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    expect(wrapper.find('.provider-stub').exists()).toBe(false);
    const bangoRadio = wrapper.findAll('input[type="radio"]')[1]!.element as HTMLInputElement;
    expect(bangoRadio.checked).toBe(true);
  });

  it('consent_cancel_reverts_the_radio_to_configured_provider', async () => {
    (mocked.state.status as Ref<unknown>).value = { state: 'not_installed', supportedTarget: true };
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    const [cloud, bango] = wrapper.findAll('input[type="radio"]');

    await bango!.setValue();
    expect((bango!.element as HTMLInputElement).checked).toBe(true);
    expect(wrapper.findComponent({ name: 'BangoAiConsentDialog' }).exists()).toBe(true);

    wrapper.findComponent({ name: 'BangoAiConsentDialog' }).vm.$emit('cancel');
    await flushPromises();

    expect(wrapper.findComponent({ name: 'BangoAiConsentDialog' }).exists()).toBe(false);
    expect((cloud!.element as HTMLInputElement).checked).toBe(true);
    expect((bango!.element as HTMLInputElement).checked).toBe(false);
  });

  it('install_cancel_reverts_the_radio_to_configured_provider', async () => {
    (mocked.state.status as Ref<unknown>).value = { state: 'not_installed', supportedTarget: true };
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    const [cloud, bango] = wrapper.findAll('input[type="radio"]');

    // Reproduce the native click: Bango AI is checked, cloud is not.
    await bango!.setValue();
    wrapper.findComponent({ name: 'BangoAiConsentDialog' }).vm.$emit('confirm');
    (mocked.state.installing as Ref<boolean>).value = true;
    await flushPromises();
    expect((bango!.element as HTMLInputElement).checked).toBe(true);
    expect((cloud!.element as HTMLInputElement).checked).toBe(false);

    // Cancelling the install clears `installing` before the backend moves.
    (mocked.state.installing as Ref<boolean>).value = false;
    await flushPromises();

    expect((cloud!.element as HTMLInputElement).checked).toBe(true);
    expect((bango!.element as HTMLInputElement).checked).toBe(false);
  });
});
