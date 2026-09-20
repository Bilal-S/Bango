import { onScopeDispose, ref } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
import { useLlmConfigStore, type LlmBackendId } from '@/stores/llm-config';

export interface BangoAiHardwareProfile {
  target: string | null;
  cpuCores: number;
  totalRamMb: number;
  availableRamMb: number;
  availableDiskMb: number;
  avx2: boolean | null;
}

export interface BangoAiVerdict {
  status: 'supported' | 'warning' | 'unsupported';
  reasons?: string[];
}

export interface BangoAiStatus {
  state: 'installing' | 'unsupported' | 'ready' | 'repair_required' | 'not_installed';
  running: boolean;
  supportedTarget: boolean;
  engineState: string;
  busy: boolean;
  profile: string;
  model: string;
  license: string;
  licenseUrl: string;
  engine: string;
  runtimeVersion: string;
  runtimeReady: boolean;
  modelReady: boolean;
  installedBytes: number;
  requiredBytes: number;
  downloadBytes: number;
  context: number;
  threads: number;
  reasoning: boolean;
  hardware: BangoAiHardwareProfile;
  verdict: BangoAiVerdict;
  modelRoot: string;
  runtimeRoot: string;
  usedFallback: boolean;
  logPath: string;
  backend: LlmBackendId;
}

interface BangoAiSettings {
  context: number;
  threads: number;
  reasoning: boolean;
}

interface BangoAiProgress {
  phase: string;
  file: string;
  fileBytes: number;
  fileTotal: number;
  overallBytes: number;
  overallTotal: number;
  message?: string | null;
}

interface BangoAiTestOutcome {
  modelLoadMs: number;
  responseMs: number;
  tokensPerSecond: number;
  tokens: number;
  effectiveContext: number;
  detail: string;
}

interface BangoAiVerifyOutcome {
  healthy: boolean;
  failures: { name: string; reason: string }[];
}

interface BangoAiInstallOutcome {
  state: string;
  runtimeBytes: number;
  modelBytes: number;
  message: string;
}

/** Shared backend ref: the selection header and the card must stay in sync. */
const sharedBackend = ref<LlmBackendId>('configured_provider');

/** Shared install state: a consent-triggered install runs while the backend
 * is still `configured_provider`, so every scope (selection header + card)
 * must observe the same installing flag, progress, and error. */
const sharedInstalling = ref(false);
const sharedProgress = ref<BangoAiProgress | null>(null);
const sharedError = ref<string | null>(null);

/** Set while the user is cancelling: the in-flight command's rejection is a
 *  deliberate cancel, not a failure, so `install()` and the terminal `error`
 *  event stay silent. Reset when the next install starts. */
const sharedCancelRequested = ref(false);

/** Test-only reset for the module-level shared refs. */
export function __resetSharedBackendForTests(): void {
  sharedBackend.value = 'configured_provider';
  sharedInstalling.value = false;
  sharedProgress.value = null;
  sharedError.value = null;
  sharedCancelRequested.value = false;
}

/**
 * Bango AI state + actions. Mirrors `use-local-embeddings`: one event
 * listener per scope, terminal events refresh the status, and failures
 * surface to the caller.
 */
export function useBangoAi() {
  const backend = sharedBackend;
  /* Canonical-gate bridge: install completion and backend switches must
   * refresh `isConfigured` so local-ready unlocks every LLM gate without an
   * app restart (aifixes1 F16). */
  const llmConfigStore = useLlmConfigStore();
  const status = ref<BangoAiStatus | null>(null);
  const progress = sharedProgress;
  const testOutcome = ref<BangoAiTestOutcome | null>(null);
  const verifyOutcome = ref<BangoAiVerifyOutcome | null>(null);
  const error = sharedError;
  const loading = ref(false);
  const installing = sharedInstalling;
  const testing = ref(false);
  const verifying = ref(false);
  const removing = ref(false);
  const switching = ref(false);

  let unlisten: (() => void) | null = null;
  let disposed = false;

  async function handleProgress(event: { payload: BangoAiProgress }): Promise<void> {
    progress.value = event.payload;
    if (event.payload.phase === 'done' || event.payload.phase === 'error') {
      /* A cancelled install is not a failure: cancelInstall already cleared
      the UI and reverted the selection, so the terminal error is dropped. */
      if (event.payload.phase === 'error' && sharedCancelRequested.value) {
        installing.value = false;
        return;
      }
      if (event.payload.phase === 'error') {
        error.value = event.payload.message ?? 'Bango AI setup failed.';
      } else {
        error.value = null;
      }
      /* Reload before clearing `installing` so the selection watcher never
      sees a stale backend mid-switch. */
      await loadStatus();
      installing.value = false;
      // Activation/removal changed LLM usability: refresh the canonical gate.
      await llmConfigStore.refreshBackendState();
    }
  }

  async function ensureListener(): Promise<void> {
    if (unlisten || disposed || !isTauri()) return;
    const stop = await listen<BangoAiProgress>('bango_ai:component', (event) => {
      void handleProgress(event);
    });
    // Scope may have been disposed while the listener promise resolved.
    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
  }

  onScopeDispose(() => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  });

  async function loadBackend(): Promise<void> {
    if (!isTauri()) return;
    try {
      const selected = await tauriCommand<string>('get_llm_backend');
      if (selected === 'bango_ai' || selected === 'configured_provider') {
        backend.value = selected;
      }
    } catch (e) {
      error.value = String(e);
    }
  }

  async function loadStatus(): Promise<void> {
    if (!isTauri()) return;
    try {
      status.value = await tauriCommand<BangoAiStatus>('get_bango_ai_status');
      if (status.value) {
        backend.value = status.value.backend;
        error.value = status.value.state === 'unsupported' ? null : error.value;
      }
    } catch (e) {
      error.value = String(e);
    }
  }

  async function load(): Promise<void> {
    loading.value = true;
    try {
      await ensureListener();
      await Promise.all([loadBackend(), loadStatus()]);
    } finally {
      loading.value = false;
    }
  }

  async function selectBackend(id: LlmBackendId): Promise<void> {
    switching.value = true;
    try {
      await tauriCommand('set_llm_backend', { backend: id });
      backend.value = id;
      await loadStatus();
      // The switch changed LLM usability: refresh the canonical gate.
      await llmConfigStore.refreshBackendState();
    } finally {
      switching.value = false;
    }
  }

  async function install(activate = true): Promise<BangoAiInstallOutcome | null> {
    installing.value = true;
    error.value = null;
    progress.value = null;
    sharedCancelRequested.value = false;
    try {
      const outcome = await tauriCommand<BangoAiInstallOutcome>('install_bango_ai', { activate });
      /* Reload before clearing `installing` so the selection watcher never
      sees a stale backend mid-switch. */
      await loadStatus();
      return outcome;
    } catch (e) {
      /* A deliberate cancel resolves silently; the section has already
      reverted to Configured Provider and must not show a red error. */
      if (sharedCancelRequested.value) return null;
      error.value = String(e);
      throw e;
    } finally {
      installing.value = false;
    }
  }

  /**
   * Cancel a running install: clear the progress UI immediately, stop the
   * backend download (staging stays for resume), and return the selection to
   * Configured Provider when the local backend was already persisted (repair
   * path). The cancelled command rejection is suppressed by `install()`.
   */
  async function cancelInstall(): Promise<void> {
    sharedCancelRequested.value = true;
    installing.value = false;
    progress.value = null;
    await tauriCommand('cancel_bango_ai_install');
    if (backend.value !== 'configured_provider') {
      try {
        await selectBackend('configured_provider');
      } catch (e) {
        error.value = String(e);
      }
    }
  }

  async function test(): Promise<BangoAiTestOutcome | null> {
    testing.value = true;
    error.value = null;
    try {
      testOutcome.value = await tauriCommand<BangoAiTestOutcome>('test_bango_ai');
      return testOutcome.value;
    } catch (e) {
      error.value = String(e);
      return null;
    } finally {
      testing.value = false;
    }
  }

  async function verify(): Promise<void> {
    verifying.value = true;
    try {
      verifyOutcome.value = await tauriCommand<BangoAiVerifyOutcome>('verify_bango_ai');
    } catch (e) {
      error.value = String(e);
    } finally {
      verifying.value = false;
    }
  }

  async function remove(): Promise<void> {
    removing.value = true;
    try {
      await tauriCommand('remove_bango_ai');
      verifyOutcome.value = null;
      testOutcome.value = null;
      await loadStatus();
    } catch (e) {
      error.value = String(e);
    } finally {
      removing.value = false;
    }
  }

  async function loadSettings(): Promise<BangoAiSettings | null> {
    try {
      return await tauriCommand<BangoAiSettings>('get_bango_ai_settings');
    } catch (e) {
      error.value = String(e);
      return null;
    }
  }

  async function saveSettings(settings: BangoAiSettings): Promise<void> {
    try {
      await tauriCommand('set_bango_ai_settings', settings as unknown as Record<string, unknown>);
      await loadStatus();
    } catch (e) {
      error.value = String(e);
    }
  }

  return {
    backend,
    status,
    progress,
    testOutcome,
    verifyOutcome,
    error,
    loading,
    installing,
    testing,
    verifying,
    removing,
    switching,
    loadBackend,
    loadStatus,
    load,
    selectBackend,
    install,
    cancelInstall,
    test,
    verify,
    remove,
    loadSettings,
    saveSettings,
  };
}
