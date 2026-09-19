/**
 * Citation Finder composable - IPC + event plumbing for the paste-prose-to-
 * citations matcher.
 */

import { tauriCommand, isTauri } from '@/composables/use-tauri-command';
import type {
  CitationFinderMode,
  CitationFinderProgress,
  CitationFinderReadiness,
  CitationResult,
  CitationStyle,
  CitationMatch,
  EmbeddingModelMismatch,
} from '@/types/citation-finder';

/** Status filter scope. Duplicates are always excluded. */
export type CitationStatusFilter = string[];

/** Phase B scope baseline from the initial coverage snapshot. */
export interface PhaseBBaseline {
  done: number;
  total: number;
}

/**
 * Rebase a runner `embedding:progress` payload onto the Phase B scope universe
 * so the bar never jumps backward when coverage is mixed.
 *
 * The initial `preparing_embeddings` snapshot counts scope coverage
 * (`done=already embedded, total=scope articles`); runner events count only
 * this run's work (`processed` of `total` needed). Summing them keeps one
 * monotonic "X of Y ready" bar; without a baseline the run counts stand alone.
 *
 * @param event Runner payload (`processed` finished this run, `total` needed).
 * @param baseline Initial snapshot counts, or `null` when none arrived.
 * @returns Cumulative counts + the 0-90 percent used by the bar.
 */
export function rebasePhaseBProgress(
  event: { processed: number; total: number },
  baseline: PhaseBBaseline | null
): { done: number; total: number; overallPercent: number } {
  const total = baseline && baseline.total > 0 ? baseline.total : Math.max(0, event.total);
  const rawDone = (baseline?.done ?? 0) + event.processed;
  const done = total > 0 ? Math.min(total, rawDone) : Math.max(0, rawDone);
  const ratio = total > 0 ? done / total : 0;
  return { done, total, overallPercent: Math.min(90, Math.round(ratio * 90)) };
}

/** At most one `citation:*` listener set is active at a time. */
let activeUnlisten: (() => void) | null = null;

/** Tear down active `citation:*` listeners. */
export function stopCitationListeners(): void {
  if (activeUnlisten) {
    activeUnlisten();
    activeUnlisten = null;
  }
}

/** Initiate a citation search. The assistant bubble arrives via `citation:done`. */
export async function findCitations(args: {
  text: string;
  mode: CitationFinderMode;
  statusFilter: CitationStatusFilter;
  onProgress?: (p: CitationFinderProgress) => void;
  onDone?: (results: CitationResult[]) => void;
  onError?: (msg: string) => void;
}): Promise<CitationFinderProgress> {
  const { text, mode, statusFilter, onProgress, onDone, onError } = args;
  /* Tear down prior subscription, then set up 3 listeners BEFORE the command
  so the first progress event (from inside the spawned task) is never missed. */
  if (activeUnlisten) {
    activeUnlisten();
    activeUnlisten = null;
  }
  if (isTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    const handles: Array<() => void> = [];
    /* Phase B scope baseline: the first `preparing_embeddings` snapshot
      carries coverage counts; runner events are run-relative. */
    let phaseBBaseline: PhaseBBaseline | null = null;
    handles.push(
      await listen<CitationFinderProgress>('citation:progress', (e) => {
        if (e.payload.phase === 'preparing_embeddings' && phaseBBaseline === null) {
          phaseBBaseline = { done: e.payload.done, total: e.payload.total };
        }
        onProgress?.(e.payload);
      })
    );
    /* Phase B forwarding: the embedding runner emits `embedding:progress`
      during the prepare phase. Translate each into a `citation:progress`
      update in the 0-90% range, rebased onto the scope baseline so mixed
      coverage cannot regress the bar. Torn down on `citation:done`/`error`. */
    handles.push(
      await listen<{ processed: number; total: number; phase: string; model: string }>(
        'embedding:progress',
        (e) => {
          const rebased = rebasePhaseBProgress(e.payload, phaseBBaseline);
          onProgress?.({
            phase: 'preparing_embeddings',
            stage: undefined,
            done: rebased.done,
            total: rebased.total,
            overallPercent: rebased.overallPercent,
            message:
              rebased.total > 0
                ? `Preparing embeddings… ${rebased.done}/${rebased.total} articles`
                : 'Preparing embeddings…',
            isRunning: true,
            isCancelled: false,
          });
        }
      )
    );
    handles.push(
      await listen<CitationResult[]>('citation:done', (e) => {
        onDone?.(e.payload);
        for (const h of handles) h();
        activeUnlisten = null;
      })
    );
    handles.push(
      await listen<string>('citation:error', (e) => {
        onError?.(e.payload);
        for (const h of handles) h();
        activeUnlisten = null;
      })
    );
    activeUnlisten = () => {
      for (const h of handles) h();
    };
  }

  return tauriCommand<CitationFinderProgress>('find_citations', {
    text,
    mode,
    statusFilter,
  });
}

/** Cancel a running citation search. */
export async function cancelSearch(): Promise<void> {
  await tauriCommand<void>('cancel_citation_search');
}

/** Read readiness payload (toggle visibility + tooltip hint). */
export async function getReadiness(
  statusFilter: CitationStatusFilter
): Promise<CitationFinderReadiness> {
  return tauriCommand<CitationFinderReadiness>('get_citation_finder_readiness', {
    statusFilter,
  });
}

/**
 * Detect whether stored embeddings were generated with a different model.
 * Returns `null` when no mismatch. Cheap (one SELECT DISTINCT + COUNT).
 */
export async function getModelMismatch(): Promise<EmbeddingModelMismatch | null> {
  return tauriCommand<EmbeddingModelMismatch | null>('get_embedding_model_mismatch');
}

/**
 * Regenerate ALL embeddings in the given status scope. Deletes existing rows
 * then re-runs. Used by the model-mismatch confirmation dialog.
 */
export async function regenerateEmbeddings(statusFilter: string | null): Promise<void> {
  await tauriCommand<unknown>('regenerate_embeddings', { statusFilter });
}

/**
 * Regenerate all embeddings in the scope while streaming live progress.
 * Subscribes to `embedding:progress` for the duration of the command (which
 * resolves after the run completes) and always releases the listener.
 *
 * @param statusFilter Comma-joined status scope; `null` = all statuses.
 * @param onProgress Live progress sink (Phase B-shaped payloads).
 */
export async function regenerateEmbeddingsWithProgress(
  statusFilter: string | null,
  onProgress: (p: CitationFinderProgress) => void
): Promise<void> {
  let unlisten: (() => void) | null = null;
  if (isTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    unlisten = await listen<{ processed: number; total: number }>('embedding:progress', (e) => {
      const rebased = rebasePhaseBProgress(e.payload, null);
      onProgress({
        phase: 'preparing_embeddings',
        stage: undefined,
        done: rebased.done,
        total: rebased.total,
        overallPercent: rebased.overallPercent,
        message:
          rebased.total > 0
            ? `Regenerating embeddings… ${rebased.done}/${rebased.total} articles`
            : 'Regenerating embeddings…',
        isRunning: true,
        isCancelled: false,
      });
    });
  }
  try {
    await regenerateEmbeddings(statusFilter);
  } finally {
    unlisten?.();
  }
}

// ── Citation formatting (pure, no deps) ───────────────────────────────────

/** Build the leading in-text citation author name. Pure. */
export function firstAuthor(match: Pick<CitationMatch, 'authors'>): string {
  const first = match.authors[0];
  if (!first) return 'Unknown';
  /* Authors are stored "Last, F." (comma-separated) or "Last First"
  (whitespace-separated). If comma, take surname before it; otherwise
  take first whitespace token. Falls back to "Unknown". */
  if (first.includes(',')) {
    const surname = first.split(',')[0]?.trim();
    if (surname) return surname;
  }
  const bySpace = first.split(/\s+/)[0]?.trim();
  if (bySpace) return bySpace;
  return first.trim() || 'Unknown';
}

/**
 * Format a citation match as plain text in the given style.
 * @param ieeeIndex  1-based index for IEEE `[N]`. Per-bubble numbering.
 */
export function formatCitation(
  match: CitationMatch,
  style: CitationStyle,
  ieeeIndex?: number
): string {
  const year = match.publicationYear ? `, ${match.publicationYear}` : '';
  const yearMla = match.publicationYear ? ` ${match.publicationYear}` : '';
  const prefixByStyle: Record<CitationStyle, string> = {
    APA: `(${firstAuthor(match)}${year})`,
    MLA: `(${firstAuthor(match)}${yearMla})`,
    Chicago: `(${firstAuthor(match)}${yearMla})`,
    IEEE: `[${ieeeIndex ?? 1}]`,
    AMA: `(${firstAuthor(match)}${yearMla})`,
  };
  const prefix = prefixByStyle[style];
  const bodyParts: string[] = [];
  if (match.authors.length > 0) bodyParts.push(match.authors.join(', '));
  if (match.publicationYear) bodyParts.push(String(match.publicationYear));
  if (match.title) bodyParts.push(match.title);
  if (match.journal) bodyParts.push(match.journal);
  if (match.doi) bodyParts.push(`doi:${match.doi}`);
  const body = bodyParts.join('. ');
  return `${prefix} ${body}.`;
}
