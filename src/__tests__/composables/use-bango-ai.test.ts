import { describe, it, expect, beforeEach, vi } from 'vitest';
import { effectScope } from 'vue';
import { flushPromises } from '@vue/test-utils';

const { eventCallbacks, unlistenSpy } = vi.hoisted(() => ({
  eventCallbacks: new Map<string, (event: { payload: unknown }) => void | Promise<void>>(),
  unlistenSpy: { calls: 0 },
}));

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: (...args: unknown[]) => mockInvoke(...args),
  isTauri: () => true,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(
    async (
      event: string,
      callback: (event: { payload: unknown }) => void | Promise<void>
    ): Promise<() => void> => {
      eventCallbacks.set(event, callback);
      return () => {
        unlistenSpy.calls += 1;
      };
    }
  ),
}));

const mockInvoke = vi.fn();

const storeMock = vi.hoisted(() => ({ refreshBackendState: vi.fn(async () => undefined) }));

vi.mock('@/stores/llm-config', () => ({
  useLlmConfigStore: () => storeMock,
}));

import {
  useBangoAi,
  __resetSharedBackendForTests,
  type BangoAiStatus,
} from '@/composables/use-bango-ai';

const STATUS: BangoAiStatus = {
  state: 'ready',
  running: false,
  supportedTarget: true,
  engineState: 'ready',
  busy: false,
  profile: 'builtin/ornith-1.5-9b-q4km@r1',
  model: 'Ornith 1.5 9B',
  license: 'MIT',
  licenseUrl: 'https://example.invalid/model',
  engine: 'llama.cpp',
  runtimeVersion: 'b10964',
  runtimeReady: true,
  modelReady: true,
  installedBytes: 6_000_000_000,
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
};

function commandResponse(command: string): unknown {
  switch (command) {
    case 'get_llm_backend':
      return 'bango_ai';
    case 'get_bango_ai_status':
      return STATUS;
    case 'set_llm_backend':
      return undefined;
    case 'install_bango_ai':
      return { state: 'ready', runtimeBytes: 1, modelBytes: 2, message: 'ready' };
    case 'test_bango_ai':
      return {
        modelLoadMs: 1200,
        responseMs: 400,
        tokensPerSecond: 12.5,
        tokens: 5,
        effectiveContext: 16_384,
        detail: 'llama.cpp is ready.',
      };
    default:
      return undefined;
  }
}

/** Tracks the persisted selection so status reads reflect a switch. */
let currentBackend = 'configured_provider';

function runInScope<T>(fn: (dispose: () => void) => T): T {
  const scope = effectScope();
  let result!: T;
  scope.run(() => {
    result = fn(() => scope.stop());
  });
  return result;
}

beforeEach(() => {
  vi.clearAllMocks();
  eventCallbacks.clear();
  unlistenSpy.calls = 0;
  __resetSharedBackendForTests();
  currentBackend = 'configured_provider';
  mockInvoke.mockImplementation(
    async (command: string, args?: { backend?: string }): Promise<unknown> => {
      if (command === 'set_llm_backend') {
        currentBackend = args?.backend ?? currentBackend;
        return undefined;
      }
      if (command === 'get_llm_backend') return currentBackend;
      if (command === 'get_bango_ai_status') return { ...STATUS, backend: currentBackend };
      return commandResponse(command);
    }
  );
});

describe('use-bango-ai', () => {
  it('loads_status_and_subscribes_to_component_events', async () => {
    currentBackend = 'bango_ai';
    await runInScope(async (dispose) => {
      const api = useBangoAi();
      await api.load();
      expect(api.status.value?.model).toBe('Ornith 1.5 9B');
      expect(api.backend.value).toBe('bango_ai');
      expect(eventCallbacks.has('bango_ai:component')).toBe(true);
      dispose();
      expect(unlistenSpy.calls).toBe(1);
    });
  });

  it('install_success_refreshes_status_and_clears_installing', async () => {
    await runInScope(async () => {
      const api = useBangoAi();
      const outcome = await api.install(true);
      expect(outcome?.state).toBe('ready');
      expect(api.installing.value).toBe(false);
      expect(api.error.value).toBeNull();
      // get_bango_ai_status is re-read after a successful install.
      const statusCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === 'get_bango_ai_status');
      expect(statusCalls.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('install_failure_sets_error_and_rethrows', async () => {
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === 'install_bango_ai') throw new Error('disk full');
      return commandResponse(command);
    });
    await runInScope(async () => {
      const api = useBangoAi();
      await expect(api.install(true)).rejects.toThrow('disk full');
      expect(api.installing.value).toBe(false);
      expect(api.error.value).toContain('disk full');
    });
  });

  it('shared_backend_ref_keeps_panels_in_sync', async () => {
    await runInScope(async () => {
      const first = useBangoAi();
      const second = useBangoAi();
      await first.selectBackend('configured_provider');
      expect(second.backend.value).toBe('configured_provider');
      await second.selectBackend('bango_ai');
      expect(first.backend.value).toBe('bango_ai');
      // Install state is shared: a consent-triggered install stays visible in
      // every scope even while the backend remains configured_provider.
      first.installing.value = true;
      first.progress.value = {
        phase: 'downloading',
        file: 'Ornith-1.5-9B-Q4_K_M.gguf',
        fileBytes: 1,
        fileTotal: 2,
        overallBytes: 1,
        overallTotal: 2,
        message: null,
      };
      expect(second.installing.value).toBe(true);
      expect(second.progress.value?.file).toBe('Ornith-1.5-9B-Q4_K_M.gguf');
      first.error.value = 'boom';
      expect(second.error.value).toBe('boom');
    });
  });

  it('terminal_install_event_refreshes_llm_config_gate', async () => {
    await runInScope(async (dispose) => {
      const api = useBangoAi();
      await api.load();
      storeMock.refreshBackendState.mockClear();

      const emit = eventCallbacks.get('bango_ai:component')!;
      expect(emit).toBeTruthy();
      await emit({
        payload: {
          phase: 'done',
          file: '',
          fileBytes: 0,
          fileTotal: 0,
          overallBytes: 1,
          overallTotal: 1,
          message: 'Bango AI is ready.',
        },
      });
      // The listener dispatches `void handleProgress(...)`: flush the async
      // refresh chain before asserting.
      await flushPromises();
      await flushPromises();
      expect(storeMock.refreshBackendState).toHaveBeenCalledTimes(1);

      storeMock.refreshBackendState.mockClear();
      await api.selectBackend('bango_ai');
      expect(storeMock.refreshBackendState).toHaveBeenCalledTimes(1);
      dispose();
    });
  });

  it('test_connection_reports_throughput_as_info', async () => {
    await runInScope(async () => {
      const api = useBangoAi();
      const outcome = await api.test();
      expect(outcome?.tokensPerSecond).toBe(12.5);
      expect(api.testOutcome.value?.effectiveContext).toBe(16_384);
      expect(api.testing.value).toBe(false);
    });
  });
});
