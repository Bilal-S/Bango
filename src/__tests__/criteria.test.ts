import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useCriteriaStore } from '@/stores/criteria';
import type { ResearchAim, Criterion } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const mockAims: ResearchAim[] = [{ id: 'a1', text: 'Aim 1', createdAt: '2023-01-01' }];

const mockCriteria: Criterion[] = [
  {
    id: 'c1',
    text: 'Include ML',
    criterionType: 'inclusion',
    priority: 'standard',
    createdAt: '2023-01-01',
  },
  {
    id: 'c2',
    text: 'Exclude non-English',
    criterionType: 'exclusion',
    priority: 'high',
    createdAt: '2023-01-01',
  },
];

describe('Criteria Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('fetches aims and criteria correctly', async () => {
    const store = useCriteriaStore();
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_research_aims') return Promise.resolve(mockAims);
      if (cmd === 'get_criteria') return Promise.resolve(mockCriteria);
      return Promise.resolve([]);
    });

    await store.fetchAll();

    expect(store.aims).toEqual(mockAims);
    expect(store.criteria).toEqual(mockCriteria);
    expect(store.inclusionCriteria).toHaveLength(1);
    expect(store.inclusionCriteria[0]!.text).toBe('Include ML');
    expect(store.exclusionCriteria).toHaveLength(1);
    expect(store.exclusionCriteria[0]!.text).toBe('Exclude non-English');
    expect(store.initialized).toBe(true);
  });

  it('filters inclusion and exclusion criteria correctly', async () => {
    const store = useCriteriaStore();
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_research_aims') return Promise.resolve([]);
      if (cmd === 'get_criteria') return Promise.resolve(mockCriteria);
      return Promise.resolve([]);
    });

    await store.fetchAll();

    expect(store.inclusionCriteria.every((c) => c.criterionType === 'inclusion')).toBe(true);
    expect(store.exclusionCriteria.every((c) => c.criterionType === 'exclusion')).toBe(true);
  });

  it('invalidates state correctly', async () => {
    const store = useCriteriaStore();
    vi.mocked(tauriCommand).mockResolvedValue([]);

    await store.fetchAll();
    store.invalidate();

    expect(store.aims).toEqual([]);
    expect(store.criteria).toEqual([]);
    expect(store.inclusionCriteria).toEqual([]);
    expect(store.exclusionCriteria).toEqual([]);
    expect(store.initialized).toBe(false);
  });
});
