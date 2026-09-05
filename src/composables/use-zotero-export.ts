import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { tauriCommand } from './use-tauri-command';
import type {
  ZoteroCollection,
  ZoteroConnectionState,
  ZoteroConnectionStatus,
  ZoteroExportPreview,
  ZoteroExportProgress,
  ZoteroExportResult,
  ZoteroSelectedCollection,
} from '@/types/zotero';

/** The local-API preference path repeated by every communication-error state. */
export const ZOTERO_ENABLE_API_HINT =
  'Enable the local API in Zotero under Settings -> Advanced -> "Allow other applications on this computer to communicate with Zotero".';

/**
 * Zotero export state machine: connection gate (with the Zotero 10 version
 * gate), collection dropdown + default selection, DOI-diff preview, the
 * export run, and the `zotero-export:progress` listener (cleaned up on
 * unmount, batch-import pattern).
 */
export function useZoteroExport() {
  const connection = ref<ZoteroConnectionStatus | null>(null);
  const loadingDefaults = ref(false);
  const collections = ref<ZoteroCollection[]>([]);
  const collectionsLoading = ref(false);
  const collectionsError = ref<string | null>(null);
  const selectedKey = ref<string | null>(null);
  const selectedName = ref<string | null>(null);
  const defaults = ref<ZoteroSelectedCollection | null>(null);
  const preview = ref<ZoteroExportPreview | null>(null);
  const previewLoading = ref(false);
  const includeFiles = ref(true);
  const exporting = ref(false);
  const result = ref<ZoteroExportResult | null>(null);
  const error = ref<string | null>(null);
  const progress = ref<ZoteroExportProgress | null>(null);

  const connectionState = computed<ZoteroConnectionState | null>(
    () => connection.value?.status ?? null
  );

  /** Connection message; null when connected/unknown. */
  const connectionMessage = computed<string | null>(() => {
    switch (connection.value?.status) {
      case 'ok':
      case undefined:
        return null;
      case 'not_running':
        return 'Zotero is not running. Start Zotero and try again.';
      case 'api_disabled':
        return connection.value.hint ?? ZOTERO_ENABLE_API_HINT;
      case 'error':
        // Any other communication error repeats the enable-API hint plus the
        // backend message (the user-facing enable-API documentation).
        return `${ZOTERO_ENABLE_API_HINT} (${connection.value.hint ?? 'Zotero connection failed.'})`;
      default:
        return null;
    }
  });

  /** Zotero < 10: the local write API does not exist (import still works). */
  const needsZotero10 = computed(() => {
    const version = connection.value?.zoteroVersion;
    if (!version) return false;
    const major = Number.parseInt(version.split('.')[0] ?? '0', 10);
    return Number.isFinite(major) && major < 10;
  });

  /** The authorize dialog is showing (progress phase `authorize`). */
  const authorizePhase = computed(() => progress.value?.phase === 'authorize');

  /** Open the panel: connection gate + collections + selection defaults. */
  async function openPanel(): Promise<void> {
    result.value = null;
    error.value = null;
    preview.value = null;
    progress.value = null;

    // Transport failures map to an `error` connection state (the command
    // itself maps every backend failure; an IPC rejection must not produce an
    // unhandled promise rejection with a silent, stuck panel).
    let connectionResult: ZoteroConnectionStatus;
    try {
      connectionResult = await tauriCommand<ZoteroConnectionStatus>('check_zotero_connection');
    } catch (e) {
      connectionResult = {
        status: 'error',
        apiVersion: null,
        zoteroVersion: null,
        serverId: null,
        hint: e instanceof Error ? e.message : String(e),
      };
    }
    connection.value = connectionResult;
    if (connectionResult.status !== 'ok') return;

    loadingDefaults.value = true;
    try {
      const [collectionList, defaultsResult] = await Promise.all([
        tauriCommand<ZoteroCollection[]>('get_zotero_collections'),
        tauriCommand<ZoteroSelectedCollection | null>('get_zotero_selected_collection'),
      ]);
      collections.value = collectionList;
      defaults.value = defaultsResult;

      // Default selection: connector exact-name match -> last used -> none.
      // Ambiguous names (multiple collections sharing the connector name)
      // fall through to the next rule.
      let chosen: ZoteroCollection | null = null;
      const connectorName = defaultsResult?.name ?? null;
      if (connectorName) {
        const matches = collectionList.filter((c) => c.name === connectorName);
        if (matches.length === 1) chosen = matches[0] ?? null;
      }
      if (!chosen) {
        const lastKey = defaultsResult?.lastCollectionKey ?? null;
        if (lastKey) chosen = collectionList.find((c) => c.key === lastKey) ?? null;
      }
      selectedKey.value = chosen?.key ?? null;
      selectedName.value = chosen?.name ?? null;
    } catch (e) {
      collectionsError.value =
        e instanceof Error ? e.message : String(e) || 'Failed to load collections';
    } finally {
      loadingDefaults.value = false;
    }
  }

  /** Fetch the DOI-diff preview with the caller's export scope. */
  async function loadPreview(status: string, screeningErrorsOnly: boolean): Promise<void> {
    if (!selectedKey.value) return;
    preview.value = null;
    error.value = null;
    previewLoading.value = true;
    try {
      preview.value = await tauriCommand<ZoteroExportPreview>('export_zotero_preview', {
        collectionKey: selectedKey.value,
        status,
        screeningErrorsOnly,
      });
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e) || 'Preview failed';
    } finally {
      previewLoading.value = false;
    }
  }

  /** Select a collection and fetch its preview for the export scope. */
  async function selectCollection(
    collection: ZoteroCollection,
    status: string,
    screeningErrorsOnly: boolean
  ): Promise<void> {
    selectedKey.value = collection.key;
    selectedName.value = collection.name;
    await loadPreview(status, screeningErrorsOnly);
  }

  /** Run the export for the caller's scope. */
  async function exportCollection(
    status: string,
    screeningErrorsOnly: boolean
  ): Promise<ZoteroExportResult | null> {
    if (!selectedKey.value) return null;
    exporting.value = true;
    error.value = null;
    result.value = null;
    progress.value = null;
    try {
      result.value = await tauriCommand<ZoteroExportResult>('export_zotero_collection', {
        collectionKey: selectedKey.value,
        status,
        screeningErrorsOnly,
        includeFiles: includeFiles.value,
      });
      return result.value;
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      // Denial / rate-limit / expired-key states carry explicit backend copy;
      // connection-ish failures repeat the enable-API hint.
      error.value =
        message.includes('not running') || message.includes('local API is disabled')
          ? `${ZOTERO_ENABLE_API_HINT} (${message})`
          : message;
      return null;
    } finally {
      exporting.value = false;
    }
  }

  // Progress listener with unmount cleanup (batch-import pattern).
  let unlisten: UnlistenFn | null = null;
  onMounted(async () => {
    try {
      unlisten = await listen<ZoteroExportProgress>('zotero-export:progress', (event) => {
        progress.value = event.payload;
      });
    } catch (e) {
      console.warn('[zotero-export] progress listener unavailable:', e);
    }
  });
  onUnmounted(() => {
    if (unlisten) unlisten();
  });

  return {
    connection,
    connectionState,
    connectionMessage,
    needsZotero10,
    loadingDefaults,
    collections,
    collectionsLoading,
    collectionsError,
    selectedKey,
    selectedName,
    defaults,
    preview,
    previewLoading,
    includeFiles,
    exporting,
    result,
    error,
    progress,
    authorizePhase,
    openPanel,
    selectCollection,
    loadPreview,
    exportCollection,
  };
}
