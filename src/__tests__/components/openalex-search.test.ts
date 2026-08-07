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
/* Stub ClearableInput as a plain input so year-range interactions don't pull
 * in extra component complexity; the options-panel tests assert on store calls,
 * not on the input's own clear affordance. */
vi.mock('@/components/clearable-input.vue', () => ({
  default: {
    name: 'ClearableInput',
    props: ['modelValue', 'type', 'min', 'max', 'placeholder', 'inputClass', 'disabled', 'title'],
    emits: ['update:modelValue', 'clear', 'enter', 'input', 'focus', 'blur'],
    template:
      '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
  },
}));

import OpenAlexSearch from '@/components/openalex-search.vue';
import { DEFAULT_OPENALEX_FILTERS, type OpenAlexFilters } from '@/types/openalex';

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
  filters?: OpenAlexFilters;
  smartSearchAvailable?: boolean;
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
    filters: overrides.filters ?? { ...DEFAULT_OPENALEX_FILTERS, workTypes: [] },
    selectedResultId: null,
    selectedIds: new Set(['W1', 'W2']),
    loading: false,
    error: null,
    smartSearchLoading: false,
    smartSearchAvailable: overrides.smartSearchAvailable ?? false,
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

describe('openalex-search.vue - Search Options panel', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    toastShow.mockReset();
    holder.current = null;
  });

  /** Find a button whose text contains the given substring (case-sensitive). */
  function findButtonByText(wrapper: ReturnType<typeof mount>, text: string) {
    return wrapper.findAll('button').find((b) => b.text().includes(text));
  }

  /** The panel body uses v-show; in the test DOM `isVisible()` is unreliable
   *  for v-show, so use the header's `aria-expanded` as the collapsed/expanded
   *  source of truth. The body + chips always exist in the DOM (v-show keeps
   *  them), so `.exists()` is true in both states. */
  it('options panel is collapsed by default and the SEARCH OPTIONS header expands it', async () => {
    holder.current = createStoreMock({
      importSelected: vi.fn(() => Promise.resolve(null)),
    });
    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    // Collapsed by default: aria-expanded=false; body + chips exist in the DOM
    // (v-show) so chip count is > 0 even before expanding.
    expect(wrapper.find('.options-header').attributes('aria-expanded')).toBe('false');
    expect(wrapper.findAll('.chip').length).toBeGreaterThan(0);

    // Expand.
    await wrapper.find('.options-header').trigger('click');
    await flushPromises();
    expect(wrapper.find('.options-header').attributes('aria-expanded')).toBe('true');
  });

  it('collapsed header shows the active-option count when options are active', async () => {
    holder.current = createStoreMock({
      importSelected: vi.fn(() => Promise.resolve(null)),
      filters: { ...DEFAULT_OPENALEX_FILTERS, workTypes: ['article'], isOa: true },
    });
    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    // Collapsed: centered count visible with full phrasing; aria-expanded=false.
    // panelFilters is seeded from the store at setup, so committed filters are
    // reflected immediately in the header count.
    const count = wrapper.find('.options-header__count');
    expect(count.exists()).toBe(true);
    expect(count.text()).toBe('2 options selected');
    expect(wrapper.find('.options-header').attributes('aria-expanded')).toBe('false');

    // Expanding hides the count and flips aria-expanded.
    await wrapper.find('.options-header').trigger('click');
    await flushPromises();
    expect(wrapper.find('.options-header__count').exists()).toBe(false);
    expect(wrapper.find('.options-header').attributes('aria-expanded')).toBe('true');
  });

  it('uncommitted panel edits survive a collapse/re-expand (v-show persistence)', async () => {
    holder.current = createStoreMock({
      importSelected: vi.fn(() => Promise.resolve(null)),
    });
    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    // Expand, select the Article chip (no Apply yet).
    await wrapper.find('.options-header').trigger('click');
    await flushPromises();
    const articleChip = wrapper.findAll('.chip').find((c) => c.text().includes('Article'));
    await articleChip!.trigger('click');
    await flushPromises();
    expect(articleChip!.classes()).toContain('chip--on');

    // Collapse then re-expand without Apply.
    await wrapper.find('.options-header').trigger('click');
    await flushPromises();
    await wrapper.find('.options-header').trigger('click');
    await flushPromises();

    // The selection must still be present (no re-seed-on-expand wipe).
    const chipAfter = wrapper.findAll('.chip').find((c) => c.text().includes('Article'));
    expect(chipAfter!.classes()).toContain('chip--on');
  });

  it('toggling a Work Type chip then Apply calls store.setFilters with the type', async () => {
    const setFilters = vi.fn();
    holder.current = {
      ...createStoreMock({ importSelected: vi.fn(() => Promise.resolve(null)) }),
      setFilters,
    };
    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    await wrapper.find('.options-header').trigger('click');
    await flushPromises();

    // Click the "Article" chip (it is unselected by default).
    const articleChip = wrapper.findAll('.chip').find((c) => c.text().includes('Article'));
    expect(articleChip).toBeDefined();
    await articleChip!.trigger('click');
    await flushPromises();

    // Apply.
    await findButtonByText(wrapper, 'Apply')!.trigger('click');
    await flushPromises();

    expect(setFilters).toHaveBeenCalledTimes(1);
    const arg = setFilters.mock.calls[0]![0] as OpenAlexFilters;
    expect(arg.workTypes).toContain('article');
  });

  it('enabling Open Access then Apply sets isOa true', async () => {
    const setFilters = vi.fn();
    holder.current = {
      ...createStoreMock({ importSelected: vi.fn(() => Promise.resolve(null)) }),
      setFilters,
    };
    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    await wrapper.find('.options-header').trigger('click');
    await flushPromises();

    const oaCheckbox = wrapper.find('label.options-toggle input[type="checkbox"]');
    await oaCheckbox.setValue(true);
    expect((oaCheckbox.element as HTMLInputElement).checked).toBe(true);

    await findButtonByText(wrapper, 'Apply')!.trigger('click');
    await flushPromises();

    const arg = setFilters.mock.calls[0]![0] as OpenAlexFilters;
    expect(arg.isOa).toBe(true);
  });

  it('Clear options resets all panel filters to defaults', async () => {
    const setFilters = vi.fn();
    holder.current = {
      ...createStoreMock({
        importSelected: vi.fn(() => Promise.resolve(null)),
        filters: { ...DEFAULT_OPENALEX_FILTERS, workTypes: ['article'], isOa: true },
      }),
      setFilters,
    };
    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    // Collapsed header shows the active count (seeded from the store).
    expect(wrapper.find('.options-header__count').exists()).toBe(true);

    await wrapper.find('.options-header').trigger('click');
    await flushPromises();
    await findButtonByText(wrapper, 'Clear options')!.trigger('click');
    await flushPromises();

    // Clear options commits defaults immediately and the count disappears.
    expect(setFilters).toHaveBeenCalled();
    const calls = setFilters.mock.calls;
    const lastArg = calls[calls.length - 1]![0] as OpenAlexFilters;
    expect(lastArg.workTypes).toEqual([]);
    expect(lastArg.isOa).toBe(false);
  });

  it('Smart Search button renders in the input row when LLM is configured', async () => {
    holder.current = createStoreMock({
      smartSearchAvailable: true,
      importSelected: vi.fn(() => Promise.resolve(null)),
    });
    const wrapper = mount(OpenAlexSearch, { global: { plugins: [createPinia()] } });
    await flushPromises();

    const smart = wrapper.find('.oa-smart-search');
    expect(smart.exists()).toBe(true);
    expect(smart.text()).toContain('Smart Search');
  });
});
