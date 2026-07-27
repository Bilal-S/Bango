import { describe, it, expect, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';
import AiDecisionCard from '@/components/ai-decision-card.vue';
import { makeArticle, shimLocalStorage } from '../helpers/fixtures';

describe('ai-decision-card.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    // Install the shim onto window so `localStorage` reads/writes resolve to it
    // (happy-dom's built-in localStorage lacks removeItem/clear).
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    localStorage.removeItem('bango-ai-reasoning-expanded');
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

  it('renders reasoning text when present (expanded by default)', () => {
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

  // ── Collapse behavior (mirrors article-metadata.vue) ───────────────────

  it('expands by default and shows the confidence bar + reasoning', () => {
    const wrapper = mount(AiDecisionCard, {
      props: {
        article: makeArticle({
          aiDecision: 'include',
          aiConfidence: 0.8,
          aiReasoning: 'Matches all criteria.',
        }),
      },
    });
    // The expanded body wrapper is present (not display:none).
    const body = wrapper.find('div[data-testid="reasoning-body"], .px-4.pb-4');
    expect(body.exists()).toBe(true);
    expect(wrapper.text()).toContain('Reasoning:');
    // Confidence bar fill exists.
    expect(wrapper.find('.h-full.rounded-full.transition-all').exists()).toBe(true);
  });

  it('hides the confidence bar + reasoning when collapsed', async () => {
    localStorage.setItem('bango-ai-reasoning-expanded', 'false');
    const wrapper = mount(AiDecisionCard, {
      props: {
        article: makeArticle({
          aiDecision: 'include',
          aiConfidence: 0.8,
          aiReasoning: 'Matches all criteria.',
        }),
      },
    });
    // Collapsed: the header is shown but the expanded body is hidden via v-show.
    expect(wrapper.text()).toContain('Included');
    // v-show keeps the element in the DOM but sets display:none; assert the
    // inline style directly (isVisible() does not detect v-show in happy-dom).
    const body = wrapper.find('.px-4.pb-4');
    expect(body.exists()).toBe(true);
    expect(body.attributes('style')).toContain('display: none');
    // The expanded-only delete icon must not be rendered (v-if, so it's absent).
    expect(wrapper.find('[title="Delete AI reasoning and confidence"]').exists()).toBe(false);
  });

  it('toggles expanded state on header click and persists to localStorage', async () => {
    const wrapper = mount(AiDecisionCard, {
      props: {
        article: makeArticle({ aiDecision: 'include', aiConfidence: 0.8 }),
      },
    });
    // Starts expanded. No reasoning text is set, so the Reasoning paragraph
    // is absent even in the expanded state.
    expect(wrapper.text()).not.toContain('Reasoning:');
    const headerBtn = wrapper.find('button');
    expect(headerBtn.exists()).toBe(true);

    // Collapse via click.
    await headerBtn.trigger('click');
    expect(localStorage.getItem('bango-ai-reasoning-expanded')).toBe('false');

    // Expand again.
    await headerBtn.trigger('click');
    expect(localStorage.getItem('bango-ai-reasoning-expanded')).toBe('true');
  });

  // ── Delete AI reasoning (trashcan icon) ────────────────────────────────

  it('emits clearReasoning with the article id when the trash icon is clicked', async () => {
    const article = makeArticle({
      id: 'art-123',
      aiDecision: 'include',
      aiConfidence: 0.9,
      aiReasoning: 'Some reasoning.',
    });
    const wrapper = mount(AiDecisionCard, { props: { article } });
    const trash = wrapper.find('[title="Delete AI reasoning and confidence"]');
    expect(trash.exists()).toBe(true);
    await trash.trigger('click');
    const clearEvents = wrapper.emitted('clearReasoning');
    expect(clearEvents).toBeDefined();
    expect(clearEvents![0]).toEqual(['art-123']);
  });

  it('does not toggle expanded state when the trash icon is clicked (stopPropagation)', async () => {
    const wrapper = mount(AiDecisionCard, {
      props: {
        article: makeArticle({
          aiDecision: 'include',
          aiConfidence: 0.9,
          aiReasoning: 'Some reasoning.',
        }),
      },
    });
    const trash = wrapper.find('[title="Delete AI reasoning and confidence"]');
    const before = localStorage.getItem('bango-ai-reasoning-expanded');
    await trash.trigger('click');
    // Clicking trash should NOT flip the persisted expanded flag.
    expect(localStorage.getItem('bango-ai-reasoning-expanded')).toBe(before);
  });
});
