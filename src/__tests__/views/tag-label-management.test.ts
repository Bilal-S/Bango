import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';

// ── Mock stores ────────────────────────────────────────────────────────────
// The view calls `fetchIfNeeded()` on mount and reads `tags`/`labels`/`loading`/
// `error`/`suggesting`. Mocking both stores keeps the test focused on the
// routing envelope (the D5 reset-filters contract) rather than store plumbing.

const mockTagsFetchIfNeeded = vi.fn().mockResolvedValue(undefined);
const mockLabelsFetchIfNeeded = vi.fn().mockResolvedValue(undefined);
const mockSuggestTags = vi.fn().mockResolvedValue(undefined);
const mockSuggestLabels = vi.fn().mockResolvedValue(undefined);

vi.mock('@/stores/tags', () => ({
  useTagsStore: () => ({
    tags: [],
    loading: false,
    error: null,
    suggesting: false,
    fetchIfNeeded: mockTagsFetchIfNeeded,
    suggestTags: mockSuggestTags,
    invalidate: vi.fn(),
  }),
}));

vi.mock('@/stores/labels', () => ({
  useLabelsStore: () => ({
    labels: [],
    loading: false,
    error: null,
    suggesting: false,
    fetchIfNeeded: mockLabelsFetchIfNeeded,
    suggestLabels: mockSuggestLabels,
    invalidate: vi.fn(),
  }),
}));

// Stub the child panel so we can emit the `filter` event directly. The
// interaction surface inside TagLabelPanel (hover-revealed affordances) is
// already covered by its own component tests; here we only need to assert that
// the view's `@filter` handler routes with the D5 envelope.
const TagLabelPanelStub = {
  name: 'TagLabelPanel',
  template: '<div class="tag-label-panel-stub" :data-kind="kind" />',
  props: ['kind', 'items', 'suggesting'],
  emits: ['filter', 'suggest', 'create', 'rename', 'delete', 'updateColor'],
};

import TagLabelManagement from '@/views/tag-label-management.vue';

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/articles', component: { template: '<div />' } },
    ],
  });
}

function mountView(router = makeRouter()) {
  setActivePinia(createPinia());
  return mount(TagLabelManagement, {
    global: {
      plugins: [createPinia(), router],
      stubs: { TagLabelPanel: TagLabelPanelStub },
    },
  });
}

/**
 * Find the (first) TagLabelPanel stub whose `kind` prop matches `kind`.
 * Returns a VueWrapper or throws so the test fails loudly if the panel is
 * missing. Uses `[0]`-style indexing because the project's tsconfig targets
 * `ES2021` (no `Array.prototype.at`).
 */
function findPanel(wrapper: ReturnType<typeof mount>, kind: 'tag' | 'label') {
  const panels = wrapper.findAllComponents({ name: 'TagLabelPanel' });
  const match = panels.find((p) => p.props('kind') === kind);
  if (!match) throw new Error(`TagLabelPanel kind="${kind}" not mounted`);
  return match;
}

describe('tag-label-management.vue - D5 reset-filters deep-link envelope', () => {
  beforeEach(() => {
    mockTagsFetchIfNeeded.mockReset().mockResolvedValue(undefined);
    mockLabelsFetchIfNeeded.mockReset().mockResolvedValue(undefined);
    mockSuggestTags.mockReset().mockResolvedValue(undefined);
    mockSuggestLabels.mockReset().mockResolvedValue(undefined);
  });

  it('clicking a tag filter routes to /articles with the D5 envelope', async () => {
    const router = makeRouter();
    const pushSpy = vi.spyOn(router, 'push');
    const wrapper = mountView(router);
    await flushPromises();

    await findPanel(wrapper, 'tag').vm.$emit('filter', 'tag-abc');
    await flushPromises();

    expect(pushSpy).toHaveBeenCalledTimes(1);
    const location = pushSpy.mock.calls[0]![0];
    expect(location).toMatchObject({
      path: '/articles',
      query: {
        tags: 'tag-abc',
        status: 'all',
        filterCollapsed: '1',
        resetFilters: '1',
      },
    });
  });

  it('clicking a label filter routes to /articles with the D5 envelope', async () => {
    const router = makeRouter();
    const pushSpy = vi.spyOn(router, 'push');
    const wrapper = mountView(router);
    await flushPromises();

    await findPanel(wrapper, 'label').vm.$emit('filter', 'label-xyz');
    await flushPromises();

    expect(pushSpy).toHaveBeenCalledTimes(1);
    const location = pushSpy.mock.calls[0]![0];
    expect(location).toMatchObject({
      path: '/articles',
      query: {
        labels: 'label-xyz',
        status: 'all',
        filterCollapsed: '1',
        resetFilters: '1',
      },
    });
  });

  // Regression guard: this is the exact bug that was fixed in `da30b7f`. If a
  // future refactor silently drops the `resetFilters` flag from the envelope,
  // the keep-alive-cached ArticleList will overlay the deep-link's tag/label
  // filter on top of whatever the prior session left in `filter.*`/`query.*`
  // (e.g. `tags="obesity" AND author="Bob"` when the user clicked a different
  // tag while an author filter was still active). Locking the full envelope
  // here ensures the bibliometric-style reset contract is honored.
  it('the tag envelope includes resetFilters=1 (regression guard for da30b7f)', async () => {
    const router = makeRouter();
    const pushSpy = vi.spyOn(router, 'push');
    const wrapper = mountView(router);
    await flushPromises();

    await findPanel(wrapper, 'tag').vm.$emit('filter', 't1');
    await flushPromises();

    const location = pushSpy.mock.calls[0]![0] as {
      query: Record<string, unknown>;
    };
    expect(location.query.resetFilters).toBe('1');
    expect(location.query.filterCollapsed).toBe('1');
    expect(location.query.status).toBe('all');
  });

  it('the label envelope includes resetFilters=1 (regression guard for da30b7f)', async () => {
    const router = makeRouter();
    const pushSpy = vi.spyOn(router, 'push');
    const wrapper = mountView(router);
    await flushPromises();

    await findPanel(wrapper, 'label').vm.$emit('filter', 'l1');
    await flushPromises();

    const location = pushSpy.mock.calls[0]![0] as {
      query: Record<string, unknown>;
    };
    expect(location.query.resetFilters).toBe('1');
    expect(location.query.filterCollapsed).toBe('1');
    expect(location.query.status).toBe('all');
  });
});
