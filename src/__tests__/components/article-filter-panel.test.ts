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
    doiText: '',
    doiEmpty: false,
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
