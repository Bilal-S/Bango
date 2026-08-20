import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import { h, defineComponent, ref, KeepAlive } from 'vue';

/* View-level characterization tests for article-list.vue, pinning the two
 * behaviors that the refactor1 Tiers 1/4 changes rely on:
 * keyboard navigation and route deep-link application. The heavy composable
 * graph (use-article-search + its stores) is mocked; the view's own logic
 * (readRouteDeepLinkParams, onKeyDown) runs for real. */

const routeState = { query: {} as Record<string, string> };
const mockPush = vi.fn();

vi.mock('vue-router', () => ({
  useRoute: () => routeState,
  useRouter: () => ({ push: mockPush }),
}));

function createSearchMocks() {
  return {
    articles: ref([]),
    loading: ref(false),
    selectedArticle: ref<{ id: string } | null>(null),
    auditTrail: ref([]),
    showDetail: ref(false),
    activeStatusTab: ref('all'),
    showFilters: ref(false),
    sortColumn: ref('changedAt'),
    sortDirection: ref('desc'),
    filter: ref({
      status: 'all',
      tags: [] as string[],
      labels: [] as string[],
      yearFrom: null,
      yearTo: null,
      journal: null,
      author: null,
    }),
    statusCounts: ref<Record<string, number>>({ all: 0 }),
    allAuthors: ref<string[]>([]),
    allTags: ref<string[]>([]),
    allLabels: ref<string[]>([]),
    STATUS_TABS: [
      'all',
      'duplicate',
      'working',
      'included',
      'rejected',
      'error',
      'references',
      'search',
    ],
    search: vi.fn(async () => {}),
    fetchCounts: vi.fn(async () => {}),
    selectArticle: vi.fn(async () => {}),
    hasPrevious: ref(false),
    hasNext: ref(false),
    navigatePrev: vi.fn(),
    navigateNext: vi.fn(),
    moveArticle: vi.fn(async () => ({ didNavigate: false })),
    deleteArticle: vi.fn(async () => {}),
    clearAiReasoning: vi.fn(async () => {}),
    refreshArticle: vi.fn(async () => {}),
    updateNotes: vi.fn(async () => {}),
    updateTags: vi.fn(async () => {}),
    updateLabels: vi.fn(async () => {}),
    updateCriteria: vi.fn(async () => {}),
    updateMetadata: vi.fn(async () => {}),
    closeDetail: vi.fn(),
    setStatusTab: vi.fn(),
    toggleSort: vi.fn(),
    toggleFilters: vi.fn(),
    applyFilters: vi.fn(async () => {}),
    clearFilters: vi.fn(async () => {}),
    applyRouteParams: vi.fn(async () => {}),
    currentPage: ref(1),
    totalPages: ref(1),
    canGoPrev: ref(false),
    canGoNext: ref(false),
    goToPage: vi.fn(),
    searchText: ref(''),
    activeTotalCount: ref(0),
    isFiltered: ref(false),
    resultCount: ref(0),
    rangeStart: ref(0),
    rangeEnd: ref(0),
    pageSize: ref(50),
    changePageSize: vi.fn(),
    executeToolbarSearch: vi.fn(async () => {}),
    clearSearch: vi.fn(),
    hasReturnTarget: ref(false),
    navigateToArticle: vi.fn(),
    returnToReferencePaperId: ref<string | null>(null),
    selectedGlobalIndex: ref<number | null>(null),
    selectedIds: ref(new Set<string>()),
    selectedCount: ref(0),
    allSelected: ref(false),
    someSelected: ref(false),
    toggleSelectRange: vi.fn(),
    toggleSelectAll: vi.fn(),
    clearSelection: vi.fn(),
    bulkUpdateStatus: vi.fn(async () => 0),
    bulkAddTag: vi.fn(async () => 0),
    bulkAddLabel: vi.fn(async () => 0),
    bulkRemoveTag: vi.fn(async () => 0),
    bulkRemoveLabel: vi.fn(async () => 0),
    attachFullText: vi.fn(async () => {}),
    deleteFullTextAttachment: vi.fn(async () => {}),
    readFullTextContent: vi.fn(async () => null),
  };
}

import { shimLocalStorage } from '../helpers/fixtures';

let mocks = createSearchMocks();

vi.mock('@/composables/use-article-search', () => ({ useArticleSearch: () => mocks }));
vi.mock('@/composables/use-screening', () => ({
  useScreening: () => ({ screenArticle: vi.fn() }),
}));
vi.mock('@/composables/use-toast', () => ({ useToast: () => ({ show: vi.fn() }) }));
vi.mock('@/composables/use-feature-flags', () => ({
  useFeatureFlags: () => ({ isPremium: ref(false) }),
}));
vi.mock('@/composables/use-references', () => ({
  useBatchReferenceScraping: () => ({
    batchProgress: { isRunning: false },
    batchPercentage: ref(0),
    startBatchScraping: vi.fn(),
    cancelBatchScraping: vi.fn(),
    resetBatchProgress: vi.fn(),
  }),
}));
vi.mock('@/composables/use-full-text-attachment', () => ({
  useFullTextAttachment: () => ({ handleAttachFullText: vi.fn() }),
}));
vi.mock('@/composables/use-article-delete', () => ({
  useArticleDelete: () => ({ handleDeleteArticle: vi.fn() }),
}));
vi.mock('@/composables/use-clear-ai-reasoning', () => ({
  useClearAiReasoning: () => ({ handleClearAiReasoning: vi.fn() }),
}));
vi.mock('@/composables/use-export', () => ({
  useExport: () => ({ exportRisForIds: vi.fn(), error: ref(null) }),
}));
vi.mock('@/stores/chat', () => ({
  useChatStore: () => ({ clearSelectedArticles: vi.fn(), addSelectedArticle: vi.fn() }),
}));
vi.mock('@/composables/use-ai-summary', () => ({ requestArticleAiSummary: vi.fn() }));

type ArticleListComponent = typeof import('@/views/article-list.vue').default;
let ArticleList: ArticleListComponent | null = null;

/** KeepAlive host so `onActivated` fires and the keydown listener attaches. */
function makeHost() {
  const component = ArticleList as ArticleListComponent;
  return defineComponent({
    name: 'KeepAliveHost',
    setup() {
      return () => h(KeepAlive, () => h(component));
    },
  });
}

describe('article-list.vue', () => {
  let wrapper: VueWrapper | null = null;

  beforeEach(() => {
    routeState.query = {};
    mockPush.mockReset();
    mocks = createSearchMocks();
    /* happy-dom-safe localStorage (bare `localStorage.getItem` is broken in
     * this environment; see helpers/fixtures.ts). */
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
  });

  afterEach(() => {
    wrapper?.unmount();
    wrapper = null;
  });

  async function mountView(): Promise<VueWrapper> {
    if (!ArticleList) ArticleList = (await import('@/views/article-list.vue')).default;
    const mounted = mount(makeHost(), {
      global: {
        stubs: {
          ArticleToolbar: true,
          ArticleTable: true,
          ArticleDetailPanel: true,
          ArticleFilterPanel: true,
          BulkActionBar: true,
          ExportDialog: true,
          SuggestInput: true,
          ReferencesView: true,
          OpenAlexSearch: true,
          BatchRefProgress: true,
        },
      },
    });
    await flushPromises();
    return mounted;
  }

  it('route_deep_link_params_apply_filters', async () => {
    routeState.query = {
      status: 'included',
      tags: 'ml,dl',
      labels: 'priority',
      yearFrom: '2020',
      yearTo: '2022',
      journal: 'Nature',
      author: 'Smith',
    };
    wrapper = await mountView();

    expect(mocks.applyRouteParams).toHaveBeenCalledTimes(1);
    expect(mocks.applyRouteParams).toHaveBeenCalledWith(
      expect.objectContaining({
        status: 'included',
        tags: ['ml', 'dl'],
        labels: ['priority'],
        yearFrom: 2020,
        yearTo: 2022,
        journal: 'Nature',
        author: 'Smith',
        filterCollapsed: false,
        resetFilters: false,
      })
    );
    /* Deep-link path suppresses the default initial search. */
    expect(mocks.search).not.toHaveBeenCalled();

    /* articleId-only deep-link (dashboard "Go to article") selects directly. */
    mocks = createSearchMocks();
    wrapper?.unmount();
    routeState.query = { articleId: 'a-42' };
    wrapper = await mountView();
    expect(mocks.selectArticle).toHaveBeenCalledWith('a-42');
    expect(mocks.applyRouteParams).not.toHaveBeenCalled();

    /* No params: plain initial search runs. */
    mocks = createSearchMocks();
    wrapper?.unmount();
    routeState.query = {};
    wrapper = await mountView();
    expect(mocks.search).toHaveBeenCalledTimes(1);
    expect(mocks.applyRouteParams).not.toHaveBeenCalled();
  });

  it('keyboard_navigation_moves_selection', async () => {
    wrapper = await mountView();

    mocks.hasPrevious.value = true;
    mocks.hasNext.value = true;

    /* Table arrows: up/down move row selection (with preventDefault). */
    const down = new KeyboardEvent('keydown', {
      key: 'ArrowDown',
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(down);
    expect(mocks.navigateNext).toHaveBeenCalledTimes(1);
    expect(down.defaultPrevented).toBe(true);

    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true })
    );
    expect(mocks.navigatePrev).toHaveBeenCalledTimes(1);

    /* Detail-panel arrows take over when the panel is open. */
    mocks.showDetail.value = true;
    mocks.selectedArticle.value = { id: 'a-1' };
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true })
    );
    expect(mocks.navigatePrev).toHaveBeenCalledTimes(2);
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })
    );
    expect(mocks.navigateNext).toHaveBeenCalledTimes(2);
    mocks.showDetail.value = false;
    mocks.selectedArticle.value = null;

    /* Typing in an input is never hijacked. */
    const input = document.createElement('input');
    document.body.appendChild(input);
    input.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })
    );
    expect(mocks.navigateNext).toHaveBeenCalledTimes(2);
    input.remove();

    /* References/Search tabs own their data - table arrows are ignored. */
    mocks.activeStatusTab.value = 'references';
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })
    );
    expect(mocks.navigateNext).toHaveBeenCalledTimes(2);
  });
});
