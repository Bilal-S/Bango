import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useCriteriaStore } from '@/stores/criteria';
import type { Criterion, ResearchAim } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const aims: ResearchAim[] = [{ id: 'aim1', text: 'Aim one', createdAt: '2026-01-01' }];
const criteria: Criterion[] = [
  {
    id: 'c1',
    criterionType: 'inclusion',
    text: 'Must include',
    priority: 'critical',
    createdAt: '2026-01-01',
  },
  {
    id: 'c2',
    criterionType: 'exclusion',
    text: 'Exclude animals',
    priority: 'high',
    createdAt: '2026-01-01',
  },
];

describe('useCriteriaStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts empty', () => {
    const store = useCriteriaStore();
    expect(store.aims).toEqual([]);
    expect(store.criteria).toEqual([]);
    expect(store.inclusionCriteria).toEqual([]);
    expect(store.exclusionCriteria).toEqual([]);
    expect(store.initialized).toBe(false);
    expect(store.loading).toBe(false);
  });

  it('fetchAll populates aims and splits criteria by type', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_research_aims') return Promise.resolve(aims);
      if (cmd === 'get_criteria') return Promise.resolve(criteria);
      return Promise.resolve([]);
    });

    const store = useCriteriaStore();
    await store.fetchAll();

    expect(store.aims).toEqual(aims);
    expect(store.criteria).toHaveLength(2);
    expect(store.inclusionCriteria).toHaveLength(1);
    expect(store.inclusionCriteria[0]!.id).toBe('c1');
    expect(store.exclusionCriteria).toHaveLength(1);
    expect(store.exclusionCriteria[0]!.id).toBe('c2');
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
  });

  it('fetchIfNeeded does nothing when initialized', async () => {
    const store = useCriteriaStore();
    store.initialized = true;
    await store.fetchIfNeeded();
    expect(tauriCommand).not.toHaveBeenCalled();
  });

  it('refresh re-fetches without clearing first', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_research_aims') return Promise.resolve(aims);
      if (cmd === 'get_criteria') return Promise.resolve(criteria);
      return Promise.resolve([]);
    });

    const store = useCriteriaStore();
    await store.refresh();

    expect(store.criteria).toHaveLength(2);
    expect(store.initialized).toBe(true);
  });

  it('criterionIndexMap numbers inclusion then exclusion', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_research_aims') return Promise.resolve([]);
      if (cmd === 'get_criteria') return Promise.resolve(criteria);
      return Promise.resolve([]);
    });

    const store = useCriteriaStore();
    await store.fetchAll();

    expect(store.criterionIndexMap.get('c1')).toBe(1);
    expect(store.criterionIndexMap.get('c2')).toBe(2);
  });

  it('invalidate resets state', async () => {
    vi.mocked(tauriCommand).mockResolvedValue([]);
    const store = useCriteriaStore();
    store.aims = aims;
    store.criteria = criteria;
    store.inclusionCriteria = [criteria[0]!];
    store.initialized = true;
    store.invalidate();
    expect(store.aims).toEqual([]);
    expect(store.criteria).toEqual([]);
    expect(store.inclusionCriteria).toEqual([]);
    expect(store.exclusionCriteria).toEqual([]);
    expect(store.initialized).toBe(false);
  });

  it('invalidate resets customLogic cache so loadCustomLogic re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue('imported rules');
    const store = useCriteriaStore();
    // Simulate a prior load: customLogic is populated and the one-shot guard
    // is latched (the state after visiting the Criteria view once).
    store.customLogic = 'old rules';
    store.customLogicLoaded = true;
    // invalidate() must clear both so the next loadCustomLogic() fetches.
    store.invalidate();
    expect(store.customLogic).toBe('');
    expect(store.customLogicLoaded).toBe(false);
    await store.loadCustomLogic();
    expect(tauriCommand).toHaveBeenCalledWith('get_screening_custom_logic');
    expect(store.customLogic).toBe('imported rules');
  });

  it('exposes AI assistant state', () => {
    const store = useCriteriaStore();
    expect(store.generatingInclusion).toBe(false);
    expect(store.generatingExclusion).toBe(false);
    expect(store.inclusionCritique).toBe('');
    expect(store.exclusionCritique).toBe('');
    expect(store.inclusionError).toBeNull();
    expect(store.exclusionError).toBeNull();
    // Mutate to confirm reactive setters exist
    store.generatingInclusion = true;
    store.inclusionCritique = 'critique';
    store.inclusionError = 'err';
    expect(store.generatingInclusion).toBe(true);
    expect(store.inclusionCritique).toBe('critique');
    expect(store.inclusionError).toBe('err');
  });
});
