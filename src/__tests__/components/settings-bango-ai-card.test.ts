import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import type { Ref } from 'vue';

const mocked = vi.hoisted(() => ({
  state: {} as Record<string, unknown>,
  selectBackend: vi.fn(),
  install: vi.fn(async () => undefined),
  cancelInstall: vi.fn(),
  test: vi.fn(),
  verify: vi.fn(),
  remove: vi.fn(),
  load: vi.fn(async () => undefined),
  loadSettings: vi.fn(async () => ({ context: 16384, threads: 4, reasoning: false })),
  saveSettings: vi.fn(),
}));

vi.mock('@/composables/use-bango-ai', async () => {
  const { ref } = await import('vue');
  mocked.state = {
    backend: ref('configured_provider'),
    status: ref(null),
    progress: ref(null),
    testOutcome: ref(null),
    verifyOutcome: ref(null),
    error: ref(null),
    loading: ref(false),
    installing: ref(false),
    testing: ref(false),
    verifying: ref(false),
    removing: ref(false),
    switching: ref(false),
    loadBackend: vi.fn(),
    loadStatus: vi.fn(),
    load: mocked.load,
    selectBackend: mocked.selectBackend,
    install: mocked.install,
    cancelInstall: mocked.cancelInstall,
    test: mocked.test,
    verify: mocked.verify,
    remove: mocked.remove,
    loadSettings: mocked.loadSettings,
    saveSettings: mocked.saveSettings,
  };
  return { useBangoAi: () => mocked.state };
});

vi.mock('@/components/settings/settings-provider-card.vue', () => ({
  default: { template: '<div class="provider-stub" />' },
}));

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(async () => undefined) }));

import SettingsBangoAiCard from '@/components/settings/settings-bango-ai-card.vue';
import SettingsAiSection from '@/components/settings/settings-ai-section.vue';
import type { BangoAiStatus } from '@/composables/use-bango-ai';

function refOf<T>(key: string): Ref<T> {
  return mocked.state[key] as Ref<T>;
}

function status(overrides: Partial<BangoAiStatus> = {}): BangoAiStatus {
  return {
    state: 'not_installed',
    running: false,
    supportedTarget: true,
    engineState: 'stopped',
    busy: false,
    profile: 'builtin/qwen3.5-2b-ud-q4kxl@r1',
    model: 'Qwen3.5 2B',
    license: 'MIT',
    licenseUrl: 'https://example.invalid/model',
    engine: 'llama.cpp',
    runtimeVersion: 'b10964',
    runtimeReady: false,
    modelReady: false,
    installedBytes: 0,
    requiredBytes: 6_500_000_000,
    downloadBytes: 5_800_000_000,
    context: 16_384,
    threads: 4,
    reasoning: false,
    hardware: {
      target: 'linux-x64',
      cpuCores: 8,
      totalRamMb: 32_768,
      availableRamMb: 16_384,
      availableDiskMb: 100_000,
      avx2: true,
    },
    verdict: { status: 'supported' },
    modelRoot: '/docs/Bango/model',
    runtimeRoot: '/appdata/Bango/ai/runtimes',
    usedFallback: false,
    logPath: '/appdata/Bango/ai/logs/llama-server.log',
    backend: 'bango_ai',
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  refOf<BangoAiStatus | null>('status').value = status();
  refOf<unknown>('progress').value = null;
  refOf<boolean>('installing').value = false;
  refOf<unknown>('error').value = null;
  refOf<string>('backend').value = 'configured_provider';
});

describe('settings-bango-ai-card', () => {
  it('renders_hardware_warning_below_ram_floor', () => {
    refOf<BangoAiStatus | null>('status').value = status({
      verdict: {
        status: 'warning',
        reasons: ['This computer has 6 GB of memory. Bango AI needs about 3 GB of memory.'],
      },
    });
    const wrapper = mount(SettingsBangoAiCard);
    expect(wrapper.text()).toContain('6 GB of memory');
    expect(wrapper.text()).toContain('Set Up Bango AI');
    expect(wrapper.text()).toContain('generally slower than cloud providers');
  });

  it('selecting_bango_ai_opens_consent_when_not_ready', async () => {
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    const radios = wrapper.findAll('input[type="radio"]');
    await radios[1]!.trigger('change');
    await flushPromises();
    expect(wrapper.text()).toContain('Set up Bango AI?');
    expect(mocked.selectBackend).not.toHaveBeenCalled();
  });

  it('renders_component_details_and_onedrive_note', async () => {
    refOf<BangoAiStatus | null>('status').value = status({
      state: 'ready',
      runtimeReady: true,
      modelReady: true,
      usedFallback: true,
    });
    const wrapper = mount(SettingsBangoAiCard);
    await flushPromises();
    const toggle = wrapper.findAll('button').find((b) => b.text().includes('Component Details'));
    // Same expandable pattern as the Embeddings card: caret icon + label,
    // shared `.settings-card__details-*` chrome.
    expect(toggle!.find('.material-symbols-outlined').text()).toBe('expand_more');
    await toggle!.trigger('click');
    expect(toggle!.find('.material-symbols-outlined').text()).toBe('expand_less');
    expect(wrapper.find('.settings-card__details-grid').exists()).toBe(true);
    expect(wrapper.find('.settings-card__details-fallback').exists()).toBe(true);
    expect(wrapper.text()).toContain('/docs/Bango/model');
    expect(wrapper.text()).toContain('OneDrive detected');
  });

  it('shows_monotonic_progress_and_cancel', async () => {
    refOf<boolean>('installing').value = true;
    refOf<unknown>('progress').value = {
      phase: 'downloading',
      file: 'Qwen3.5-2B-UD-Q4_K_XL.gguf',
      fileBytes: 2_000_000_000,
      fileTotal: 5_800_000_000,
      overallBytes: 50,
      overallTotal: 100,
      message: null,
    };
    const wrapper = mount(SettingsBangoAiCard);
    expect(wrapper.text()).toContain('50%');
    // ONE overall meter (aifixes1 Wave 3): the per-component slim bar is
    // gone; the model label + byte counts live on as a text row.
    expect(wrapper.text()).toContain('AI model');
    expect(wrapper.text()).toContain('1.86 GB of 5.40 GB');
    expect(wrapper.findAll('.bango-card__bar')).toHaveLength(1);
    const cancel = wrapper.findAll('button').find((b) => b.text() === 'Cancel');
    await cancel!.trigger('click');
    expect(mocked.cancelInstall).toHaveBeenCalledTimes(1);
  });

  it('restored_selection_without_components_offers_setup_or_switch', async () => {
    refOf<string>('backend').value = 'bango_ai';
    const wrapper = mount(SettingsAiSection);
    await flushPromises();
    expect(wrapper.text()).toContain('Set Up Bango AI');
    expect(wrapper.text()).toContain('Use Configured Provider');
    expect(wrapper.text()).toContain('Your content stays on this device');
  });

  it('unsupported_target_renders_blocked_state_without_cloud_fallback', () => {
    refOf<BangoAiStatus | null>('status').value = status({
      state: 'unsupported',
      supportedTarget: false,
      verdict: {
        status: 'unsupported',
        reasons: ['Bango AI is not available for this system.'],
      },
    });
    const wrapper = mount(SettingsBangoAiCard);
    expect(wrapper.text()).toContain('Bango AI unavailable on this machine');
    expect(wrapper.text()).toContain('Use Configured Provider');
    expect(wrapper.text().toLowerCase()).not.toContain('fall back');
  });
});
