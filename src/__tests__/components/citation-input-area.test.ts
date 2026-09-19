import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import CitationInputArea from '@/components/citation-input-area.vue';
import type { CitationFinderProgress, CitationFinderReadiness } from '@/types/citation-finder';

function makeReadiness(overrides: Partial<CitationFinderReadiness> = {}): CitationFinderReadiness {
  return {
    totalArticles: 4,
    embeddedCount: 2,
    coveragePct: 50,
    providerSupportsEmbeddings: true,
    statuses: ['working', 'included'],
    embeddingStatus: 'enabled',
    embeddingModel: 'm',
    embeddingBackend: 'configured_provider',
    localReady: false,
    chatProviderSupportsEmbeddings: true,
    ...overrides,
  };
}

function makeProgress(overrides: Partial<CitationFinderProgress> = {}): CitationFinderProgress {
  return {
    phase: 'searching',
    done: 1,
    total: 4,
    overallPercent: 40,
    message: 'Classifying candidates...',
    isRunning: true,
    isCancelled: false,
    ...overrides,
  };
}

function mountArea(props: Partial<InstanceType<typeof CitationInputArea>['$props']> = {}) {
  return mount(CitationInputArea, {
    props: {
      readiness: makeReadiness(),
      toggleState: 'enabled',
      styleValue: 'APA',
      mode: 'whole_block',
      statuses: { working: true, included: true, rejected: false },
      draft: 'Some prose.',
      progress: null,
      loading: false,
      cancelling: false,
      ...props,
    },
  });
}

describe('citation-input-area.vue', () => {
  it('shows the coverage notice while coverage is below 100 and idle', () => {
    const wrapper = mountArea();
    expect(wrapper.find('.citation-coverage-notice').exists()).toBe(true);
    expect(wrapper.text()).toContain('First run will prepare embeddings for 4 article(s)');
  });

  it('hides the coverage notice while a search is running', () => {
    const wrapper = mountArea({ progress: makeProgress() });
    expect(wrapper.find('.citation-coverage-notice').exists()).toBe(false);
  });

  it('shows the disabled-provider banner only in the disabled state', () => {
    const disabled = mountArea({ toggleState: 'disabled' });
    expect(disabled.find('.citation-disabled-banner').exists()).toBe(true);
    const enabled = mountArea();
    expect(enabled.find('.citation-disabled-banner').exists()).toBe(false);
  });

  it('emits update:statuses with a fresh flags object on a checkbox change', async () => {
    const wrapper = mountArea();
    const included = wrapper
      .findAll('label.citation-input-area__checkbox')
      .find((l) => l.text().includes('Included'));
    await included!.find('input').setValue(false);
    expect(wrapper.emitted('update:statuses')).toEqual([
      [{ working: true, included: false, rejected: false }],
    ]);
  });

  it('emits update:mode from the scope segmented buttons', async () => {
    const wrapper = mountArea();
    const perStatement = wrapper
      .findAll('.citation-input-area__mode-btn')
      .find((b) => b.text().includes('Per Statement'));
    await perStatement!.trigger('click');
    expect(wrapper.emitted('update:mode')).toEqual([['per_statement']]);
  });

  it('emits send from the Find Citations button and disables it without prose', async () => {
    const wrapper = mountArea({ draft: '' });
    const find = wrapper.find('.citation-input-area__find-btn');
    expect((find.element as HTMLButtonElement).disabled).toBe(true);
    await wrapper.setProps({ draft: 'Typed prose.' });
    expect((find.element as HTMLButtonElement).disabled).toBe(false);
    await find.trigger('click');
    expect(wrapper.emitted('send')).toHaveLength(1);
  });

  it('replaces the Find button with the live progress + cancel while running', async () => {
    const wrapper = mountArea({ progress: makeProgress() });
    expect(wrapper.find('.citation-input-area__find-btn').exists()).toBe(false);
    expect(wrapper.find('.citation-progress--inline').exists()).toBe(true);
    expect(wrapper.text()).toContain('Classifying candidates...');
    await wrapper.find('.citation-progress__cancel').trigger('click');
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });

  it('emits close and openSettings from their buttons', async () => {
    const wrapper = mountArea();
    await wrapper.find('.citation-input-area__close').trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(1);
    const withBanner = mountArea({ toggleState: 'disabled' });
    await withBanner.find('.citation-disabled-banner__btn').trigger('click');
    expect(withBanner.emitted('openSettings')).toHaveLength(1);
  });
});
