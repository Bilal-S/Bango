import { ref, onScopeDispose } from 'vue';
import { tauriCommand } from './use-tauri-command';

/** Backend selection (`configured_provider` | `bango_local`). */
export type EmbeddingBackendId = 'configured_provider' | 'bango_local';

/** Payload of `get_local_embeddings_status` (camelCase, mirrors the Rust struct). */
export interface LocalComponentStatus {
  /** "not_installed" | "repair_required" | "ready" | "installing" | "unsupported". */
  state: string;
  running: boolean;
  runtimeReady: boolean;
  profile: string;
  model: string;
  license: string;
  licenseUrl: string;
  supportedTarget: boolean;
  installedBytes: number;
  requiredBytes: number;
  downloadBytes: number;
  runtimeVersion: string | null;
  threadBudget: number;
  modelRoot: string;
  runtimeRoot: string;
  usedFallback: boolean;
}

/** One `embedding:component` progress event payload. */
export interface ComponentProgress {
  phase: 'downloading' | 'verifying' | 'installing' | 'done' | 'error' | string;
  file: string;
  fileBytes: number;
  fileTotal: number;
  overallBytes: number;
  overallTotal: number;
  message: string | null;
}

/** Verification outcome (`verify_local_embeddings`). */
export interface VerifyOutcome {
  healthy: boolean;
  failures: { name: string; reason: string }[];
}

/**
 * Shared backend selection state. Module-level so every consumer (the
 * Embeddings card and the provider card's override field) sees backend
 * switches immediately, without cross-card event plumbing. First
 * `loadBackend()`/`selectBackend()` wins until the next call.
 */
const sharedBackend = ref<EmbeddingBackendId>('configured_provider');

/**
 * TEST-ONLY reset for the module-level `sharedBackend` ref so test files stay
 * robust to reordering (a test that selects `bango_local` otherwise leaks
 * into the next test). Never call from production code.
 */
export function __resetSharedBackendForTests(): void {
  sharedBackend.value = 'configured_provider';
}

/**
 * Composable for the Settings Embeddings card: backend selection + local
 * component status + install/cancel/verify/remove + `embedding:component`
 * progress events.
 *
 * Event subscription: one `listen` per composable scope, released on scope
 * dispose. Events update `progress` regardless of who started the install
 * (the install may have been started earlier and the card re-mounted).
 */
export function useLocalEmbeddings() {
  const backend = sharedBackend;
  const status = ref<LocalComponentStatus | null>(null);
  const progress = ref<ComponentProgress | null>(null);
  const verifyResult = ref<VerifyOutcome | null>(null);
  const error = ref<string | null>(null);
  const loading = ref(false);
  const installing = ref(false);
  const verifying = ref(false);
  const removing = ref(false);

  let unlisten: (() => void) | null = null;
  let listenPromise: Promise<void> | null = null;
  /** Set on scope dispose: a listener resolving after disposal is released
   * immediately instead of being stored (and leaked) - the dynamic import in
   * `ensureProgressListener` can outlive a quick unmount. */
  let disposed = false;

  function ensureProgressListener(): Promise<void> {
    if (!listenPromise) {
      listenPromise = import('@tauri-apps/api/event')
        .then(({ listen }) =>
          listen<ComponentProgress>('embedding:component', (event) => {
            progress.value = event.payload;
            if (event.payload.phase === 'done' || event.payload.phase === 'error') {
              installing.value = false;
              // The terminal event lands before the command resolves; refresh
              // the derived status (state, sizes, runtime readiness).
              void loadStatus();
            }
          })
        )
        .then((fn) => {
          if (disposed) {
            // The scope died while the import was in flight: release now.
            fn();
            return;
          }
          unlisten = fn;
        })
        .catch(() => {
          // Non-Tauri (tests/browser): events simply never arrive.
          listenPromise = null;
        });
    }
    return listenPromise;
  }
  onScopeDispose(() => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  });

  /** Load the backend selection only. */
  async function loadBackend(): Promise<void> {
    backend.value = await tauriCommand<EmbeddingBackendId>('get_embedding_backend');
  }

  /** Load the local component status only. */
  async function loadStatus(): Promise<void> {
    try {
      status.value = await tauriCommand<LocalComponentStatus>('get_local_embeddings_status');
    } catch {
      // Best-effort: keep the last known status (the card degrades to a hint).
    }
  }

  /** Load both the selection and the component status. */
  async function load(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      await Promise.all([loadBackend(), loadStatus()]);
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
    void ensureProgressListener();
  }

  /**
   * Persist a backend selection, then best-effort re-probe (backend-aware:
   * offline for local, HTTP for cloud) so the capability gates reopen
   * immediately - without this, `recall` stays closed until the next
   * generation or Test Connection because `set_embedding_backend` resets
   * the capability triple to `unknown`.
   *
   * @throws when the backend rejects the selection (invalid id / DB error).
   */
  async function selectBackend(value: EmbeddingBackendId): Promise<void> {
    const saved = await tauriCommand<EmbeddingBackendId>('set_embedding_backend', {
      backend: value,
    });
    backend.value = saved;
    // Fire-and-forget: a probe failure must not fail the selection switch
    // (the next generation re-probes on its own).
    void tauriCommand('probe_embeddings').catch(() => undefined);
  }

  /**
   * Install (or repair) the local components. Progress arrives via
   * `embedding:component` events; resolves when the install command does
   * (model + runtime + engine self-test).
   *
   * @throws on failure - the caller surfaces the error (a terminal `error`
   * event also carries the message).
   */
  async function install(): Promise<void> {
    installing.value = true;
    progress.value = null;
    error.value = null;
    try {
      await tauriCommand('install_local_embeddings');
      await loadStatus();
    } catch (e) {
      error.value = String(e);
      throw e;
    } finally {
      installing.value = false;
    }
  }

  /** Request cancellation of a running install (takes effect in-flight). */
  async function cancelInstall(): Promise<void> {
    await tauriCommand('cancel_local_embeddings_install');
  }

  /** Full SHA-256 + runtime verification. */
  async function verify(): Promise<VerifyOutcome> {
    verifying.value = true;
    verifyResult.value = null;
    try {
      verifyResult.value = await tauriCommand<VerifyOutcome>('verify_local_embeddings');
      return verifyResult.value;
    } finally {
      verifying.value = false;
    }
  }

  /** Remove all local embedding artifacts (model + runtime, every root). */
  async function remove(): Promise<void> {
    removing.value = true;
    try {
      await tauriCommand('remove_local_embeddings');
      await loadStatus();
    } finally {
      removing.value = false;
    }
  }

  return {
    backend,
    status,
    progress,
    verifyResult,
    error,
    loading,
    installing,
    verifying,
    removing,
    load,
    loadBackend,
    loadStatus,
    selectBackend,
    install,
    cancelInstall,
    verify,
    remove,
  };
}
