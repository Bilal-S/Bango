import { computed, ref, type ComputedRef } from 'vue';
import { useLlmConfigured } from '@/composables/use-llm-configured';

/**
 * Dashboard CTA button state. Priority order:
 *   1. `connect_llm`    - LLM not configured
 *   2. `start_screening` - LLM ok AND working articles await screening
 *   3. `build_wiki`     - LLM ok, screening done, wiki not yet built
 *   4. `review_wiki`    - LLM ok, screening done, wiki exists
 */
type DashboardCtaState = 'connect_llm' | 'start_screening' | 'build_wiki' | 'review_wiki';

/** Dashboard CTA button descriptor resolved by {@link useDashboardCta}. */
interface DashboardCta {
  /** Material Symbols icon name (reused from the existing icon set). */
  icon: string;
  /** Button label. */
  label: string;
  /** Route to navigate to on click. */
  route: string;
  /** The resolved state (for tests / debugging). */
  state: DashboardCtaState;
}

/** CTA button manifest (icon + label + route) by state. */
const CTA_BY_STATE: Record<DashboardCtaState, DashboardCta> = {
  connect_llm: { icon: 'link', label: 'Connect LLM', route: '/settings', state: 'connect_llm' },
  start_screening: {
    icon: 'play_arrow',
    label: 'Start AI Screening',
    route: '/screening',
    state: 'start_screening',
  },
  build_wiki: {
    icon: 'local_library',
    label: 'Build Wiki',
    route: '/wiki',
    state: 'build_wiki',
  },
  review_wiki: {
    icon: 'local_library',
    label: 'Review Wiki',
    route: '/wiki',
    state: 'review_wiki',
  },
};

export interface DashboardCtaDeps {
  /** Count of articles currently in the Working status. */
  workingCount: ComputedRef<number>;
}

/**
 * Dashboard primary-button (CTA) resolution (refactor1 T4.3): pure computed
 * over the canonical `useLlmConfigured()` gate, the working-article count,
 * and the wiki-built flag. `useDashboard` re-exposes the returned refs
 * unchanged.
 */
export function useDashboardCta(deps: DashboardCtaDeps) {
  const { workingCount } = deps;

  /**
   * Reactive "is the LLM configured?" gate sourced from the canonical Pinia
   * store. Replaces the one-shot `has_llm_config` IPC probe.
   */
  const llmConfigured = useLlmConfigured();
  /**
   * True when the wiki is initialized AND has at least one generated page
   * (mirrors the `chat-view.vue` wikiReady test: `initialized && pageCount > 0`).
   * Owned here; `useDashboard.refresh()` is the sole writer.
   */
  const wikiBuilt = ref(false);

  /** Resolved CTA state (pure computed over the three signals above). */
  const ctaState = computed<DashboardCtaState>(() => {
    if (!llmConfigured.value) return 'connect_llm';
    if (workingCount.value > 0) return 'start_screening';
    if (!wikiBuilt.value) return 'build_wiki';
    return 'review_wiki';
  });

  const cta = computed<DashboardCta>(() => CTA_BY_STATE[ctaState.value]);

  return { llmConfigured, wikiBuilt, ctaState, cta };
}
