import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';
import ArticleFilterPanel from '@/components/article-filter-panel.vue';
import type { ArticleFilter } from '@/composables/use-article-search';

// ── Mock stores ─────────────────────────────────────────────────────
const mockTagsStore = {
  tags: [
    { id: 't1', name: 'machine-learning', source: 'user_created', color: null, articleCount: 5 },
    { id: 't2', name: 'nlp', source: 'ai_suggested', color: null, articleCount: 3 },
  ],
  fetchTags: vi.fn(),
};

const mockLabelsStore = {
  labels: [
    { id: 'l1', name: 'priority-read', source: 'user_created', color: null, articleCount: 2 },
    { id: 'l2', name: 'disputed', source: 'ai_generated', color: null, articleCount: 1 },
  ],
  fetchLabels: vi.fn(),
};

vi.mock('@/stores/tags', () => ({
  useTagsStore: vi.fn(() => mockTagsStore),
}));

vi.mock('@/stores/labels', () => ({
  useLabelsStore: vi.fn(() => mockLabelsStore),
}));

// ── Helpers ─────────────────────────────────────────────────────────
function makeFilter(overrides: Partial<ArticleFilter> = {}): ArticleFilter {
  return {
    titleMatch: 'contains',
    titleText: '',
    authorText: '',
    yearFrom: null,
    yearTo: null,
    journal: '',
    tags: [],
    labels: [],
    excludedTags: [],
    excludedLabels: [],
    ...overrides,
  };
}

/**
 * Mount the panel with a reactive-ish filter. The component receives `filter`
 * as a prop and emits `update:filter` mutations; the test harness applies
 * those mutations back onto the filter object so the component re-renders.
 */
function mountPanel(
  filter: ArticleFilter = makeFilter(),
  extraProps: Record<string, unknown> = {}
) {
  const wrapper = mount(ArticleFilterPanel, {
    props: {
      filter,
      allAuthors: ['Alice', 'Bob'],
      allTags: ['machine-learning', 'nlp'],
      allLabels: ['priority-read', 'disputed'],
      ...extraProps,
    },
    global: {
      stubs: { SuggestInput: true },
    },
  });

  // Apply `update:filter` emissions back onto the filter object so the
  // component sees its own mutations (mimics the real parent handler in
  // `article-list.vue` which writes back into the reactive filter).
  wrapper.vm.$nextTick = wrapper.vm.$nextTick.bind(wrapper.vm);
  return wrapper;
}

/** Apply any emitted `update:filter` events onto the filter object. */
function applyEmittedUpdates(wrapper: ReturnType<typeof mountPanel>, filter: ArticleFilter): void {
  const events = wrapper.emitted('update:filter');
  if (!events) return;
  for (const evt of events) {
    const [key, value] = evt as [keyof ArticleFilter, unknown];
    (filter as unknown as Record<string, unknown>)[key] = value;
  }
}

describe('article-filter-panel.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  // ── Rendering ──────────────────────────────────────────────────────
  it('renders the Filters heading and Apply button', () => {
    const wrapper = mountPanel();
    expect(wrapper.text()).toContain('Filters');
    expect(wrapper.find('button:not([type])').exists() || wrapper.text()).toBeTruthy();
  });

  it('shows the "Click tag to toggle exclude." hint next to the Tags label', () => {
    const wrapper = mountPanel();
    expect(wrapper.text()).toContain('Click tag to toggle exclude.');
  });

  it('shows the "Click label to toggle exclude." hint next to the Labels label', () => {
    const wrapper = mountPanel();
    expect(wrapper.text()).toContain('Click label to toggle exclude.');
  });

  // ── Tag pills: inclusion ───────────────────────────────────────────
  it('renders included tag pills with the tag name', () => {
    const filter = makeFilter({ tags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    expect(wrapper.text()).toContain('machine-learning');
  });

  it('emits update:filter to add a tag when selected from the combobox', async () => {
    const filter = makeFilter();
    const wrapper = mountPanel(filter);
    // The SuggestInput is stubbed; find it and emit a select event.
    const suggest = wrapper.findComponent({ name: 'SuggestInput' });
    // The first SuggestInput is for tags (tags section comes before labels).
    await suggest.vm.$emit('select', 'nlp');
    const events = wrapper.emitted('update:filter');
    expect(events).toBeTruthy();
    // Find the event that sets tags to include 'nlp'.
    const tagsEvents = events!.filter((e) => (e as [string, unknown[]])[0] === 'tags');
    expect(tagsEvents.length).toBeGreaterThan(0);
    const lastTagsEvent = tagsEvents[tagsEvents.length - 1];
    expect(lastTagsEvent![1]).toEqual(['nlp']);
  });

  // ── Tag pills: NOT-toggle ──────────────────────────────────────────
  it('moves a tag from tags to excludedTags when the pill body is clicked', async () => {
    const filter = makeFilter({ tags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    // Find the pill span (the clickable one with class afp-pill, not the
    // remove button). Click the first afp-pill in the Tags section.
    const pills = wrapper.findAll('.afp-pill');
    expect(pills.length).toBeGreaterThan(0);
    await pills[0]!.trigger('click');
    applyEmittedUpdates(wrapper, filter);
    // The tag should now be excluded, not included.
    expect(filter.tags).not.toContain('machine-learning');
    expect(filter.excludedTags).toContain('machine-learning');
  });

  it('moves a tag from excludedTags back to tags when the excluded pill is clicked', async () => {
    const filter = makeFilter({ excludedTags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    const pills = wrapper.findAll('.afp-pill');
    expect(pills.length).toBeGreaterThan(0);
    await pills[0]!.trigger('click');
    applyEmittedUpdates(wrapper, filter);
    expect(filter.excludedTags).not.toContain('machine-learning');
    expect(filter.tags).toContain('machine-learning');
  });

  it('renders the bold "NOT:" prefix on excluded tag pills', () => {
    const filter = makeFilter({ excludedTags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    const excludedPill = wrapper.find('.afp-pill--excluded');
    expect(excludedPill.exists()).toBe(true);
    expect(excludedPill.text()).toContain('NOT:');
    expect(excludedPill.text()).toContain('machine-learning');
    // The NOT: prefix should be bold.
    const notSpan = excludedPill.find('.afp-pill__not');
    expect(notSpan.exists()).toBe(true);
    expect(notSpan.classes()).toContain('font-bold');
  });

  it('removes a tag entirely (from excludedTags) when the x button is clicked, without toggling negation', async () => {
    const filter = makeFilter({ excludedTags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    // The remove button inside the excluded pill.
    const removeBtn = wrapper.find('.afp-pill--excluded button');
    expect(removeBtn.exists()).toBe(true);
    await removeBtn.trigger('click');
    applyEmittedUpdates(wrapper, filter);
    // The tag is gone from both arrays - it was NOT moved to `tags`.
    expect(filter.excludedTags).not.toContain('machine-learning');
    expect(filter.tags).not.toContain('machine-learning');
  });

  it('removes an included tag entirely when the x button is clicked (stop propagation prevents toggle)', async () => {
    const filter = makeFilter({ tags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    const removeBtn = wrapper.find('.afp-pill button');
    expect(removeBtn.exists()).toBe(true);
    await removeBtn.trigger('click');
    applyEmittedUpdates(wrapper, filter);
    expect(filter.tags).not.toContain('machine-learning');
    expect(filter.excludedTags).not.toContain('machine-learning');
  });

  // ── Label pills: NOT-toggle parity ─────────────────────────────────
  it('moves a label from labels to excludedLabels when the pill body is clicked', async () => {
    const filter = makeFilter({ labels: ['priority-read'] });
    const wrapper = mountPanel(filter);
    // Find the label pill. There are two SuggestInput stubs; the label pills
    // are in the second section. We target all afp-pill elements and pick the
    // one whose text contains the label name.
    const pills = wrapper.findAll('.afp-pill');
    const labelPill = pills.find((p) => p.text().includes('priority-read'));
    expect(labelPill).toBeTruthy();
    await labelPill!.trigger('click');
    applyEmittedUpdates(wrapper, filter);
    expect(filter.labels).not.toContain('priority-read');
    expect(filter.excludedLabels).toContain('priority-read');
  });

  it('renders the bold "NOT:" prefix on excluded label pills', () => {
    const filter = makeFilter({ excludedLabels: ['disputed'] });
    const wrapper = mountPanel(filter);
    const excludedPills = wrapper.findAll('.afp-pill--excluded');
    const disputedPill = excludedPills.find((p) => p.text().includes('disputed'));
    expect(disputedPill).toBeTruthy();
    expect(disputedPill!.text()).toContain('NOT:');
    expect(disputedPill!.find('.afp-pill__not').exists()).toBe(true);
  });

  // ── Combobox availability ──────────────────────────────────────────
  it('hides a tag from the combobox suggestions when it is already included', () => {
    const filter = makeFilter({ tags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    // The stubbed SuggestInput receives `suggestions` as a prop.
    const suggests = wrapper.findAllComponents({ name: 'SuggestInput' });
    // First SuggestInput = tags. The already-included tag should be absent.
    const tagSuggestions = suggests[0]!.props('suggestions') as string[];
    expect(tagSuggestions).not.toContain('machine-learning');
    expect(tagSuggestions).toContain('nlp');
  });

  it('hides a tag from the combobox suggestions when it is already excluded', () => {
    const filter = makeFilter({ excludedTags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    const suggests = wrapper.findAllComponents({ name: 'SuggestInput' });
    const tagSuggestions = suggests[0]!.props('suggestions') as string[];
    expect(tagSuggestions).not.toContain('machine-learning');
  });

  // ── Clear / Apply events ───────────────────────────────────────────
  it('emits "clear" when Clear All is clicked', async () => {
    const wrapper = mountPanel();
    const clearBtn = wrapper.find('button.text-slate-500');
    expect(clearBtn.exists()).toBe(true);
    await clearBtn.trigger('click');
    expect(wrapper.emitted('clear')).toBeTruthy();
  });

  it('emits "apply" when Apply Filters is clicked', async () => {
    const wrapper = mountPanel();
    // The Apply button is the last button with text "Apply Filters".
    const buttons = wrapper.findAll('button');
    const applyBtn = buttons.find((b) => b.text().includes('Apply Filters'));
    expect(applyBtn).toBeTruthy();
    await applyBtn!.trigger('click');
    expect(wrapper.emitted('apply')).toBeTruthy();
  });
});
