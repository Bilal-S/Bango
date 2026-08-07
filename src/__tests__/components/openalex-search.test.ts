import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import type { OpenAlexResultItem } from '@/types/openalex';

/* Holder indirection so the hoisted `vi.mock` factory can return a per-test
 * store mock. The factory runs lazily when the component calls
 * `useOpenAlexStore()` at setup (i.e. at `mount()`), by which point each test
 * has installed its mock via `holder.current = ...`. */
const { holder } = vi.hoisted(() => ({ holder: { current: null as unknown } }));

vi.mock('@/stores/openalex', () => ({
  useOpenAlexStore: () => holder.current,
}));

const toastShow = vi.fn();
vi.mock('@/composables/use-toast', () => ({
  useToast: () => ({ show: toastShow }),
}));

/* Stub child components - the lifecycle test only needs the action bar. */
vi.mock('@/components/openalex-result-item.vue', () => ({
  default: { name: 'OpenAlexResultItem', template: '<div />' },
}));
vi.mock('@/components/openalex-detail-panel.vue', () => ({
  default: { name: 'OpenAlexDetailPanel', template: '<div />' },
}));

import OpenAlexSearch from '@/components/openalex-search.vue';

function makeResult(id: string): OpenAlexResultItem {
  return {
    work: {
      id,
      doi: `10.1234/${id}`,
      title: `Article ${id}`,
      publicationYear: 2024,
      publicationDate: '2024-01-01',
      authorships: [],
      primaryLocation: null,
      abstractInvertedIndex: null,
      biblio: null,
      citedByCount: 0,
      language: 'en',
      keywords: [],
      type: 'article',
      openAccess: null,
      isRetracted: false,
      referencedWorks: [],
    },
    abstractText: 'Abstract text.',
    snippet: 'Abstract text.',
    alreadyInLibrary: false,
  };
}

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (v: T) => void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

/** Build a plain (non-ref) store mock. Template nested access (`store.loading`
 * etc.) works with plain values; reactivity in these tests comes from the
 * component's own `importing` ref. */
function createStoreMock(overrides: {
  selectedCount?: number;
  importSelected: () => Promise<{ importedCount: number; skippedCount: number } | null>;
}) {
  const results = [makeResult('W1'), makeResult('W2')];
  return {
    query: '',
    results,
    totalCount: results.length,
    currentPage: 1,
    perPage: 25,
    sortBy: 'relevance_score:desc',
    filters: {},
    selectedResultId: null,
    selectedIds: new Set(['W1', 'W2']),
    loading: false,
    error: null,
    smartSearchLoading: false,
    smartSearchAvailable: false,
    hasSearched: true,
    settings: { hasApiKey: false, mailto: '', retrieveReferences: false },
    totalPages: 1,
    selectedResult: null,
    selectedCount: overrides.selectedCount ?? 2,
    selectableCount: results.length,
    cappedTotalCount: results.length,
    search: vi.fn(),
    setQuery: vi.fn(),
    setFilters: vi.fn(),
    setSort: vi.fn(),
    setPerPage: vi.fn(),
    goToPage: vi.fn(),
    selectResult: vi.fn(),
    toggleSelection: vi.fn(),
    selectAll: vi.fn(),
    clearSelection: vi.fn(),
    clearSearch: vi.fn(),
    importSelected: overrides.importSelected,
    importSingle: vi.fn(),
    refreshLibraryFlags: vi.fn(),
    loadSettings: vi.fn(() => Promise.resolve()),
    saveSettings: vi.fn(),
    smartSearch: vi.fn(),
  };
}

function findAddToWorkingButton(wrapper: ReturnType<typeof mount>) {
  const buttons = wrapper.findAll('button');
  return buttons.find((b) => b.text().includes('Add to Working') || b.text().includes('Adding...'));
}

describe('openalex-search.vue - bulk "Add to Working" lifecycle', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    toastShow.mockReset();
    holder.current = null;
  });

  it('renders "Add to Working" and is enabled when articles are selected', async () => {
    holder.current = createStoreMock({
      importSelected: vi.fn(() => Promise.resolve({ importedCount: 2, skippedCount: 0 })),
    });
    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    const btn = findAddToWorkingButton(wrapper);
    expect(btn).toBeDefined();
    expect(btn!.text()).toBe('Add to Working');
    expect(btn!.attributes('disabled')).toBeUndefined();
  });

  it('shows "Adding..." and disables while importSelected is in flight, then restores', async () => {
    const { promise, resolve } = deferred<{
      importedCount: number;
      skippedCount: number;
    } | null>();
    const importSelected = vi.fn(() => promise);
    holder.current = createStoreMock({ importSelected });

    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    const btn = findAddToWorkingButton(wrapper);
    expect(btn).toBeDefined();
    await btn!.trigger('click');
    await flushPromises();

    // Mid-flight: label flips to "Adding..." and the `importing` flag disables
    // the button (selectedCount is still > 0 in the mock, so only `importing`
    // accounts for the disabled attribute).
    const inFlightBtn = findAddToWorkingButton(wrapper);
    expect(inFlightBtn!.text()).toBe('Adding...');
    expect(inFlightBtn!.attributes('disabled')).toBeDefined();
    expect(importSelected).toHaveBeenCalledTimes(1);

    // Resolve the import. The mock keeps selectedCount at 2 (no real
    // clearSelection wiring), so once `importing` resets the button re-enables
    // with its original label.
    resolve({ importedCount: 2, skippedCount: 0 });
    await flushPromises();

    const settledBtn = findAddToWorkingButton(wrapper);
    expect(settledBtn!.text()).toBe('Add to Working');
    expect(settledBtn!.attributes('disabled')).toBeUndefined();
    expect(toastShow).toHaveBeenCalledWith(
      expect.stringContaining('Added 2 article(s) to Working list'),
      'success'
    );
  });

  it('re-enables even when importSelected resolves null (nothing imported)', async () => {
    const importSelected = vi.fn(() => Promise.resolve(null));
    holder.current = createStoreMock({ importSelected });

    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    const btn = findAddToWorkingButton(wrapper);
    await btn!.trigger('click');
    await flushPromises();

    // `importing` resets in finally regardless of the null result; no toast.
    expect(findAddToWorkingButton(wrapper)!.attributes('disabled')).toBeUndefined();
    expect(toastShow).not.toHaveBeenCalled();
  });
});
