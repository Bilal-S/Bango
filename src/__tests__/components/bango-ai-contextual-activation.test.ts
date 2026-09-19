import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(async () => undefined) }));

import BangoAiContextualDialog from '@/components/bango-ai-contextual-dialog.vue';

describe('bango-ai-contextual-activation', () => {
  it('not_ready_backend_offers_in_place_setup_or_switch', async () => {
    const wrapper = mount(BangoAiContextualDialog, {
      props: {
        model: 'Ornith 1.5 9B',
        downloadBytes: 5_800_000_000,
        license: 'MIT',
        licenseUrl: 'https://example.invalid/model',
        installing: false,
        progress: null,
        error: null,
      },
    });
    const text = wrapper.text();
    expect(text).toContain('Bango AI is not set up yet');
    expect(text).toContain('Set Up Bango AI');
    expect(text).toContain('Use Configured Provider');
    expect(text).toContain('Cancel');

    const setup = wrapper.findAll('button').find((b) => b.text().includes('Set Up Bango AI'));
    await setup!.trigger('click');
    expect(wrapper.emitted('setup')).toHaveLength(1);

    const useConfigured = wrapper
      .findAll('button')
      .find((b) => b.text().includes('Use Configured Provider'));
    await useConfigured!.trigger('click');
    expect(wrapper.emitted('useConfigured')).toHaveLength(1);

    const cancel = wrapper.findAll('button').find((b) => b.text() === 'Cancel');
    await cancel!.trigger('click');
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });
});
