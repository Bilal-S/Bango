import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createRouter, createMemoryHistory } from 'vue-router';

// Mock the composable so the component's branch logic (consent gating,
// repair banner, install actions) is testable without IPC. The factory
// fills a hoisted holder with controllable refs + spy actions.
const mock = vi.hoisted(() => ({ api: null as Record<string, unknown> | null }));
vi.mock('@/composables/use-local-embeddings', async () => {
  const { ref } = await import('vue');
  const api = {
    backend: ref<'configured_provider' | 'bango_local'>('configured_provider'),
    status: ref<unknown>(null),
    progress: ref<unknown>(null),
    verifyResult: ref<unknown>(null),
    error: ref<string | null>(null),
    loading: ref(false),
    installing: ref(false),
    verifying: ref(false),
    removing: ref(false),
    load: vi.fn(async () => undefined),
    loadBackend: vi.fn(async () => undefined),
    loadStatus: vi.fn(async () => undefined),
    selectBackend: vi.fn(async () => undefined),
    install: vi.fn(async () => undefined),
    cancelInstall: vi.fn(async () => undefined),
    verify: vi.fn(async () => ({ healthy: true, failures: [] })),
    remove: vi.fn(async () => undefined),
  };
  mock.api = api;
  return { useLocalEmbeddings: () => api, __resetSharedBackendForTests: vi.fn() };
});

import SettingsEmbeddings from '@/components/settings/settings-embeddings.vue';
import type { LocalComponentStatus } from '@/composables/use-local-embeddings';

/** Not-installed, supported-target status (consent-gating precondition). */
const NOT_INSTALLED: LocalComponentStatus = {
  state: 'not_installed',
  running: false,
  runtimeReady: false,
  profile: 'builtin/embeddinggemma-300m-q4@r1',
  model: 'EmbeddingGemma 300M Q4',
  license: 'Gemma Terms of Use',
  licenseUrl: 'https://example.invalid/terms',
  supportedTarget: true,
  installedBytes: 0,
  requiredBytes: 500_000_000,
  downloadBytes: 250_000_000,
  runtimeVersion: '1.30.0',
  threadBudget: 4,
  modelRoot: '/docs/Bango/model',
  runtimeRoot: '/appdata/Bango/ai/runtimes',
  usedFallback: false,
};

/** Typed accessor for the mocked composable API (spy actions + controllable refs). */
function api(): {
  backend: { value: 'configured_provider' | 'bango_local' };
  status: { value: unknown };
  error: { value: string | null };
  installing: { value: boolean };
  selectBackend: ReturnType<typeof vi.fn>;
  install: ReturnType<typeof vi.fn>;
} {
  return mock.api as ReturnType<typeof api>;
}
/** Fresh memory router so the Learn-more link navigation is assertable. */
function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div/>' } },
      { path: '/help', component: { template: '<div/>' } },
    ],
  });
}

async function mountCard(router = makeRouter()) {
  const wrapper = mount(SettingsEmbeddings, {
    global: { plugins: [router], stubs: { teleports: true } },
  });
  await flushPromises();
  return { wrapper, router };
}

beforeEach(() => {
  vi.clearAllMocks();
  // Reset shared composable state between tests.
  api().backend.value = 'configured_provider';
  api().status.value = NOT_INSTALLED;
  api().error.value = null;
  api().installing.value = false;
});

describe('settings-embeddings consent gating', () => {
  it('selecting_bango_local_when_not_ready_opens_consent_without_persisting', async () => {
    const { wrapper } = await mountCard();
    expect(wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).exists()).toBe(false);

    const localRadio = wrapper
      .findAll('input[type="radio"]')
      .find((r) => (r.element as HTMLInputElement).value === 'bango_local')!;
    await localRadio.setValue();

    // The dialog opens and the selection is NOT persisted until confirm.
    expect(wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).exists()).toBe(true);
    expect(api().selectBackend).not.toHaveBeenCalled();
  });

  it('consent_cancel_closes_the_dialog_and_keeps_the_cloud_selection', async () => {
    const { wrapper } = await mountCard();
    const localRadio = wrapper
      .findAll('input[type="radio"]')
      .find((r) => (r.element as HTMLInputElement).value === 'bango_local')!;
    await localRadio.setValue();

    const dialog = wrapper.findComponent({ name: 'EmbeddingsConsentDialog' });
    dialog.vm.$emit('cancel');
    await flushPromises();

    expect(wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).exists()).toBe(false);
    expect(api().backend.value).toBe('configured_provider');
    expect(api().selectBackend).not.toHaveBeenCalled();
  });

  it('consent_confirm_selects_the_backend_then_installs', async () => {
    const { wrapper } = await mountCard();
    const localRadio = wrapper
      .findAll('input[type="radio"]')
      .find((r) => (r.element as HTMLInputElement).value === 'bango_local')!;
    await localRadio.setValue();

    wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).vm.$emit('confirm');
    await flushPromises();

    expect(api().selectBackend).toHaveBeenCalledWith('bango_local');
    expect(api().install).toHaveBeenCalledTimes(1);
    expect(wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).exists()).toBe(false);
  });

  it('selecting_bango_local_when_ready_persists_without_the_dialog', async () => {
    api().status.value = { ...NOT_INSTALLED, state: 'ready', runtimeReady: true };
    const { wrapper } = await mountCard();

    const localRadio = wrapper
      .findAll('input[type="radio"]')
      .find((r) => (r.element as HTMLInputElement).value === 'bango_local')!;
    await localRadio.setValue();

    expect(api().selectBackend).toHaveBeenCalledWith('bango_local');
    expect(wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).exists()).toBe(false);
  });

  it('shows_the_repair_banner_when_the_runtime_is_missing', async () => {
    api().status.value = { ...NOT_INSTALLED, state: 'ready', runtimeReady: false };
    const { wrapper } = await mountCard();
    expect(wrapper.text()).toContain('ONNX Runtime engine library is missing or damaged');
  });

  it('manual_download_routes_through_consent_when_cloud_selected', async () => {
    // L4 (findings-7): with the cloud backend selected, the card's Download
    // button must open the consent dialog (Gemma terms precede ANY first
    // download) instead of installing directly.
    const { wrapper } = await mountCard();
    const download = wrapper.findAll('button').find((b) => b.text().includes('Download'))!;
    await download.trigger('click');

    expect(wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).exists()).toBe(true);
    expect(api().install).not.toHaveBeenCalled();
    expect(api().selectBackend).not.toHaveBeenCalled();

    // Confirm then persists + installs (the standard consent flow).
    wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).vm.$emit('confirm');
    await flushPromises();
    expect(api().selectBackend).toHaveBeenCalledWith('bango_local');
    expect(api().install).toHaveBeenCalledTimes(1);
  });

  it('manual_download_installs_directly_when_local_selected', async () => {
    api().backend.value = 'bango_local';
    const { wrapper } = await mountCard();
    const download = wrapper.findAll('button').find((b) => b.text().includes('Download'))!;
    await download.trigger('click');

    expect(wrapper.findComponent({ name: 'EmbeddingsConsentDialog' }).exists()).toBe(false);
    expect(api().install).toHaveBeenCalledTimes(1);
  });

  it('card_describes_what_embeddings_do_and_links_to_the_help_section', async () => {
    const { wrapper, router } = await mountCard();
    const desc = wrapper.find('.settings-card__desc');
    expect(desc.text()).toContain('by meaning instead of exact keywords');
    expect(desc.text()).toContain('Citation Finder');
    // The in-card privacy table is gone; the explanation lives in the Help section.
    expect(wrapper.find('.emb-privacy').exists()).toBe(false);
    const learnMore = wrapper.find('.settings-card__learn-more');
    expect(learnMore.exists()).toBe(true);
    const pushSpy = vi.spyOn(router, 'push');
    await learnMore.trigger('click');
    expect(pushSpy).toHaveBeenCalledWith('/help?tab=reference#ref-embeddings');
  });

  it('radios_disable_while_a_backend_switch_is_in_flight', async () => {
    // Start from a ready LOCAL selection so the cloud radio is unchecked -
    // VTU fires no change event when a radio is already checked.
    api().backend.value = 'bango_local';
    api().status.value = { ...NOT_INSTALLED, state: 'ready', runtimeReady: true };
    const { wrapper } = await mountCard();
    const fieldset = () => wrapper.find('fieldset.emb-options');
    expect((fieldset().element as HTMLFieldSetElement).disabled).toBe(false);

    // Hold the switch open; the radios must disable until it settles.
    let resolveSelect!: () => void;
    api().selectBackend.mockImplementation(
      () => new Promise<void>((resolve) => (resolveSelect = resolve))
    );
    const cloudRadio = wrapper
      .findAll('input[type="radio"]')
      .find((r) => (r.element as HTMLInputElement).value === 'configured_provider')!;
    await cloudRadio.setValue();
    await flushPromises();
    expect((fieldset().element as HTMLFieldSetElement).disabled).toBe(true);

    resolveSelect();
    await flushPromises();
    expect((fieldset().element as HTMLFieldSetElement).disabled).toBe(false);
  });
});
