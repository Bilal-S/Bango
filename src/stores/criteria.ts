import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { ResearchAim, Criterion } from '@/types';
import type { SearchStrategyResult } from '@/types/search-strategy';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

export const useCriteriaStore = defineStore('criteria', () => {
  const aims = ref<ResearchAim[]>([]);
  const criteria = ref<Criterion[]>([]);
  const loading = ref(false);
  const initialized = ref(false);

  const inclusionCriteria = ref<Criterion[]>([]);
  const exclusionCriteria = ref<Criterion[]>([]);

  // ── AI assistant state (persists across route navigation) ──────────
  const generatingInclusion = ref(false);
  const generatingExclusion = ref(false);
  const inclusionCritique = ref('');
  const exclusionCritique = ref('');
  const inclusionError = ref<string | null>(null);
  const exclusionError = ref<string | null>(null);

  // ── Custom Screening Instructions + Check Rules state ─────────────
  // `customLogic` is loaded once on mount and persisted via Save. The
  // rules-check critique mirrors the inclusion/exclusion critique pattern
  // (persists across route navigation, dismissible, collapsible).
  const customLogic = ref('');
  const customLogicLoaded = ref(false);
  const generatingRulesCheck = ref(false);
  const rulesCritique = ref('');
  const rulesError = ref<string | null>(null);

  // ── Search Strategy Builder state (session-scoped, NOT persisted to DB) ──
  // Mirrors the inclusion/exclusion critique pattern: survives route
  // navigation so the user can leave the Criteria view and come back, clears
  // on app close. The audit entry is the only durable record of a run.
  const generatingSearchStrategy = ref(false);
  const searchStrategyResult = ref<SearchStrategyResult | null>(null);
  const searchStrategyError = ref<string | null>(null);

  // ── Critique card collapse state (session-scoped, mirrors the critique
  // refs above). Persists the user's expand/collapse choice across route
  // navigation so leaving and returning to the Criteria view preserves it.
  // Default true so a freshly-generated critique shows expanded.
  const inclusionCritiqueExpanded = ref(true);
  const exclusionCritiqueExpanded = ref(true);
  const rulesCritiqueExpanded = ref(true);

  /** Load the persisted custom screening-instructions text (once). */
  async function loadCustomLogic(): Promise<void> {
    if (customLogicLoaded.value || !isTauri()) return;
    try {
      const value = await tauriCommand<string | null>('get_screening_custom_logic');
      customLogic.value = value ?? '';
    } catch {
      customLogic.value = '';
    } finally {
      customLogicLoaded.value = true;
    }
  }

  /** Persist the custom screening-instructions text. Trims surrounding whitespace. */
  async function saveCustomLogic(text: string): Promise<void> {
    await tauriCommand('set_screening_custom_logic', { value: text });
    customLogic.value = text;
  }

  /** Run the holistic ruleset consistency review (Check Rules button). */
  async function runRulesCheck(): Promise<void> {
    generatingRulesCheck.value = true;
    rulesError.value = null;
    try {
      const result = await tauriCommand<{ critique: string }>('check_rules');
      rulesCritique.value = result.critique;
      rulesCritiqueExpanded.value = true;
    } catch (e: unknown) {
      rulesError.value = e instanceof Error ? e.message : String(e);
    } finally {
      generatingRulesCheck.value = false;
    }
  }

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetchAll();
  }

  async function fetchAll(): Promise<void> {
    loading.value = true;
    try {
      const [aimsResult, criteriaResult] = await Promise.all([
        tauriCommand<ResearchAim[]>('get_research_aims'),
        tauriCommand<Criterion[]>('get_criteria'),
      ]);
      aims.value = aimsResult;
      criteria.value = criteriaResult;
      inclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'inclusion');
      exclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'exclusion');
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  function invalidate(): void {
    aims.value = [];
    criteria.value = [];
    inclusionCriteria.value = [];
    exclusionCriteria.value = [];
    initialized.value = false;
    // Reset the custom-logic cache so a project import / reset re-fetches the
    // value from the backend instead of short-circuiting on the one-shot
    // `customLogicLoaded` guard in `loadCustomLogic`.
    customLogic.value = '';
    customLogicLoaded.value = false;
  }

  /** Re-fetch from backend without clearing arrays first (preserves scroll position). */
  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      const [aimsResult, criteriaResult] = await Promise.all([
        tauriCommand<ResearchAim[]>('get_research_aims'),
        tauriCommand<Criterion[]>('get_criteria'),
      ]);
      aims.value = aimsResult;
      criteria.value = criteriaResult;
      inclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'inclusion');
      exclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'exclusion');
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  const criterionIndexMap = computed(() => {
    const map = new Map<string, number>();
    let n = 1;
    for (const c of inclusionCriteria.value) {
      map.set(c.id, n++);
    }
    for (const c of exclusionCriteria.value) {
      map.set(c.id, n++);
    }
    return map;
  });

  return {
    aims,
    criteria,
    inclusionCriteria,
    exclusionCriteria,
    criterionIndexMap,
    loading,
    initialized,
    // AI assistant state
    generatingInclusion,
    generatingExclusion,
    inclusionCritique,
    exclusionCritique,
    inclusionError,
    exclusionError,
    // Custom Screening Instructions + Check Rules state
    customLogic,
    customLogicLoaded,
    generatingRulesCheck,
    rulesCritique,
    rulesError,
    rulesCritiqueExpanded,
    loadCustomLogic,
    saveCustomLogic,
    runRulesCheck,
    // Search Strategy Builder state
    generatingSearchStrategy,
    searchStrategyResult,
    searchStrategyError,
    // Critique card collapse state (session-scoped)
    inclusionCritiqueExpanded,
    exclusionCritiqueExpanded,
    fetchIfNeeded,
    fetchAll,
    invalidate,
    refresh,
  };
});
