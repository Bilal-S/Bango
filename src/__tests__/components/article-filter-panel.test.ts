import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';
import ArticleFilterPanel from '@/components/article-filter-panel.vue';
import { makeTagsStore, makeLabelsStore } from '../helpers/fixtures';
import type { ArticleFilter } from '@/composables/use-article-search';
import type { Criterion, SuggestOption } from '@/types';

// ── Mock stores ─────────────────────────────────────────────────────
const mockTagsStore = makeTagsStore({
  tags: [
    { id: 't1', name: 'machine-learning', source: 'user_created', color: null, articleCount: 5 },
    { id: 't2', name: 'nlp', source: 'ai_suggested', color: null, articleCount: 3 },
  ],
});

const mockLabelsStore = makeLabelsStore({
  labels: [
    { id: 'l1', name: 'priority-read', source: 'user_created', color: null, articleCount: 2 },
    { id: 'l2', name: 'disputed', source: 'ai_generated', color: null, articleCount: 1 },
  ],
});

const mockCriteria = [
  {
    id: 'c1',
    criterionType: 'inclusion',
    text: 'The Studies should not include animal subjects',
    priority: 'critical',
    createdAt: '2024-01-01T00:00:00Z',
  },
  {
    id: 'c2',
    criterionType: 'inclusion',
    text: 'Published after 2010',
    priority: 'standard',
    createdAt: '2024-01-01T00:00:00Z',
  },
  {
    id: 'c3',
    criterionType: 'exclusion',
    text: 'Not a human study',
    priority: 'low',
    createdAt: '2024-01-01T00:00:00Z',
  },
] as Criterion[];

const mockCriteriaStore = {
  criteria: mockCriteria,
  inclusionCriteria: mockCriteria.filter((c) => c.criterionType === 'inclusion'),
  exclusionCriteria: mockCriteria.filter((c) => c.criterionType === 'exclusion'),
  criterionIndexMap: new Map([
    ['c1', 1],
    ['c2', 2],
    ['c3', 3],
  ]),
};

vi.mock('@/stores/tags', () => ({
  useTagsStore: vi.fn(() => mockTagsStore),
}));

vi.mock('@/stores/labels', () => ({
  useLabelsStore: vi.fn(() => mockLabelsStore),
}));

vi.mock('@/stores/criteria', () => ({
  useCriteriaStore: vi.fn(() => mockCriteriaStore),
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
    doiText: '',
    doiEmpty: false,
    tags: [],
    labels: [],
    excludedTags: [],
    excludedLabels: [],
    criteria: [],
    criteriaUnknown: false,
    criteriaEmpty: false,
    exclusionCriteriaEmpty: false,
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

/**
 * Find a stubbed SuggestInput by its placeholder. The panel renders three
 * comboboxes (criteria in the metadata grid, then tags and labels in the
 * 2-column grid below); DOM-order indexing is fragile, so target by placeholder.
 * The `?? all[0]!` fallback keeps the inferred element type (the expect above
 * already failed the test when nothing matched).
 */
function findSuggestByPlaceholder(wrapper: ReturnType<typeof mountPanel>, placeholder: string) {
  const all = wrapper.findAllComponents({ name: 'SuggestInput' });
  const found = all.find((c) => c.props('placeholder') === placeholder);
  expect(found, `expected a SuggestInput with placeholder "${placeholder}"`).toBeTruthy();
  return found ?? all[0]!;
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
    // The SuggestInput is stubbed; find the tags one and emit a select event.
    const suggest = findSuggestByPlaceholder(wrapper, 'Search tags to add...');
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
    const suggest = findSuggestByPlaceholder(wrapper, 'Search tags to add...');
    const tagSuggestions = suggest.props('suggestions') as string[];
    expect(tagSuggestions).not.toContain('machine-learning');
    expect(tagSuggestions).toContain('nlp');
  });

  it('hides a tag from the combobox suggestions when it is already excluded', () => {
    const filter = makeFilter({ excludedTags: ['machine-learning'] });
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search tags to add...');
    const tagSuggestions = suggest.props('suggestions') as string[];
    expect(tagSuggestions).not.toContain('machine-learning');
  });

  // ── Match Criteria filter ──────────────────────────────────────────
  it('renders the Match Criteria heading with the sentinels at the END of the combobox list', () => {
    const wrapper = mountPanel();
    expect(wrapper.text()).toContain('Match Criteria');
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    const options = suggest.props('options') as SuggestOption[];
    // Criteria first (with number badges), then X, Y, Z - always last.
    expect(options.map((o) => o.id)).toEqual([
      'c1',
      'c2',
      'c3',
      '__x_exclusion_empty__',
      '__y_unknown__',
      '__z_empty__',
    ]);
    expect(options[3]).toMatchObject({
      id: '__x_exclusion_empty__',
      label: 'X. No Exclusion Criteria',
    });
    expect(options[4]).toMatchObject({ id: '__y_unknown__', label: 'Y. Unknown Criteria' });
    expect(options[5]).toMatchObject({ id: '__z_empty__', label: 'Z. No Criteria' });
  });

  it('renders an active criterion pill with the number badge and 20-char truncated text', () => {
    const filter = makeFilter({ criteria: ['c1'] });
    const wrapper = mountPanel(filter);
    const pill = wrapper.find('.afp-crit-pill');
    expect(pill.exists()).toBe(true);
    expect(pill.text()).toContain('1');
    expect(pill.text()).toContain('The Studies should n...');
    // The untruncated tail must not render; the full text lives in the tooltip.
    expect(pill.text()).not.toContain('animal subjects');
    expect(pill.attributes('title')).toBe('The Studies should not include animal subjects');
  });

  it('leaves short criterion text untruncated', () => {
    const filter = makeFilter({ criteria: ['c2'] });
    const wrapper = mountPanel(filter);
    const pill = wrapper.find('.afp-crit-pill');
    expect(pill.text()).toContain('Published after 2010');
    expect(pill.text()).not.toContain('...');
  });

  it('tints inclusion pills emerald and exclusion pills rose', () => {
    const filter = makeFilter({ criteria: ['c1', 'c3'] });
    const wrapper = mountPanel(filter);
    const pills = wrapper.findAll('.afp-crit-pill');
    expect(pills).toHaveLength(2);
    expect(pills[0]!.classes()).toContain('bg-emerald-50');
    expect(pills[1]!.classes()).toContain('bg-rose-50');
  });

  it('adds a criterion when selected from the combobox (option id carries the UUID)', async () => {
    const filter = makeFilter();
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    await suggest.vm.$emit('select', 'Not a human study', {
      id: 'c3',
      label: 'Not a human study',
      badge: '3',
    });
    applyEmittedUpdates(wrapper, filter);
    expect(filter.criteria).toEqual(['c3']);
  });

  it('hides already-active criteria from the combobox options and numbers them via badges', () => {
    const filter = makeFilter({ criteria: ['c1'] });
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    const options = suggest.props('options') as SuggestOption[];
    expect(options.map((o) => o.id)).toEqual([
      'c2',
      'c3',
      '__x_exclusion_empty__',
      '__y_unknown__',
      '__z_empty__',
    ]);
    expect(options[0]).toMatchObject({ label: 'Published after 2010', badge: '2' });
  });

  it('removes a criterion pill when the x button is clicked', async () => {
    const filter = makeFilter({ criteria: ['c1', 'c2'] });
    const wrapper = mountPanel(filter);
    await wrapper.find('.afp-crit-pill button').trigger('click');
    applyEmittedUpdates(wrapper, filter);
    expect(filter.criteria).toEqual(['c2']);
  });

  it('selecting X from the combobox emits update:filter(exclusionCriteriaEmpty, true)', async () => {
    const wrapper = mountPanel();
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    await suggest.vm.$emit('select', 'X. No Exclusion Criteria', {
      id: '__x_exclusion_empty__',
      label: 'X. No Exclusion Criteria',
    });
    expect(wrapper.emitted('update:filter')).toContainEqual(['exclusionCriteriaEmpty', true]);
  });

  it('selecting X while Z is active clears Z (mutually exclusive)', async () => {
    const filter = makeFilter({ criteriaEmpty: true });
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    await suggest.vm.$emit('select', 'X. No Exclusion Criteria', {
      id: '__x_exclusion_empty__',
      label: 'X. No Exclusion Criteria',
    });
    applyEmittedUpdates(wrapper, filter);
    expect(filter.exclusionCriteriaEmpty).toBe(true);
    expect(filter.criteriaEmpty).toBe(false);
  });

  it('X combines with a specific criterion (AND, no mutual clearing)', async () => {
    const filter = makeFilter({ exclusionCriteriaEmpty: true });
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    await suggest.vm.$emit('select', 'Published after 2010', {
      id: 'c2',
      label: 'Published after 2010',
      badge: '2',
    });
    applyEmittedUpdates(wrapper, filter);
    expect(filter.criteria).toEqual(['c2']);
    expect(filter.exclusionCriteriaEmpty).toBe(true);
  });

  it('selecting Y from the combobox emits update:filter(criteriaUnknown, true)', async () => {
    const wrapper = mountPanel();
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    await suggest.vm.$emit('select', 'Y. Unknown Criteria', {
      id: '__y_unknown__',
      label: 'Y. Unknown Criteria',
    });
    expect(wrapper.emitted('update:filter')).toContainEqual(['criteriaUnknown', true]);
  });

  it('selecting Y while Z is active clears Z (mutually exclusive)', async () => {
    const filter = makeFilter({ criteriaEmpty: true });
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    await suggest.vm.$emit('select', 'Y. Unknown Criteria', {
      id: '__y_unknown__',
      label: 'Y. Unknown Criteria',
    });
    applyEmittedUpdates(wrapper, filter);
    expect(filter.criteriaUnknown).toBe(true);
    expect(filter.criteriaEmpty).toBe(false);
  });

  it('selecting Z from the combobox clears specific criteria, Y, and X', async () => {
    const filter = makeFilter({
      criteria: ['c1'],
      criteriaUnknown: true,
      exclusionCriteriaEmpty: true,
    });
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    await suggest.vm.$emit('select', 'Z. No Criteria', {
      id: '__z_empty__',
      label: 'Z. No Criteria',
    });
    applyEmittedUpdates(wrapper, filter);
    expect(filter.criteriaEmpty).toBe(true);
    expect(filter.criteria).toEqual([]);
    expect(filter.criteriaUnknown).toBe(false);
    expect(filter.exclusionCriteriaEmpty).toBe(false);
  });

  it('adding the first criterion clears the Z sentinel', async () => {
    const filter = makeFilter({ criteriaEmpty: true });
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    await suggest.vm.$emit('select', 'Published after 2010', {
      id: 'c2',
      label: 'Published after 2010',
      badge: '2',
    });
    applyEmittedUpdates(wrapper, filter);
    expect(filter.criteria).toEqual(['c2']);
    expect(filter.criteriaEmpty).toBe(false);
  });

  it('renders X/Y/Z sentinel pills with an x only while their flag is on', () => {
    // Inactive: no sentinel pills in the DOM (they live in the dropdown list).
    const inactive = mountPanel();
    expect(inactive.find('.afp-sentinel--x').exists()).toBe(false);
    expect(inactive.find('.afp-sentinel--y').exists()).toBe(false);
    expect(inactive.find('.afp-sentinel--z').exists()).toBe(false);

    // Active: removable pills, dashed-styled to mark them as sentinels.
    const wrapper = mountPanel(
      makeFilter({ criteria: ['c1'], criteriaUnknown: true, criteriaEmpty: true })
    );
    const y = wrapper.find('.afp-sentinel--y');
    const z = wrapper.find('.afp-sentinel--z');
    expect(y.text()).toContain('Y. Unknown Criteria');
    expect(z.text()).toContain('Z. No Criteria');
    expect(y.find('button').exists()).toBe(true);
    expect(z.find('button').exists()).toBe(true);
    expect(y.classes()).toContain('border-dashed');
    expect(wrapper.find('.afp-crit-pill').classes()).not.toContain('border-dashed');

    // X renders independently of the other two (Z clears it, but a fresh mount
    // with only X on must show its pill).
    const xWrapper = mountPanel(makeFilter({ exclusionCriteriaEmpty: true }));
    const x = xWrapper.find('.afp-sentinel--x');
    expect(x.exists()).toBe(true);
    expect(x.text()).toContain('X. No Exclusion Criteria');
    expect(x.find('button').exists()).toBe(true);
    expect(x.classes()).toContain('border-dashed');
  });

  it('removes the X sentinel pill when its x is clicked', async () => {
    const filter = makeFilter({ exclusionCriteriaEmpty: true });
    const wrapper = mountPanel(filter);
    await wrapper.find('.afp-sentinel--x button').trigger('click');
    expect(wrapper.emitted('update:filter')).toContainEqual(['exclusionCriteriaEmpty', false]);
  });

  it('removes the Y sentinel pill when its x is clicked', async () => {
    const filter = makeFilter({ criteriaUnknown: true });
    const wrapper = mountPanel(filter);
    await wrapper.find('.afp-sentinel--y button').trigger('click');
    expect(wrapper.emitted('update:filter')).toContainEqual(['criteriaUnknown', false]);
  });

  it('removes the Z sentinel pill when its x is clicked', async () => {
    const filter = makeFilter({ criteriaEmpty: true });
    const wrapper = mountPanel(filter);
    await wrapper.find('.afp-sentinel--z button').trigger('click');
    expect(wrapper.emitted('update:filter')).toContainEqual(['criteriaEmpty', false]);
  });

  it('hides active sentinels from the combobox options', () => {
    const filter = makeFilter({ criteriaUnknown: true, exclusionCriteriaEmpty: true });
    const wrapper = mountPanel(filter);
    const suggest = findSuggestByPlaceholder(wrapper, 'Search criteria to add...');
    const options = suggest.props('options') as SuggestOption[];
    expect(options.map((o) => o.id)).toEqual(['c1', 'c2', 'c3', '__z_empty__']);
  });

  // ── Clear / Apply events ───────────────────────────────────────────
  it('emits "clear" when the Clear Filter button is clicked', async () => {
    const wrapper = mountPanel();
    const clearBtn = wrapper.find('.afp-clear-btn');
    expect(clearBtn.exists()).toBe(true);
    await clearBtn.trigger('click');
    expect(wrapper.emitted('clear')).toBeTruthy();
  });

  it('emits "apply" when Apply Filters is clicked', async () => {
    const wrapper = mountPanel();
    const applyBtn = wrapper.find('.afp-apply-btn');
    expect(applyBtn.exists()).toBe(true);
    await applyBtn.trigger('click');
    expect(wrapper.emitted('apply')).toBeTruthy();
  });

  it('emits "close" when the close button is clicked', async () => {
    const wrapper = mountPanel();
    const closeBtn = wrapper.find('.afp-close-btn');
    expect(closeBtn.exists()).toBe(true);
    await closeBtn.trigger('click');
    expect(wrapper.emitted('close')).toBeTruthy();
  });

  // ── Enter-to-apply (guarded by year validity) ─────────────────────
  it('emits "apply" when Enter is pressed in the Title input (valid year range)', async () => {
    const wrapper = mountPanel();
    const titleInput = wrapper.find('input[placeholder="Filter by title..."]');
    expect(titleInput.exists()).toBe(true);
    await titleInput.trigger('keyup', { key: 'Enter' });
    expect(wrapper.emitted('apply')).toBeTruthy();
  });

  it('emits "apply" when Enter is pressed in the Journal input', async () => {
    const wrapper = mountPanel();
    const journalInput = wrapper.find('input[placeholder="Filter by journal..."]');
    expect(journalInput.exists()).toBe(true);
    await journalInput.trigger('keyup', { key: 'Enter' });
    expect(wrapper.emitted('apply')).toBeTruthy();
  });

  it('does NOT emit "apply" when Enter is pressed and the year range is invalid', async () => {
    const filter = makeFilter({ yearFrom: 2024, yearTo: 2020 });
    const wrapper = mountPanel(filter);
    const titleInput = wrapper.find('input[placeholder="Filter by title..."]');
    await titleInput.trigger('keyup', { key: 'Enter' });
    expect(wrapper.emitted('apply')).toBeFalsy();
  });

  // ── Year-range validation (per-field + symmetric bounds + messaging) ──
  describe('year-range validation', () => {
    it('is valid when From = To (boundary equality is allowed)', () => {
      const filter = makeFilter({ yearFrom: 2020, yearTo: 2020 });
      const wrapper = mountPanel(filter);
      // Apply Filters button is enabled (not disabled) when valid.
      const applyBtn = wrapper.find('.afp-apply-btn');
      expect(applyBtn.attributes('disabled')).toBeFalsy();
      // No red hint rendered.
      expect(wrapper.find('p.text-red-500').exists()).toBe(false);
    });

    it('is valid when From < To', () => {
      const filter = makeFilter({ yearFrom: 2018, yearTo: 2024 });
      const wrapper = mountPanel(filter);
      expect(wrapper.find('.afp-apply-btn').attributes('disabled')).toBeFalsy();
      expect(wrapper.find('p.text-red-500').exists()).toBe(false);
    });

    it('is invalid and shows the "From <= To" hint when From > To', () => {
      const filter = makeFilter({ yearFrom: 2024, yearTo: 2020 });
      const wrapper = mountPanel(filter);
      // `disabled` is a boolean attribute - present means truthy. Use
      // `toBeDefined()` (not `toBeTruthy()`) because Vue renders it as `""`.
      expect(wrapper.find('.afp-apply-btn').attributes('disabled')).toBeDefined();
      const hint = wrapper.find('p.text-red-500');
      expect(hint.exists()).toBe(true);
      expect(hint.text()).toContain('From year must be less than or equal to To year.');
    });

    it('flags ONLY the From field red when From is out-of-range (not To)', () => {
      // From too high: previously a bug (the old check only gated From < 1850,
      // so From=3000 slipped through). Now the symmetric bounds catch it.
      const filter = makeFilter({ yearFrom: 3000, yearTo: null });
      const wrapper = mountPanel(filter);
      // Find the From and To ClearableInput components by placeholder.
      const inputs = wrapper.findAllComponents({ name: 'ClearableInput' });
      const fromInput = inputs.find((c) => c.props('placeholder') === 'From');
      const toInput = inputs.find((c) => c.props('placeholder') === 'To');
      expect(fromInput?.props('inputClass') ?? '').toContain('border-red-300');
      // To is null, so To is NOT individually invalid.
      expect(toInput?.props('inputClass') ?? '').toContain('border-slate-200');
      expect(toInput?.props('inputClass') ?? '').not.toContain('border-red-300');
      const hint = wrapper.find('p.text-red-500');
      expect(hint.text()).toContain('From year must be between 1850-2100.');
    });

    it('flags ONLY the To field red when To is out-of-range low (not From)', () => {
      // To too low: previously a bug (the old check only gated To > 2100, so
      // To=1000 slipped through). Now the symmetric bounds catch it.
      const filter = makeFilter({ yearFrom: null, yearTo: 1000 });
      const wrapper = mountPanel(filter);
      const inputs = wrapper.findAllComponents({ name: 'ClearableInput' });
      const fromInput = inputs.find((c) => c.props('placeholder') === 'From');
      const toInput = inputs.find((c) => c.props('placeholder') === 'To');
      expect(toInput?.props('inputClass') ?? '').toContain('border-red-300');
      expect(fromInput?.props('inputClass') ?? '').toContain('border-slate-200');
      expect(fromInput?.props('inputClass') ?? '').not.toContain('border-red-300');
      const hint = wrapper.find('p.text-red-500');
      expect(hint.text()).toContain('To year must be between 1850-2100.');
    });

    it('shows the "Both years" hint when both are individually out of range', () => {
      const filter = makeFilter({ yearFrom: 1000, yearTo: 9999 });
      const wrapper = mountPanel(filter);
      expect(wrapper.find('.afp-apply-btn').attributes('disabled')).toBeDefined();
      const hint = wrapper.find('p.text-red-500');
      expect(hint.text()).toContain('Both years must be between 1850-2100.');
    });

    it('flags BOTH fields red when From > To (the shared range-flip rule)', () => {
      const filter = makeFilter({ yearFrom: 2024, yearTo: 2020 });
      const wrapper = mountPanel(filter);
      const inputs = wrapper.findAllComponents({ name: 'ClearableInput' });
      const fromInput = inputs.find((c) => c.props('placeholder') === 'From');
      const toInput = inputs.find((c) => c.props('placeholder') === 'To');
      expect(fromInput?.props('inputClass') ?? '').toContain('border-red-300');
      expect(toInput?.props('inputClass') ?? '').toContain('border-red-300');
    });

    it('is valid when only From is set (To undefined)', () => {
      const filter = makeFilter({ yearFrom: 2020, yearTo: null });
      const wrapper = mountPanel(filter);
      expect(wrapper.find('.afp-apply-btn').attributes('disabled')).toBeFalsy();
      expect(wrapper.find('p.text-red-500').exists()).toBe(false);
    });

    it('is valid when only To is set (From undefined)', () => {
      const filter = makeFilter({ yearFrom: null, yearTo: 2024 });
      const wrapper = mountPanel(filter);
      expect(wrapper.find('.afp-apply-btn').attributes('disabled')).toBeFalsy();
      expect(wrapper.find('p.text-red-500').exists()).toBe(false);
    });

    it('is valid when neither year is set', () => {
      const filter = makeFilter();
      const wrapper = mountPanel(filter);
      expect(wrapper.find('.afp-apply-btn').attributes('disabled')).toBeFalsy();
      expect(wrapper.find('p.text-red-500').exists()).toBe(false);
    });

    it('accepts boundary years 1850 and 2100 (inclusive)', () => {
      const filter = makeFilter({ yearFrom: 1850, yearTo: 2100 });
      const wrapper = mountPanel(filter);
      expect(wrapper.find('.afp-apply-btn').attributes('disabled')).toBeFalsy();
      expect(wrapper.find('p.text-red-500').exists()).toBe(false);
    });
  });

  // ── Action-row composition ─────────────────────────────────────────
  it('renders the filter_alt_off icon inside the Clear Filter button', () => {
    const wrapper = mountPanel();
    const clearBtn = wrapper.find('.afp-clear-btn');
    const icon = clearBtn.find('.material-symbols-outlined');
    expect(icon.exists()).toBe(true);
    expect(icon.text()).toBe('filter_alt_off');
  });

  it('renders the filter_alt icon inside the Apply Filters button', () => {
    const wrapper = mountPanel();
    const applyBtn = wrapper.find('.afp-apply-btn');
    const icon = applyBtn.find('.material-symbols-outlined');
    expect(icon.exists()).toBe(true);
    expect(icon.text()).toBe('filter_alt');
  });

  it('uses a Material Symbols close icon for the close button (not a bare × glyph)', () => {
    const wrapper = mountPanel();
    const closeBtn = wrapper.find('.afp-close-btn');
    const icon = closeBtn.find('.material-symbols-outlined');
    expect(icon.exists()).toBe(true);
    expect(icon.text()).toBe('close');
  });

  // ── Result-count notice (centered between Clear Filter and Apply) ──
  it('renders the "Filter active: n article(s) found." notice when isFiltered + resultCount are set', () => {
    const wrapper = mountPanel(makeFilter(), { isFiltered: true, resultCount: 3 });
    const notice = wrapper.find('.afp-result-count');
    expect(notice.exists()).toBe(true);
    expect(notice.text()).toContain('Filter active: 3 article(s) found.');
  });

  it('uses the singular form for exactly one result', () => {
    const wrapper = mountPanel(makeFilter(), { isFiltered: true, resultCount: 1 });
    const notice = wrapper.find('.afp-result-count');
    expect(notice.exists()).toBe(true);
    expect(notice.text()).toContain('Filter active: 1 article found.');
    expect(notice.text()).not.toContain('article(s)');
  });

  it('hides the result-count notice when no filter is active', () => {
    const wrapper = mountPanel(makeFilter(), { isFiltered: false, resultCount: 10 });
    expect(wrapper.find('.afp-result-count').exists()).toBe(false);
  });

  it('hides the result-count notice before the first apply (resultCount undefined)', () => {
    const wrapper = mountPanel();
    expect(wrapper.find('.afp-result-count').exists()).toBe(false);
  });

  // ── Per-field clear ("x") via ClearableInput ───────────────────────
  it('emits update:filter(titleText, "") + apply when the Title field "x" is clicked', async () => {
    const filter = makeFilter({ titleText: 'some title' });
    const wrapper = mountPanel(filter);
    // The Title field's ClearableInput renders the clear button.
    const titleClearable = wrapper
      .findAllComponents({ name: 'ClearableInput' })
      .find((c) => c.props('modelValue') === 'some title');
    expect(titleClearable).toBeTruthy();
    await titleClearable!.vm.$emit('clear');
    const events = wrapper.emitted('update:filter') ?? [];
    const titleEvents = events.filter((e) => (e as [string, unknown])[0] === 'titleText');
    expect(titleEvents.length).toBeGreaterThan(0);
    expect((titleEvents[titleEvents.length - 1] as [string, unknown])[1]).toBe('');
    expect(wrapper.emitted('apply')).toBeTruthy();
  });

  it('emits update:filter(yearFrom, null) when the Year From "x" is clicked (not "")', async () => {
    const filter = makeFilter({ yearFrom: 2020 });
    const wrapper = mountPanel(filter);
    // Year From is a number-typed ClearableInput whose modelValue is '2020'.
    const yearFromClearable = wrapper
      .findAllComponents({ name: 'ClearableInput' })
      .find((c) => c.props('modelValue') === '2020');
    expect(yearFromClearable).toBeTruthy();
    await yearFromClearable!.vm.$emit('clear');
    const events = wrapper.emitted('update:filter') ?? [];
    const yearEvents = events.filter((e) => (e as [string, unknown])[0] === 'yearFrom');
    expect(yearEvents.length).toBeGreaterThan(0);
    expect((yearEvents[yearEvents.length - 1] as [string, unknown])[1]).toBeNull();
  });

  it('renders the DOI clear "x" but hides it when doiEmpty is checked (input disabled)', async () => {
    // When doiEmpty is true the DOI ClearableInput is disabled, so its "x" is
    // hidden (the component's `v-if="modelValue && !disabled"` gate).
    const filter = makeFilter({ doiText: '10.1000/abc', doiEmpty: true });
    const wrapper = mountPanel(filter);
    const doiClearable = wrapper
      .findAllComponents({ name: 'ClearableInput' })
      .find((c) => c.props('placeholder') === 'Filter by DOI...');
    expect(doiClearable).toBeTruthy();
    expect(doiClearable!.props('disabled')).toBe(true);
    // No clear button rendered while disabled.
    expect(doiClearable!.find('.clearable-input__clear').exists()).toBe(false);
  });
});
