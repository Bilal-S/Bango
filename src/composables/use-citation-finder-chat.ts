/**
 * Citation Finder orchestration for the Chat view: readiness + toggle state
 * (backend-aware, incl. the Bango Local not-installed exception), the
 * submit pipeline (contextual local-embeddings prompt -> model-mismatch
 * dialog -> search dispatch), the mismatch dialog handlers, the local
 * prompt handlers, and the citation copy action. Owns no markup.
 */

import { ref, computed, watch, type Ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useToast } from '@/composables/use-toast';
import {
  getReadiness,
  getModelMismatch,
  regenerateEmbeddingsWithProgress,
} from '@/composables/use-citation-finder';
import type {
  CitationFinderMode,
  CitationFinderProgress,
  CitationStatusFlags,
  EmbeddingModelMismatch,
} from '@/types/citation-finder';
import type { useLocalEmbeddings } from '@/composables/use-local-embeddings';
import type { useChatStore } from '@/stores/chat';

export function useCitationFinderChat(args: {
  /** The chat store. */
  chatStore: ReturnType<typeof useChatStore>;
  /** The canonical LLM-configured gate (see src/AGENTS.md). */
  isLlmConfigured: Ref<boolean>;
  /** The local-embeddings composable instance (backend switch + install). */
  localEmbeddings: ReturnType<typeof useLocalEmbeddings>;
  /** Re-reads readiness after a backend switch or install. */
  checkReadiness: () => Promise<void>;
  /** Dispatches the held prose through the store's search path. */
  runSearch: (text: string) => Promise<void>;
}) {
  const { chatStore } = args;
  const toast = useToast();

  /** Articles-to-Search selection (store-backed + localStorage-persisted). */
  const citationStatuses = computed(() => chatStore.citationStatuses);

  /** Computed status filter array passed to the backend. */
  const citationStatusFilter = computed(() => {
    const selected = chatStore.citationStatuses;
    const out: string[] = [];
    if (selected.working) out.push('working');
    if (selected.included) out.push('included');
    if (selected.rejected) out.push('rejected');
    return out;
  });

  /** Persist a selection change, then re-check readiness under the new scope. */
  function setCitationStatuses(next: CitationStatusFlags): void {
    chatStore.setCitationStatuses(next);
    void args.checkReadiness();
  }

  /** Whether the citation input area is shown (source === 'citation-finder'). */
  const isCitationMode = computed(() => chatStore.source === 'citation-finder');

  /**
   * The Citation Finder toggle's visible/disabled/hidden state, derived from
   * the readiness payload's `embeddingStatus` triple-state.
   *
   * - `'enabled'`: embeddings are working; toggle is clickable.
   * - `'unknown'`: probe has not run yet; toggle is clickable (Phase B will
   *   probe on first run).
   * - `'disabled'`: known-unsupported provider; toggle renders disabled with
   *   a tooltip pointing the user to Settings.
   * - `'hidden'`: readiness has not loaded yet OR the LLM is not configured.
   *
   * Backend-aware (T7): with `bango_local` selected and the components not
   * installed, the toggle stays clickable - the submit opens the contextual
   * download prompt instead of a hard-disabled toggle.
   */
  const citationToggleState = computed<'enabled' | 'unknown' | 'disabled' | 'hidden'>(() => {
    const r = chatStore.citationReadiness;
    if (!r || !args.isLlmConfigured.value) return 'hidden';
    if (r.embeddingBackend === 'bango_local' && !r.localReady) return 'unknown';
    return r.embeddingStatus;
  });

  /** Tooltip for the Citation Finder toggle, varying by state. */
  const citationToggleTitle = computed(() => {
    if (isCitationMode.value) {
      return 'Citation Finder active. Click to return to article context.';
    }
    const r = chatStore.citationReadiness;
    if (r?.embeddingBackend === 'bango_local') {
      if (!r.localReady) {
        return 'Bango Local embeddings are not downloaded yet. You will be asked to download them (or use your configured provider) when you search.';
      }
      return r.embeddingStatus === 'disabled'
        ? "Bango Local's last self-test failed. Open Settings - Embeddings and use Verify Installation (or re-install), then search again."
        : 'Find citations for text you are writing (semantic search over your library, running on-device).';
    }
    switch (citationToggleState.value) {
      case 'disabled':
        return 'Current provider does not support embeddings. Switch to an embedding-capable provider (e.g. OpenAI, Ollama) - or select Bango Local in Settings - Embeddings.';
      case 'unknown':
        return 'Find citations for text you are writing (semantic search over your library). First run will prepare embeddings.';
      default:
        return 'Find citations for text you are writing (semantic search over your library)';
    }
  });

  /**
   * Readiness check. Populates `chatStore.citationReadiness` (drives the
   * toggle state). Self-healing stale-disabled (T7 follow-up, findings-6
   * 3.5): healthy local components + a persisted `disabled` triple means a
   * transient self-test failure stuck around; fire the backend-aware -
   * offline - probe once and re-read.
   */
  async function checkCitationFinderReadiness(): Promise<void> {
    try {
      let r = await getReadiness(citationStatusFilter.value);
      if (
        r.embeddingBackend === 'bango_local' &&
        r.localReady &&
        r.embeddingStatus === 'disabled'
      ) {
        try {
          await tauriCommand('probe_embeddings');
          r = await getReadiness(citationStatusFilter.value);
        } catch {
          // Keep the stale payload; the Phase A gate carries the message.
        }
      }
      chatStore.setCitationReadiness(r);
    } catch {
      // Provider not configured / IPC error: hide the toggle.
      chatStore.setCitationReadiness(null);
    }
  }

  /** Mode toggle handler (segmented button). */
  function onSetCitationMode(mode: CitationFinderMode): void {
    chatStore.setCitationFinderMode(mode);
  }

  /** Flip the citation-finder source on. Mutually exclusive with wiki. */
  function onToggleCitationFinder(): void {
    if (chatStore.source === 'citation-finder') {
      chatStore.setSource('articles');
    } else {
      chatStore.setSource('citation-finder');
    }
  }

  /* Model-mismatch confirmation dialog state. */
  const mismatchDialog = ref<EmbeddingModelMismatch | null>(null);
  /** Prose held while a dialog is open; re-dispatched on continue paths. */
  const pendingSearchText = ref('');
  /** True while the Regenerate action is dispatching. */
  const regenerating = ref(false);
  /** Live `embedding:progress` payload while regenerating (drives the dialog
   *  progress bar); cleared on completion/failure. */
  const regeneratingProgress = ref<CitationFinderProgress | null>(null);

  /* T7 contextual Bango Local prompt: when the local backend is selected
   * but not installed, a submit opens the download prompt instead of the
   * Phase A hard error. */
  const localPromptOpen = ref(false);

  /**
   * Submit the citation search. Before dispatching: (1) the contextual
   * local-embeddings prompt when `bango_local` is selected but not
   * installed, (2) the model-mismatch dialog when stored embeddings were
   * generated with a different model (recall would silently return zero
   * hits). The mismatch dialog fires once per stored-model key per session.
   */
  async function handleCitationSend(): Promise<void> {
    const text = chatStore.citationDraft;
    chatStore.citationDraft = '';

    const readiness = chatStore.citationReadiness;
    if (readiness?.embeddingBackend === 'bango_local' && !readiness.localReady) {
      void args.localEmbeddings.load();
      localPromptOpen.value = true;
      pendingSearchText.value = text;
      return;
    }

    /* Cheap pre-check: detect stored-model mismatch before searching. One
     * SELECT DISTINCT + COUNT(*) (sub-ms), safe to run on every submit. */
    try {
      const mismatch = await getModelMismatch();
      if (
        mismatch &&
        mismatch.storedModel &&
        chatStore.mismatchDismissedFor !== mismatch.storedModel
      ) {
        mismatchDialog.value = mismatch;
        pendingSearchText.value = text;
        return;
      }
    } catch {
      // Non-fatal: if the mismatch IPC fails, proceed with the search.
    }

    await args.runSearch(text);
  }

  /**
   * Confirm the mismatch dialog: regenerate the checked statuses (empty =
   * `included`) while streaming live progress into the dialog. The held
   * prose is NOT auto-submitted (the regeneration is async); it is restored
   * to the textarea for a one-click re-submit.
   */
  async function confirmMismatchRegenerate(): Promise<void> {
    if (!mismatchDialog.value || regenerating.value) return;
    regenerating.value = true;
    regeneratingProgress.value = null;
    try {
      /* Scope regeneration to the same statuses the search uses so we
       * don't wipe embeddings generated for other statuses. */
      const scope = citationStatusFilter.value.join(',');
      await regenerateEmbeddingsWithProgress(scope, (p) => {
        regeneratingProgress.value = p;
      });
      toast.show('Embeddings regenerated. Search again to use the updated vectors.', 'info');
      chatStore.citationDraft = pendingSearchText.value;
      pendingSearchText.value = '';
      /* Mark mismatch resolved so the dialog doesn't re-fire for the same
       * stored model before the regenerate completes. */
      chatStore.setMismatchDismissed(mismatchDialog.value.storedModel);
      mismatchDialog.value = null;
      // Coverage drops to 0% then climbs; refresh the readiness payload.
      void args.checkReadiness();
    } catch (e) {
      toast.show(
        `Embedding regeneration failed: ${e instanceof Error ? e.message : String(e)}`,
        'error'
      );
    } finally {
      regenerating.value = false;
      regeneratingProgress.value = null;
    }
  }

  /** Continue with the search despite the mismatch; records the dismissal
   * so the dialog does not re-fire for the same stored model this session. */
  async function continueMismatchSearch(): Promise<void> {
    if (!mismatchDialog.value) return;
    const text = pendingSearchText.value;
    chatStore.setMismatchDismissed(mismatchDialog.value.storedModel);
    mismatchDialog.value = null;
    pendingSearchText.value = '';
    await args.runSearch(text);
  }

  /** Cancel the mismatch dialog: drops the held prose without a dismissal. */
  function cancelMismatchDialog(): void {
    mismatchDialog.value = null;
    pendingSearchText.value = '';
  }

  /** Download and Continue: install the local components (progress streams
   * into the dialog), then continue the held search. On failure the dialog
   * stays open with the inline error for a retry. */
  async function confirmLocalDownload(): Promise<void> {
    if (args.localEmbeddings.installing.value) return;
    try {
      await args.localEmbeddings.install();
      localPromptOpen.value = false;
      await args.checkReadiness();
      toast.show('Bango Local embeddings are ready.', 'success');
      const text = pendingSearchText.value;
      pendingSearchText.value = '';
      await args.runSearch(text);
    } catch {
      // `localEmbeddings.error` renders inline; the user can retry/cancel.
    }
  }

  /** Use Configured Provider: switch the backend (the composable fires the
   * backend-aware probe so capability gates reopen), refresh readiness, and
   * continue the held search on the cloud backend. */
  async function confirmLocalUseCloud(): Promise<void> {
    localPromptOpen.value = false;
    const text = pendingSearchText.value;
    pendingSearchText.value = '';
    try {
      await args.localEmbeddings.selectBackend('configured_provider');
      await args.checkReadiness();
      await args.runSearch(text);
    } catch (e) {
      toast.show(
        `Could not switch embedding provider: ${e instanceof Error ? e.message : String(e)}`,
        'error'
      );
      chatStore.citationDraft = text;
    }
  }

  /** Cancel: restore the held prose to the textarea (nothing searched). */
  function cancelLocalPrompt(): void {
    localPromptOpen.value = false;
    chatStore.citationDraft = pendingSearchText.value;
    pendingSearchText.value = '';
  }

  /** Copy a citation string to the clipboard + toast. */
  async function handleCopyCitation(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      toast.show('Citation copied to clipboard.', 'success');
    } catch {
      toast.show('Failed to copy citation.', 'error');
    }
  }

  /* Reactively re-check readiness when LLM config changes (provider switch,
   * Test Connection). Deep watch because the config object is mutated in
   * place by Settings auto-save. */
  function watchLlmConfig(config: Ref<unknown>): void {
    watch(
      config,
      () => {
        void checkCitationFinderReadiness();
      },
      { deep: true }
    );
  }

  return {
    citationStatuses,
    citationStatusFilter,
    setCitationStatuses,
    isCitationMode,
    citationToggleState,
    citationToggleTitle,
    checkCitationFinderReadiness,
    onSetCitationMode,
    onToggleCitationFinder,
    handleCitationSend,
    mismatchDialog,
    regenerating,
    regeneratingProgress,
    confirmMismatchRegenerate,
    continueMismatchSearch,
    cancelMismatchDialog,
    localPromptOpen,
    confirmLocalDownload,
    confirmLocalUseCloud,
    cancelLocalPrompt,
    handleCopyCitation,
    watchLlmConfig,
  };
}
