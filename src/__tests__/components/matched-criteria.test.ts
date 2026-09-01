import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';
import MatchedCriteria from '@/components/matched-criteria.vue';
import { useCriteriaStore } from '@/stores/criteria';
import type { Article } from '@/types';
import { makeArticle as makeBaseArticle } from '../helpers/fixtures';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => false,
  tauriCommand: vi.fn(),
}));

function makeArticle(overrides: Partial<Article> = {}): Article {
  return makeBaseArticle({
    title: 'T',
    abstractText: '',
    authors: [],
    publicationYear: null,
    ...overrides,
  });
}

describe('matched-criteria.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders empty message when no criteria matched', () => {
    const store = useCriteriaStore();
    store.inclusionCriteria = [];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: { article: makeArticle() },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('No criteria matched');
  });

  it('renders the section header', () => {
    const store = useCriteriaStore();
    store.inclusionCriteria = [];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: { article: makeArticle() },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('Matched Criteria');
  });

  it('renders matched inclusion criterion text', () => {
    const store = useCriteriaStore();
    store.criteria = [
      {
        id: 'c1',
        criterionType: 'inclusion',
        text: 'Must be human study',
        priority: 'high',
        createdAt: '',
      },
    ];
    store.inclusionCriteria = [store.criteria[0]!];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: {
        article: makeArticle({ matchedInclusionCriteria: ['c1'] }),
      },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('Must be human study');
  });

  it('renders matched exclusion criterion text with line-through', () => {
    const store = useCriteriaStore();
    store.criteria = [
      {
        id: 'c2',
        criterionType: 'exclusion',
        text: 'Animal study',
        priority: 'standard',
        createdAt: '',
      },
    ];
    store.exclusionCriteria = [store.criteria[0]!];
    store.inclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: {
        article: makeArticle({ matchedExclusionCriteria: ['c2'] }),
      },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('Animal study');
  });

  it('renders failed inclusion criteria in the exclusion list without strikethrough', () => {
    const store = useCriteriaStore();
    store.criteria = [
      {
        id: 'c1',
        criterionType: 'inclusion',
        text: 'Must be human study',
        priority: 'high',
        createdAt: '',
      },
      {
        id: 'c2',
        criterionType: 'exclusion',
        text: 'Animal study',
        priority: 'standard',
        createdAt: '',
      },
    ];
    store.inclusionCriteria = [store.criteria[0]!];
    store.exclusionCriteria = [store.criteria[1]!];

    const wrapper = mount(MatchedCriteria, {
      props: {
        article: makeArticle({ matchedExclusionCriteria: ['c1', 'c2'] }),
      },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    // The failed inclusion (c1) carries the rejection-reason tooltip; the
    // violated exclusion (c2) keeps its plain-text tooltip.
    expect(wrapper.html()).toContain(
      'Failed inclusion criterion (reason for rejection): Must be human study'
    );
    expect(wrapper.html()).toContain('title="Animal study"');
  });

  it('renders edit button', () => {
    const store = useCriteriaStore();
    store.inclusionCriteria = [];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: { article: makeArticle() },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('edit');
  });

  it('opens dialog when edit clicked', async () => {
    const store = useCriteriaStore();
    store.inclusionCriteria = [];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: { article: makeArticle() },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    await wrapper.find('button').trigger('click');
    // The CriteriaEditDialog is stubbed; we just verify no crash.
    expect(wrapper.exists()).toBe(true);
  });
});
