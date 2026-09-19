import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(async () => undefined) }));

import BangoAiConsentDialog from '@/components/settings/bango-ai-consent-dialog.vue';

function mountDialog() {
  return mount(BangoAiConsentDialog, {
    props: {
      model: 'Ornith 1.5 9B',
      downloadBytes: 5_800_000_000,
      license: 'MIT',
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
    expect(text).toContain('Ornith 1.5 9B');
    expect(text).toContain('about 5.4 GB');
    expect(text).toContain('MIT');
    expect(text).toContain('on this device');

    const confirm = wrapper.findAll('button').find((b) => b.text().includes('Download and Enable'));
    await confirm!.trigger('click');
    expect(wrapper.emitted('confirm')).toHaveLength(1);

    const cancel = wrapper.findAll('button').find((b) => b.text() === 'Cancel');
    await cancel!.trigger('click');
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });
});
