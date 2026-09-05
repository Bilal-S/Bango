/**
 * Zotero local API integration types (import wizard + export panel).
 * Mirrors the Rust payload structs in `src-tauri/src/zotero/` and
 * `src-tauri/src/commands/zotero.rs` (serde camelCase).
 */
import type { ImportPreview, ImportResult } from '@/composables/use-import';

export type ZoteroConnectionState = 'ok' | 'not_running' | 'api_disabled' | 'error';

export interface ZoteroConnectionStatus {
  status: ZoteroConnectionState;
  apiVersion: string | null;
  /** From `X-Zotero-Version` on every response; gates the write API (10+). */
  zoteroVersion: string | null;
  /** From `Zotero-Server-ID`; binds the stored write key to this instance. */
  serverId: string | null;
  hint: string | null;
}

/** Flat collection entry (`parentKey` is null for root collections). */
export interface ZoteroCollection {
  key: string;
  name: string;
  parentKey: string | null;
}

/** Collection preview: the standard review-step `ImportPreview` plus Zotero data. */
export interface ZoteroCollectionPreview {
  preview: ImportPreview;
  /** Zotero item keys aligned with `preview.previewArticles` (valid records only). */
  articleKeys: string[];
  libraryVersion: number | null;
  totalItems: number;
  mappedArticles: number;
  attachmentCount: number;
  tagCount: number;
}

/** `zotero-import:progress` event payload. */
export interface ZoteroImportProgress {
  phase: 'metadata' | 'attachments';
  done: number;
  total: number;
  failed: number;
}

/** Import result: the standard `ImportResult` plus attachment tallies. */
export interface ZoteroImportResult {
  result: ImportResult;
  attachedCount: number;
  attachmentFailedCount: number;
  attachmentSkippedCount: number;
}

/** Connector-reported selection + the last-collection fallback default. */
export interface ZoteroSelectedCollection {
  name: string | null;
  libraryName: string | null;
  editable: boolean;
  lastCollectionKey: string | null;
  lastCollectionName: string | null;
}

/** `zotero-export:progress` event payload. */
export interface ZoteroExportProgress {
  phase: 'authorize' | 'items' | 'files';
  done: number;
  total: number;
  failed: number;
}

/** Export preview counts (the DOI diff; nothing is written). */
export interface ZoteroExportPreview {
  totalArticles: number;
  missingCount: number;
  alreadyPresentCount: number;
  noDoiCount: number;
  fileCount: number;
}

/** Export result counts + the target collection echo. */
export interface ZoteroExportResult {
  exportedCount: number;
  failedCount: number;
  /** Items Zotero reported as unchanged (already up to date). */
  unchangedCount: number;
  alreadyPresentCount: number;
  noDoiCount: number;
  fileAttachedCount: number;
  fileFailedCount: number;
  fileSkippedCount: number;
  collectionName: string;
  libraryVersion: number | null;
}
