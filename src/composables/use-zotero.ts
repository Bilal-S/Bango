import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { tauriCommand } from './use-tauri-command';
import type {
  ZoteroCollection,
  ZoteroCollectionPreview,
  ZoteroConnectionState,
  ZoteroConnectionStatus,
  ZoteroImportProgress,
} from '@/types/zotero';

/**
 * Zotero import wizard state: connection check, collection listing, preview
 * fetch, and the `zotero-import:progress` listener (cleaned up on unmount,
 * batch-import pattern). The endpoint is fixed to localhost; the flow is
 * read-only until the review step's Confirm.
 */
export function useZotero() {
  const connection = ref<ZoteroConnectionStatus | null>(null);
  const checkingConnection = ref(false);
  const collections = ref<ZoteroCollection[]>([]);
  const collectionsLoading = ref(false);
  const collectionsError = ref<string | null>(null);
  const previewLoading = ref(false);
  const previewError = ref<string | null>(null);
  const zoteroProgress = ref<ZoteroImportProgress | null>(null);

  /** User-facing connection message; null when connected/unknown. */
  const connectionMessage = computed<string | null>(() => {
    switch (connection.value?.status) {
      case 'ok':
      case undefined:
        return null;
      case 'not_running':
        return 'Zotero is not running. Start Zotero and try again.';
      case 'api_disabled':
        return (
          connection.value.hint ??
          'Enable the Zotero local API in Settings -> Advanced -> "Allow other applications on this computer to communicate with Zotero".'
        );
      case 'error':
        return connection.value.hint ?? 'Zotero connection failed.';
      default:
        return null;
    }
  });

  const connectionState = computed<ZoteroConnectionState | null>(
    () => connection.value?.status ?? null
  );

  /** Probe the local Zotero API; true when connected. Never throws. */
  async function checkConnection(): Promise<boolean> {
    checkingConnection.value = true;
    connection.value = null;
    let result: ZoteroConnectionStatus;
    try {
      result = await tauriCommand<ZoteroConnectionStatus>('check_zotero_connection');
    } catch (e) {
      // The command itself maps every failure; a transport error still needs
      // a user-visible state.
      result = {
        status: 'error',
        apiVersion: null,
        zoteroVersion: null,
        serverId: null,
        hint: e instanceof Error ? e.message : String(e),
      };
    }
    connection.value = result;
    checkingConnection.value = false;
    return result.status === 'ok';
  }

  /** Load the flat collection list. */
  async function loadCollections(): Promise<void> {
    collectionsLoading.value = true;
    collectionsError.value = null;
    try {
      collections.value = await tauriCommand<ZoteroCollection[]>('get_zotero_collections');
    } catch (e) {
      console.error('[zotero] loadCollections failed:', e);
      collectionsError.value =
        e instanceof Error ? e.message : String(e) || 'Failed to load collections';
    } finally {
      collectionsLoading.value = false;
    }
  }

  /** Fetch the review-step preview for a collection. Throws on failure. */
  async function fetchPreview(collectionKey: string): Promise<ZoteroCollectionPreview> {
    previewLoading.value = true;
    previewError.value = null;
    try {
      return await tauriCommand<ZoteroCollectionPreview>('get_zotero_collection_preview', {
        collectionKey,
      });
    } catch (e) {
      console.error('[zotero] fetchPreview failed:', e);
      previewError.value = e instanceof Error ? e.message : String(e) || 'Preview failed';
      throw e;
    } finally {
      previewLoading.value = false;
    }
  }

  // Progress listener with unmount cleanup (batch-import pattern).
  let unlisten: UnlistenFn | null = null;
  onMounted(async () => {
    try {
      unlisten = await listen<ZoteroImportProgress>('zotero-import:progress', (event) => {
        zoteroProgress.value = event.payload;
      });
    } catch (e) {
      console.warn('[zotero] progress listener unavailable:', e);
    }
  });
  onUnmounted(() => {
    if (unlisten) unlisten();
  });

  return {
    connection,
    connectionState,
    connectionMessage,
    checkingConnection,
    checkConnection,
    collections,
    collectionsLoading,
    collectionsError,
    loadCollections,
    previewLoading,
    previewError,
    fetchPreview,
    zoteroProgress,
  };
}
