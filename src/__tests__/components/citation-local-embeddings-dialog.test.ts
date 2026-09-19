import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';

import CitationLocalEmbeddingsDialog from '@/components/citation-local-embeddings-dialog.vue';
import type { ComponentProgress, LocalComponentStatus } from '@/composables/use-local-embeddings';

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

function mountDialog(
  props: Partial<{
    installing: boolean;
    error: string | null;
    chatProviderSupportsEmbeddings: boolean;
    progress: ComponentProgress | null;
  }> = {}
) {
  return mount(CitationLocalEmbeddingsDialog, {
    props: {
      status: STATUS,
      progress: null,
      installing: false,
      error: null,
      chatProviderSupportsEmbeddings: true,
      ...props,
    },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('citation-local-embeddings-dialog', () => {
  it('renders_the_three_actions_and_model', () => {
    const wrapper = mountDialog();
    const text = wrapper.text();
    expect(text).toContain('EmbeddingGemma 300M Q4');
    expect(text).toContain('Download and Continue');
    expect(text).toContain('Use Configured Provider');
    expect(text).toContain('Cancel');
  });

  it('disables_all_actions_while_installing_and_shows_progress', async () => {
    const progress: ComponentProgress = {
      phase: 'downloading',
      file: 'model_q4.onnx',
      fileBytes: 125_000_000,
      fileTotal: 196_725_760,
      overallBytes: 125_000_000,
      overallTotal: 250_000_000,
      message: null,
    };
    const wrapper = mountDialog({ installing: true, progress });
    const buttons = wrapper.findAll('button');
    expect(buttons).toHaveLength(3);
    for (const button of buttons) {
      expect(button.attributes('disabled')).toBeDefined();
    }
    expect(wrapper.text()).toContain('50%');
    // Escape must NOT cancel mid-install (the overlay click neither).
    await wrapper.find('.dialog-overlay').trigger('keydown', { key: 'Escape' });
    expect(wrapper.emitted('cancel')).toBeUndefined();
  });

  it('emits_download_useCloud_and_cancel', async () => {
    const wrapper = mountDialog();
    const buttons = wrapper.findAll('button');
    await buttons.find((b) => b.text().includes('Cancel'))!.trigger('click');
    await buttons.find((b) => b.text().includes('Use Configured Provider'))!.trigger('click');
    await buttons.find((b) => b.text().includes('Download and Continue'))!.trigger('click');
    expect(wrapper.emitted('cancel')).toHaveLength(1);
    expect(wrapper.emitted('useCloud')).toHaveLength(1);
    expect(wrapper.emitted('download')).toHaveLength(1);
  });

  it('renders_an_install_error_inline', () => {
    const wrapper = mountDialog({ error: 'Not enough disk space' });
    expect(wrapper.text()).toContain('Not enough disk space');
  });

  it('hides_use_configured_provider_when_the_chat_provider_cannot_embed', () => {
    // findings-7: the option is a dead end for Anthropic/Z.AI users - it
    // hides with an explanatory note, and the license line shows regardless.
    const wrapper = mountDialog({ chatProviderSupportsEmbeddings: false });
    expect(wrapper.text()).not.toContain('Use Configured Provider');
    expect(wrapper.text()).toContain('Your provider has no embedding API');
    expect(wrapper.text()).toContain('Gemma Terms of Use');
  });
});
