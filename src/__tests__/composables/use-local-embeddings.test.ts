import { describe, it, expect, beforeEach, vi } from 'vitest';

// ── Hoisted mocks ────────────────────────────────────────────────────
// `vi.mock` is hoisted above all imports, so state it closes over must be
// hoisted too (`vi.hoisted`) to avoid TDZ errors when mocked code runs
// during module import.
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

// Import after mocks are set up (vi.mock is hoisted above imports).
import {
  useLocalEmbeddings,
  __resetSharedBackendForTests,
  type LocalComponentStatus,
} from '@/composables/use-local-embeddings';
import { effectScope } from 'vue';

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

function commandResponse(command: string): unknown {
  switch (command) {
    case 'get_embedding_backend':
      return 'configured_provider';
    case 'get_local_embeddings_status':
      return NOT_INSTALLED;
    case 'set_embedding_backend':
      return 'bango_local';
    case 'install_local_embeddings':
      return { downloaded: 7, skipped: 0, bytesDownloaded: 0 };
    case 'verify_local_embeddings':
      return { healthy: true, failures: [] };
    default:
      return null;
  }
}

describe('use-local-embeddings', () => {
  let dispose: (() => void) | null = null;

  /** Build a fresh composable inside an effect scope (so `onScopeDispose` runs). */
  function make() {
    const scope = effectScope();
    const api = scope.run(() => useLocalEmbeddings())!;
    dispose = () => scope.stop();
    return api;
  }

  beforeEach(async () => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((command: string) => Promise.resolve(commandResponse(command)));
    eventCallbacks.clear();
    unlistenSpy.calls = 0;
    __resetSharedBackendForTests();
  });

  it('load_populates_selection_status_and_subscribes_to_events', async () => {
    const api = make();
    await api.load();

    expect(api.backend.value).toBe('configured_provider');
    expect(api.status.value?.state).toBe('not_installed');
    expect(mockInvoke).toHaveBeenCalledWith('get_embedding_backend');
    expect(mockInvoke).toHaveBeenCalledWith('get_local_embeddings_status');
    // The listener registers lazily after load (dynamic import resolves).
    await vi.waitFor(() => expect(eventCallbacks.has('embedding:component')).toBe(true));
    dispose?.();
    // Scope dispose releases the listener.
    expect(unlistenSpy.calls).toBe(1);
  });

  it('progress_events_update_and_terminal_done_refreshes_status', async () => {
    const api = make();
    await api.load();
    await vi.waitFor(() => expect(eventCallbacks.has('embedding:component')).toBe(true));

    const emit = eventCallbacks.get('embedding:component')!;
    api.installing.value = true;
    emit({
      payload: {
        phase: 'downloading',
        file: 'model_q4.onnx',
        fileBytes: 10,
        fileTotal: 100,
        overallBytes: 10,
        overallTotal: 250_000_000,
        message: null,
      },
    });
    expect(api.progress.value?.file).toBe('model_q4.onnx');

    const ready = { ...NOT_INSTALLED, state: 'ready', runtimeReady: true };
    mockInvoke.mockImplementation((command: string) =>
      Promise.resolve(command === 'get_local_embeddings_status' ? ready : commandResponse(command))
    );
    emit({
      payload: {
        phase: 'done',
        file: '',
        fileBytes: 0,
        fileTotal: 0,
        overallBytes: 1,
        overallTotal: 1,
        message: null,
      },
    });
    expect(api.installing.value).toBe(false);
    await vi.waitFor(() => expect(api.status.value?.state).toBe('ready'));
    dispose?.();
  });

  it('select_backend_persists_and_updates_shared_ref', async () => {
    const a = make();
    await a.selectBackend('bango_local');
    expect(mockInvoke).toHaveBeenCalledWith('set_embedding_backend', { backend: 'bango_local' });
    expect(a.backend.value).toBe('bango_local');
    // Probe-after-switch: the capability gates reopen immediately because
    // `set_embedding_backend` resets the triple (backend-aware probe; a
    // failure is swallowed and must not fail the switch).
    await vi.waitFor(() => expect(mockInvoke).toHaveBeenCalledWith('probe_embeddings'));

    // The backend ref is module-level shared state: a second composable
    // (e.g. the provider card) sees the switch immediately.
    const b = make();
    expect(b.backend.value).toBe('bango_local');
    dispose?.();
  });

  it('listener_resolving_after_scope_dispose_is_released_immediately', async () => {
    // Regression: a quick unmount while the dynamic `listen` import is in
    // flight must not store (and leak) the resolved unlisten callback - it
    // must be CALLED immediately (observable via the release counter).
    const holder: { release: (() => void) | null } = { release: null };
    const deferred = new Promise<() => void>((resolve) => {
      holder.release = () =>
        resolve(() => {
          unlistenSpy.calls += 1;
        });
    });
    const { listen } = await import('@tauri-apps/api/event');
    (listen as ReturnType<typeof vi.fn>).mockImplementationOnce(async () => deferred as never);

    const api = make();
    const loadPromise = api.load();
    // Dispose BEFORE the dynamic import resolves the listener registration.
    dispose?.();
    expect(unlistenSpy.calls).toBe(0);
    // Now the registration resolves: the composable must call the unlisten
    // fn immediately (scope is gone) instead of storing it forever.
    holder.release?.();
    await loadPromise;
    await vi.waitFor(() => expect(unlistenSpy.calls).toBe(1));
  });

  it('install_reloads_status_failures_surface_in_error', async () => {
    const api = make();
    const ready = { ...NOT_INSTALLED, state: 'ready', runtimeReady: true };
    let call = 0;
    mockInvoke.mockImplementation((command: string) => {
      call += 1;
      if (command === 'install_local_embeddings') return Promise.resolve(null);
      if (command === 'get_local_embeddings_status')
        return Promise.resolve(call > 1 ? ready : NOT_INSTALLED);
      return Promise.resolve(commandResponse(command));
    });

    await api.install();
    expect(api.installing.value).toBe(false);
    expect(api.status.value?.state).toBe('ready');

    // Failure path: error ref set + rethrow for the caller's toast/dialog.
    mockInvoke.mockImplementation((command: string) =>
      command === 'install_local_embeddings'
        ? Promise.reject(new Error('Not enough disk space'))
        : Promise.resolve(commandResponse(command))
    );
    await expect(api.install()).rejects.toThrow('Not enough disk space');
    expect(api.error.value).toContain('Not enough disk space');
    expect(api.installing.value).toBe(false);
    dispose?.();
  });

  it('cancel_install_reverts_backend_and_suppresses_the_error', async () => {
    const api = make();
    await api.selectBackend('bango_local');
    expect(api.backend.value).toBe('bango_local');

    let rejectInstall!: (error: Error) => void;
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'install_local_embeddings') {
        return new Promise((_resolve, reject) => {
          rejectInstall = reject;
        });
      }
      if (command === 'set_embedding_backend') return Promise.resolve('configured_provider');
      return Promise.resolve(commandResponse(command));
    });

    const installPromise = api.install();
    expect(api.installing.value).toBe(true);

    await api.cancelInstall();

    expect(mockInvoke).toHaveBeenCalledWith('cancel_local_embeddings_install');
    expect(api.installing.value).toBe(false);
    expect(api.progress.value).toBeNull();
    expect(api.backend.value).toBe('configured_provider');
    expect(mockInvoke).toHaveBeenCalledWith('set_embedding_backend', {
      backend: 'configured_provider',
    });

    rejectInstall(new Error('Cancelled'));
    await expect(installPromise).resolves.toBeUndefined();
    expect(api.error.value).toBeNull();
    dispose?.();
  });

  it('install_after_cancel_reports_real_failures_again', async () => {
    const api = make();
    await api.selectBackend('bango_local');

    let rejectInstall!: (error: Error) => void;
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'install_local_embeddings') {
        return new Promise((_resolve, reject) => {
          rejectInstall = reject;
        });
      }
      return Promise.resolve(commandResponse(command));
    });

    const first = api.install();
    await api.cancelInstall();
    rejectInstall(new Error('Cancelled'));
    await first;

    mockInvoke.mockImplementation((command: string) =>
      command === 'install_local_embeddings'
        ? Promise.reject(new Error('Not enough disk space'))
        : Promise.resolve(commandResponse(command))
    );
    await expect(api.install()).rejects.toThrow('Not enough disk space');
    expect(api.error.value).toContain('Not enough disk space');
    dispose?.();
  });

  it('verify_stores_outcome_remove_reloads_status', async () => {
    const api = make();
    const outcome = await api.verify();
    expect(outcome.healthy).toBe(true);
    expect(api.verifyResult.value?.healthy).toBe(true);

    mockInvoke.mockImplementation((command: string) =>
      Promise.resolve(
        command === 'get_local_embeddings_status' ? NOT_INSTALLED : commandResponse(command)
      )
    );
    await api.remove();
    expect(mockInvoke).toHaveBeenCalledWith('remove_local_embeddings');
    expect(api.status.value?.state).toBe('not_installed');
    dispose?.();
  });
});
