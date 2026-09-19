import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';

// Mock the opener plugin (the license link must not open anything in tests).
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(async () => undefined),
}));

import EmbeddingsConsentDialog from '@/components/settings/embeddings-consent-dialog.vue';
import type { LocalComponentStatus } from '@/composables/use-local-embeddings';

const STATUS: LocalComponentStatus = {
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

/** Mount helper asserting the two-action surface. */
function mountDialog(status: LocalComponentStatus | null = STATUS) {
  return mount(EmbeddingsConsentDialog, { props: { status } });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('embeddings-consent-dialog', () => {
  it('renders_model_terms_and_live_download_size', () => {
    const wrapper = mountDialog();
    const text = wrapper.text();
    expect(text).toContain('EmbeddingGemma 300M Q4');
    expect(text).toContain('Gemma Terms of Use');
    expect(text).toContain('about 250 MB');
    expect(text).toContain('Download and Enable');
  });

  it('falls_back_to_platform_range_without_status', () => {
    const wrapper = mountDialog(null);
    expect(wrapper.text()).toContain('about 220-300 MB');
  });

  it('cancel_button_receives_initial_focus', () => {
    // Attach to the document so happy-dom tracks focus on the mounted node
    // (detached mounts leave `document.activeElement` on <body>).
    const mountHost = document.createElement('div');
    document.body.appendChild(mountHost);
    const wrapper = mount(EmbeddingsConsentDialog, {
      props: { status: STATUS },
      attachTo: mountHost,
    });
    try {
      const cancel = wrapper.findAll('button').find((b) => b.text().includes('Cancel'))!;
      expect(document.activeElement).toBe(cancel.element);
    } finally {
      wrapper.unmount();
      mountHost.remove();
    }
  });

  it('escape_key_cancels_and_buttons_emit_their_actions', async () => {
    const wrapper = mountDialog();
    await wrapper.find('.dialog-overlay').trigger('keydown', { key: 'Escape' });
    expect(wrapper.emitted('cancel')).toHaveLength(1);

    const buttons = wrapper.findAll('button');
    await buttons.find((b) => b.text().includes('Cancel'))!.trigger('click');
    await buttons.find((b) => b.text().includes('Download and Enable'))!.trigger('click');
    expect(wrapper.emitted('cancel')).toHaveLength(2);
    expect(wrapper.emitted('confirm')).toHaveLength(1);
  });
});
