import { describe, it, expect, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';
import AiDecisionCard from '@/components/ai-decision-card.vue';
import { makeArticle } from '../helpers/fixtures';

describe('ai-decision-card.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders nothing when no aiDecision', () => {
    const wrapper = mount(AiDecisionCard, {
      props: { article: makeArticle({ aiDecision: null }) },
    });
    expect(wrapper.find('section').exists()).toBe(false);
  });

  it('renders Included label and verified icon for include decision', () => {
    const wrapper = mount(AiDecisionCard, {
      props: { article: makeArticle({ aiDecision: 'include', aiConfidence: 0.9 }) },
    });
    expect(wrapper.text()).toContain('Included');
    expect(wrapper.text()).toContain('verified');
    expect(wrapper.text()).toContain('90% Confidence');
  });

  it('renders Excluded label and cancel icon for exclude decision', () => {
    const wrapper = mount(AiDecisionCard, {
      props: { article: makeArticle({ aiDecision: 'exclude', aiConfidence: 0.7 }) },
    });
    expect(wrapper.text()).toContain('Excluded');
    expect(wrapper.text()).toContain('cancel');
  });

  it('shows --- when aiConfidence is null', () => {
    const wrapper = mount(AiDecisionCard, {
      props: { article: makeArticle({ aiDecision: 'include', aiConfidence: null }) },
    });
    expect(wrapper.text()).toContain('--- Confidence');
  });

  it('renders reasoning text when present', () => {
    const wrapper = mount(AiDecisionCard, {
      props: {
        article: makeArticle({
          aiDecision: 'include',
          aiConfidence: 0.8,
          aiReasoning: 'This paper meets all inclusion criteria.',
        }),
      },
    });
    expect(wrapper.text()).toContain('Reasoning:');
    expect(wrapper.text()).toContain('This paper meets all inclusion criteria.');
  });

  it('omits reasoning paragraph when absent', () => {
    const wrapper = mount(AiDecisionCard, {
      props: { article: makeArticle({ aiDecision: 'include', aiConfidence: 0.5 }) },
    });
    expect(wrapper.text()).not.toContain('Reasoning:');
  });

  it('replaces criterion UUIDs in reasoning with numbered refs', async () => {
    const { useCriteriaStore } = await import('@/stores/criteria');
    const store = useCriteriaStore();
    const uuid = '550e8400-e29b-41d4-a716-446655440000';
    store.criteria = [
      {
        id: uuid,
        criterionType: 'inclusion',
        text: 'Must be human',
        priority: 'high',
        createdAt: '',
      },
    ];
    store.inclusionCriteria = [store.criteria[0]!];

    const wrapper = mount(AiDecisionCard, {
      props: {
        article: makeArticle({
          aiDecision: 'include',
          aiConfidence: 0.9,
          aiReasoning: `Matches criterion ${uuid} precisely.`,
        }),
      },
    });
    expect(wrapper.text()).not.toContain(uuid);
    expect(wrapper.text()).toContain('[1]');
  });

  it('collapses double brackets in reasoning', () => {
    const wrapper = mount(AiDecisionCard, {
      props: {
        article: makeArticle({
          aiDecision: 'include',
          aiConfidence: 0.9,
          aiReasoning: 'See [[reference]] here.',
        }),
      },
    });
    expect(wrapper.text()).toContain('[reference]');
    expect(wrapper.text()).not.toContain('[[reference]]');
  });
});
