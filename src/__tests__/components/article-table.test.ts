import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import ArticleTable from '@/components/article-table.vue';
import type { Article } from '@/types';

// Stub child components - we only care about the exposed scroll surface,
// not the rendered chips/badges.
vi.mock('@/components/status-badge.vue', () => ({
  default: { name: 'StatusBadge', template: '<span/>' },
}));
vi.mock('@/components/confidence-bar.vue', () => ({
  default: { name: 'ConfidenceBar', template: '<span/>' },
}));

function makeArticle(id: string, sequenceId: number): Article {
  return {
    id,
    sequenceId,
    status: 'included',
    screeningError: false,
    title: `Article ${id}`,
    abstractText: '',
    authors: ['Doe J'],
    publicationYear: 2021,
    doi: null,
    journal: null,
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    keywords: [],
    url: null,
    language: null,
    publisher: null,
    publisherCity: null,
    publisherAddress: null,
    issn: null,
    eissn: null,
    journalIndexId: null,
    referenceType: 'JOUR',
    date: null,
    authorAddress: null,
    affiliation: null,
    accessionNumber: null,
    customField3: null,
    journalAbbreviation: null,
    journalIsoAbbreviation: null,
    notes: null,
    webOfScienceDb: null,
    userNotes: null,
    risExtras: null,
    duplicateOf: null,
    aiDecision: null,
    aiReasoning: null,
    aiConfidence: null,
    matchedInclusionCriteria: [],
    matchedExclusionCriteria: [],
    tags: [],
    labels: [],
    manualOverride: false,
    importSource: null,
    importedAt: '',
    changedAt: '',
    screenedAt: null,
    dataLength: null,
    tokenEstimate: null,
    actualTokens: null,
    fullText: null,
    fullTextAiSummary: null,
    numCited: null,
    numReferences: null,
    hasCitationDetails: false,
    hasReferenceDetails: false,
    hasFullText: false,
    fullTextFileName: null,
    hasFiguresOrTables: false,
    isTranslated: false,
    translationStatus: 'none',
    translationError: null,
    translatedAt: null,
  } as Article;
}

/**
 * Simulate horizontal overflow on the scroll container so the
 * `canScrollLeft` / `canScrollRight` flags reflect real geometry. jsdom
 * reports 0 for all scroll metrics by default, so without this override the
 * exposed flags would always be false.
 */
function simulateOverflow(
  el: HTMLElement,
  scrollLeft: number,
  clientWidth: number,
  scrollWidth: number
): void {
  Object.defineProperty(el, 'scrollLeft', { configurable: true, get: () => scrollLeft });
  Object.defineProperty(el, 'clientWidth', { configurable: true, get: () => clientWidth });
  Object.defineProperty(el, 'scrollWidth', { configurable: true, get: () => scrollWidth });
  // jsdom does not implement scrollBy; stub it so scrollTable() does not throw.
  el.scrollBy = vi.fn();
}

describe('article-table.vue - exposed scroll surface', () => {
  beforeEach(() => {
    // jsdom lacks ResizeObserver; the component instantiates one in onMounted
    // via `new ResizeObserver(cb)`. Provide a constructable mock.
    class FakeResizeObserver {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    }
    globalThis.ResizeObserver = FakeResizeObserver as unknown as typeof ResizeObserver;
  });

  function mountTable() {
    return mount(ArticleTable, {
      props: {
        articles: [makeArticle('a1', 1), makeArticle('a2', 2)],
        selectedId: null,
        sortColumn: null,
        sortDirection: 'asc' as const,
        selectedIds: new Set<string>(),
        allSelected: false,
        someSelected: false,
      },
      attachTo: document.body,
    });
  }

  it('exposes scrollTable + canScrollLeft + canScrollRight', () => {
    const wrapper = mountTable();
    const vm = wrapper.vm as unknown as {
      scrollTable: (d: 'left' | 'right') => void;
      canScrollLeft: boolean;
      canScrollRight: boolean;
    };
    expect(typeof vm.scrollTable).toBe('function');
    // Vue auto-unwraps exposed refs on the public instance.
    expect(typeof vm.canScrollLeft).toBe('boolean');
    expect(typeof vm.canScrollRight).toBe('boolean');
  });

  it('reports canScrollLeft=false / canScrollRight=true when scrolled to the start of an overflowing table', async () => {
    const wrapper = mountTable();
    const scrollEl = wrapper.find('.article-table-scroll').element as HTMLElement;
    // Content wider than the viewport; scrolled to the left edge.
    simulateOverflow(scrollEl, 0, 100, 400);
    // Trigger the scroll listener the same way the browser would.
    scrollEl.dispatchEvent(new Event('scroll'));
    await wrapper.vm.$nextTick();

    const vm = wrapper.vm as unknown as { canScrollLeft: boolean; canScrollRight: boolean };
    expect(vm.canScrollLeft).toBe(false);
    expect(vm.canScrollRight).toBe(true);
  });

  it('reports canScrollLeft=true / canScrollRight=false when scrolled to the far right', async () => {
    const wrapper = mountTable();
    const scrollEl = wrapper.find('.article-table-scroll').element as HTMLElement;
    // Scrolled to the far right edge (scrollWidth - clientWidth = 300).
    simulateOverflow(scrollEl, 300, 100, 400);
    scrollEl.dispatchEvent(new Event('scroll'));
    await wrapper.vm.$nextTick();

    const vm = wrapper.vm as unknown as { canScrollLeft: boolean; canScrollRight: boolean };
    expect(vm.canScrollLeft).toBe(true);
    expect(vm.canScrollRight).toBe(false);
  });

  it('scrollTable calls scrollBy on the container', () => {
    const wrapper = mountTable();
    const scrollEl = wrapper.find('.article-table-scroll').element as HTMLElement;
    simulateOverflow(scrollEl, 0, 100, 400);

    const vm = wrapper.vm as unknown as { scrollTable: (d: 'left' | 'right') => void };
    vm.scrollTable('right');
    expect(scrollEl.scrollBy).toHaveBeenCalledWith(expect.objectContaining({ left: 200 }));
    vm.scrollTable('left');
    expect(scrollEl.scrollBy).toHaveBeenCalledWith(expect.objectContaining({ left: -200 }));
  });
});

describe('article-table.vue - title cell tooltip', () => {
  beforeEach(() => {
    class FakeResizeObserver {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    }
    globalThis.ResizeObserver = FakeResizeObserver as unknown as typeof ResizeObserver;
  });

  it('binds the full article title to the title cell :title attribute for hover tooltip', () => {
    const wrapper = mount(ArticleTable, {
      props: {
        articles: [makeArticle('a1', 1)],
        selectedId: null,
        sortColumn: null,
        sortDirection: 'asc' as const,
        selectedIds: new Set<string>(),
        allSelected: false,
        someSelected: false,
      },
      attachTo: document.body,
    });
    // The truncated title <p> carries the full title in its `title`
    // attribute so the native browser tooltip reveals the full text on
    // hover when the cell clips it. Locate it via the title cell's
    // distinctive `max-w-xs` class + the truncate <p> inside.
    const titleCell = wrapper.find('td.max-w-xs p.truncate');
    expect(titleCell.exists()).toBe(true);
    expect(titleCell.attributes('title')).toBe('Article a1');
  });
});
