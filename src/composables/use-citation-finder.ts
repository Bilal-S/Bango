/**
 * Citation Finder composable — IPC + event plumbing for the paste-prose-to-
 * citations matcher (spec §8.7).
 *
 * The Chat view owns the message list; this composable owns the 3 Tauri
 * commands (`find_citations`, `cancel_citation_search`,
 * `get_citation_finder_readiness`) + the 3 events (`citation:progress`,
 * `citation:done`, `citation:error`). The chat store's `sendMessage` branch
 * delegates here instead of calling `send_chat_message`.
 *
 * The Citation Style `<select>` lives only in the citation-finder input area;
 * the active style is captured at submit time and frozen into the assistant
 * bubble via the store, so each bubble renders all its cards with the style
 * that was selected when the search ran.
 */

import { tauriCommand, isTauri } from '@/composables/use-tauri-command';
import type {
  CitationFinderMode,
  CitationFinderProgress,
  CitationFinderReadiness,
  CitationResult,
  CitationStyle,
  CitationMatch,
} from '@/types/citation-finder';

/** The status filter the search is scoped to. Duplicates are always excluded
 *  (the backend never surfaces them; the UI hides the Duplicate checkbox). */
export type CitationStatusFilter = string[];

/** Internal holder so the live `UnlistenFn` set can be torn down when a new
 *  search starts (or when the Chat view unmounts via `stopCitationListeners`).
 *  At most one listener set is active at a time. */
let activeUnlisten: (() => void) | null = null;

/** Tear down any active `citation:*` listeners. Called by `findCitations`
 *  before re-subscribing, and should be called by the Chat view's
 *  `onUnmounted` to avoid dangling listeners when navigating away. */
export function stopCitationListeners(): void {
  if (activeUnlisten) {
    activeUnlisten();
    activeUnlisten = null;
  }
}

/**
 * Initiate a citation search. The command returns immediately with the
 * initial progress snapshot; the assistant bubble arrives via `citation:done`.
 *
 * @param args.text      The pasted prose to find citations for.
 * @param args.mode      `'whole_block'` (one embedding) or `'per_statement'`
 *                       (LLM splits into ≤5 claims, each embedded separately).
 *                       Snake_case wire token — matches the Rust enum's
 *                       `#[serde(rename_all = "snake_case")]`.
 * @param args.statusFilter  Article statuses to include in the candidate pool.
 * @param args.onProgress   Optional callback for `citation:progress` payloads
 *                          (the Chat view uses it to drive the progress bar).
 * @param args.onDone       Callback for `citation:done` — receives the result
 *                          groups + the captured style (the store pushes the
 *                          assistant bubble here).
 * @param args.onError      Callback for `citation:error`.
 */
export async function findCitations(args: {
  text: string;
  mode: CitationFinderMode;
  statusFilter: CitationStatusFilter;
  onProgress?: (p: CitationFinderProgress) => void;
  onDone?: (results: CitationResult[]) => void;
  onError?: (msg: string) => void;
}): Promise<CitationFinderProgress> {
  const { text, mode, statusFilter, onProgress, onDone, onError } = args;
  // Tear down any prior subscription, then set up the 3 listeners BEFORE the
  // command so the first progress event (emitted from inside the spawned
  // task) is never missed.
  if (activeUnlisten) {
    activeUnlisten();
    activeUnlisten = null;
  }
  if (isTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    const handles: Array<() => void> = [];
    handles.push(
      await listen<CitationFinderProgress>('citation:progress', (e) => {
        onProgress?.(e.payload);
      })
    );
    // Phase B forwarding: the embedding runner emits `embedding:progress`
    // during the prepare phase (payload `{processed, total, phase, model}`).
    // The backend runs the runner with `emit_events=true`, so translate each
    // into a `citation:progress` update in the 0-90% range (the same range the
    // backend's initial Phase B snapshot uses). `done`/`total` mirror the
    // runner's `processed`/`total`. Torn down on `citation:done`/`error`.
    handles.push(
      await listen<{ processed: number; total: number; phase: string; model: string }>(
        'embedding:progress',
        (e) => {
          const { processed, total } = e.payload;
          // Map (processed, total) to 0-90% (Phase B range). total == 0 guard
          // avoids division by zero for a stale payload.
          const ratio = total > 0 ? Math.min(processed, total) / total : 0;
          const overallPercent = Math.min(90, Math.round(ratio * 90));
          onProgress?.({
            phase: 'preparing_embeddings',
            stage: undefined,
            done: processed,
            total,
            overallPercent,
            message: `Preparing embeddings… ${processed}/${total} articles`,
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

/**
 * Cancel a running citation search. The backend sets the `Arc<AtomicBool>`
 * cancel token, which aborts Phase B (via `JoinSet::abort_all` inside
 * `generate_embeddings_inner`) or Phase C (token check between stages) and
 * emits `citation:error "Cancelled"`.
 */
export async function cancelSearch(): Promise<void> {
  await tauriCommand<void>('cancel_citation_search');
}

/**
 * Read the readiness payload (toggle visibility + tooltip hint). The toggle
 * is hidden when `providerSupportsEmbeddings === false` (e.g. Anthropic).
 * Does NOT gate the action — `find_citations` runs its own Phase A check.
 */
export async function getReadiness(
  statusFilter: CitationStatusFilter
): Promise<CitationFinderReadiness> {
  return tauriCommand<CitationFinderReadiness>('get_citation_finder_readiness', {
    statusFilter,
  });
}

// ── Citation formatting (pure, no deps) ───────────────────────────────────
// `citation_finder/AGENTS.md` Option A. Zero npm deps. Reuses the 5-style list; IEEE `[N]` is
// assigned by the caller (per-bubble card order), passed in via `ieeeIndex`.

/** Build the leading initial(s) for the in-text citation: "Smith" from
 *  "Smith, J." or "Unknown" when the author list is empty. Pure. */
export function firstAuthor(match: Pick<CitationMatch, 'authors'>): string {
  const first = match.authors[0];
  if (!first) return 'Unknown';
  // Authors are stored "Last, F." (comma-separated) or "Last First"
  // (whitespace-separated). If there's a comma, take the surname before it;
  // otherwise take the first whitespace token. Falls back to the whole string
  // + "Unknown" for empty input.
  if (first.includes(',')) {
    const surname = first.split(',')[0]?.trim();
    if (surname) return surname;
  }
  const bySpace = first.split(/\s+/)[0]?.trim();
  if (bySpace) return bySpace;
  return first.trim() || 'Unknown';
}

/** Format a citation match as a plain-text string in the given style.
 *
 *  @param match       The citation to format.
 *  @param style       One of the 5 shared styles.
 *  @param ieeeIndex   1-based index for IEEE `[N]` numbering. Ignored for
 *                     other styles. The caller passes the card's position
 *                     within the assistant bubble so `[1]`, `[2]`, … are
 *                     unique across the whole bubble (per-bubble numbering).
 *  @returns Plain-text citation, e.g.
 *           `(Smith, 2024). Smith, J. Title. Journal. doi:10.1000/x`. */
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
