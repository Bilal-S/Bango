import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import DetailHeader from '@/components/detail-header.vue';
import { makeArticle as makeBaseArticle } from '../helpers/fixtures';
import type { Article } from '@/types';

/**
 * Article factory for the translation test suite (shared base + file
 * defaults). The default factory article is English so
 * `isTranslationEligible` is false unless a test overrides `language`.
 */
function makeArticle(overrides: Partial<Article> = {}): Article {
  return makeBaseArticle({
    abstractText: '',
    authors: [],
    referenceType: 'JOUR',
    ...overrides,
  });
}

const baseProps = {
  article: makeArticle(),
  canRequestAiSummary: false,
  isAiSummaryPending: false,
  hasReturnTarget: false,
  fullScreen: false,
  isLlmConfigured: true,
  // Eligibility is owned by the parent (mirrors canRequestAiSummary). The
  // default factory article is English so `isTranslationEligible` is false.
  canRequestTranslation: false,
  isTranslationEligible: false,
};

describe('detail-header.vue - translation (language-plan-v2)', () => {
  it('shows_translate_button_for_non_english_not_translated_when_llm_configured', () => {
    // Eligible + LLM configured -> parent passes canRequestTranslation=true.
    // The enabled amber translate button renders.
    const wrapper = mount(DetailHeader, {
      props: {
        ...baseProps,
        article: makeArticle({ language: 'French', isTranslated: false }),
        isTranslationEligible: true,
        canRequestTranslation: true,
      },
    });
    const translateBtn = wrapper.find('button[title="Translate to English"]');
    expect(translateBtn.exists()).toBe(true);
    expect(translateBtn.attributes('disabled')).toBeUndefined();
    expect(translateBtn.text()).toContain('translate');
  });

  it('hides_translate_button_when_translated', () => {
    // Once isTranslated is true, the parent passes isTranslationEligible=false
    // and the translate button is replaced by the bright-red "Translated"
    // status chip; the action button does NOT render.
    const wrapper = mount(DetailHeader, {
      props: {
        ...baseProps,
        article: makeArticle({ language: 'French', isTranslated: true }),
        isTranslationEligible: false,
        canRequestTranslation: false,
      },
    });
    const translateBtn = wrapper.find('button[title="Translate to English"]');
    expect(translateBtn.exists()).toBe(false);
    // The translated status chip is rendered instead.
    expect(wrapper.html()).toContain('Translated');
  });

  it('renders_bright_red_translated_badge_when_translated', () => {
    // The TRANSLATED chip must be bright red (text-red-700 / bg-red-50), not
    // the previous emerald palette.
    const wrapper = mount(DetailHeader, {
      props: {
        ...baseProps,
        article: makeArticle({ language: 'French', isTranslated: true }),
        isTranslationEligible: false,
        canRequestTranslation: false,
      },
    });
    const html = wrapper.html();
    expect(html).toContain('Translated');
    expect(html).toContain('text-red-700');
    expect(html).toContain('bg-red-50');
    // Emerald palette must NOT be present on the translated chip.
    expect(html).not.toContain('text-emerald-700');
    expect(html).not.toContain('bg-emerald-50');
  });

  it('disables_translate_button_when_eligible_but_llm_not_configured', () => {
    // Eligible (non-English, not translated, not in-flight) but LLM is not
    // configured: the action is visible but disabled with a tooltip guiding
    // the user to Settings. This is the hover-hint UX the spec requires.
    const wrapper = mount(DetailHeader, {
      props: {
        ...baseProps,
        isLlmConfigured: false,
        article: makeArticle({ language: 'French', isTranslated: false }),
        isTranslationEligible: true,
        canRequestTranslation: false,
      },
    });
    // The enabled translate button must NOT render.
    expect(wrapper.find('button[title="Translate to English"]').exists()).toBe(false);
    // The disabled placeholder button is rendered instead.
    const disabledBtn = wrapper.find(
      'button[title="Configure an LLM provider in Settings to enable translations"]'
    );
    expect(disabledBtn.exists()).toBe(true);
    expect(disabledBtn.attributes('disabled')).toBeDefined();
    expect(disabledBtn.text()).toContain('translate');
  });

  it('hides_translate_action_entirely_when_not_eligible', () => {
    // English article: not eligible. Neither the enabled button nor the
    // disabled placeholder should render (the action is hidden entirely,
    // matching the AI Summary pattern where the button just disappears).
    const wrapper = mount(DetailHeader, {
      props: {
        ...baseProps,
        article: makeArticle({ language: null, isTranslated: false }),
        isLlmConfigured: false,
        isTranslationEligible: false,
        canRequestTranslation: false,
      },
    });
    expect(wrapper.find('button[title="Translate to English"]').exists()).toBe(false);
    expect(
      wrapper
        .find('button[title="Configure an LLM provider in Settings to enable translations"]')
        .exists()
    ).toBe(false);
  });
});
