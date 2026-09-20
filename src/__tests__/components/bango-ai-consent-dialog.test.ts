import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(async () => undefined) }));

import BangoAiConsentDialog from '@/components/settings/bango-ai-consent-dialog.vue';

function mountDialog() {
  return mount(BangoAiConsentDialog, {
    props: {
      model: 'Qwen3.5 2B',
      downloadBytes: 1_339_752_704,
      license: 'Apache-2.0',
      licenseUrl: 'https://example.invalid/model',
    },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('bango-ai-consent-dialog', () => {
  it('shows_model_size_and_license_and_emits_confirm', async () => {
    const wrapper = mountDialog();
    const text = wrapper.text();
    expect(text).toContain('Qwen3.5 2B');
    expect(text).toContain('about 1.2 GB');
    expect(text).toContain('Apache-2.0');
    expect(text).toContain('on this device');

    const confirm = wrapper.findAll('button').find((b) => b.text().includes('Download and Enable'));
    await confirm!.trigger('click');
    expect(wrapper.emitted('confirm')).toHaveLength(1);

    const cancel = wrapper.findAll('button').find((b) => b.text() === 'Cancel');
    await cancel!.trigger('click');
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });
});
