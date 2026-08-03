import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';

import HelpTabZotero from '@/components/help/help-tab-zotero.vue';

/**
 * Mount helper: the component uses `useRouter()` so each mount needs a fresh
 * Pinia and a router instance installed as plugins. Mirrors the wiki-toolbar
 * test harness pattern.
 */
function mountZotero() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div/>' } },
      { path: '/import', component: { template: '<div/>' } },
      { path: '/settings', component: { template: '<div/>' } },
    ],
  });
  return { wrapper: mount(HelpTabZotero, { global: { plugins: [pinia, router] } }), router };
}

describe('help-tab-zotero.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders all six numbered step cards', () => {
    const { wrapper } = mountZotero();
    // Six numbered indicators (1..6).
    const numbers = wrapper.findAll('.ht-step__number').map((n) => n.text());
    expect(numbers).toEqual(['1', '2', '3', '4', '5', '6']);
  });

  it('renders the six step titles in order', () => {
    const { wrapper } = mountZotero();
    const titles = wrapper.findAll('.ht-step__title').map((t) => t.text());
    expect(titles).toEqual([
      'Collect articles in Zotero',
      'Set up automatic file renaming',
      'Export articles as RIS',
      'Copy PDF files to Bango full-text folder',
      'Import the RIS file into Bango',
      'Run Batch Import to attach full text',
    ]);
  });

  it('renders the no-DOI warning callout', () => {
    const { wrapper } = mountZotero();
    const callout = wrapper.find('.ht-zotero__callout');
    expect(callout.exists()).toBe(true);
    // The callout must mention the DOI-only matching limitation so users are
    // not silently misled by Zotero's title-fallback filename behavior.
    expect(wrapper.text()).toContain('only articles with a DOI');
    expect(wrapper.text()).toContain('no DOI');
  });

  it('renders the Zotero template inside a <pre> block on step 2', () => {
    const { wrapper } = mountZotero();
    const pre = wrapper.find('.ht-zotero__pre');
    expect(pre.exists()).toBe(true);
    // The template must contain the DOI branch + the title fallback branch.
    expect(pre.text()).toContain('{{ if DOI }}');
    expect(pre.text()).toContain('{{ else }}');
    expect(pre.text()).toContain('{{ endif }}');
  });

  it('shows a Copy button next to the template that writes to the clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(window, 'navigator', {
      value: { clipboard: { writeText } },
      configurable: true,
    });
    const { wrapper } = mountZotero();
    const copyBtn = wrapper.find('.ht-zotero__copy-btn');
    expect(copyBtn.exists()).toBe(true);
    await copyBtn.trigger('click');
    await flushPromises();
    expect(writeText).toHaveBeenCalledTimes(1);
    // The copied payload is the full template string.
    const copied = writeText.mock.calls[0]![0] as string;
    expect(copied).toContain('{{ if DOI }}');
    expect(copied).toContain('{{ endif }}');
  });

  it('shows the Copied state right after copying, then resets', async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(window, 'navigator', {
      value: { clipboard: { writeText } },
      configurable: true,
    });
    const { wrapper } = mountZotero();
    await wrapper.find('.ht-zotero__copy-btn').trigger('click');
    await flushPromises();
    // Immediately after the click the button shows the "done" styling + label.
    expect(wrapper.find('.ht-zotero__copy-btn--done').exists()).toBe(true);
    expect(wrapper.find('.ht-zotero__copy-btn').text()).toContain('Copied');
    // After the 2s reset timer, the button returns to the default Copy label.
    vi.advanceTimersByTime(2100);
    await flushPromises();
    expect(wrapper.find('.ht-zotero__copy-btn--done').exists()).toBe(false);
    expect(wrapper.find('.ht-zotero__copy-btn').text()).toContain('Copy');
    vi.useRealTimers();
  });

  it('renders Go-to buttons only on the steps that route into Bango (3, 5, 6)', () => {
    const { wrapper } = mountZotero();
    const goButtons = wrapper.findAll('.ht-step__go-btn');
    expect(goButtons).toHaveLength(3);
    const labels = goButtons.map((b) => b.text());
    expect(labels.some((t) => t.includes('Go to Import'))).toBe(true);
    expect(labels.some((t) => t.includes('Go to Settings'))).toBe(true);
  });

  it('navigates to /import when the step 3 Go-to Import button is clicked', async () => {
    const { wrapper, router } = mountZotero();
    const pushSpy = vi.spyOn(router, 'push');
    const goButtons = wrapper.findAll('.ht-step__go-btn');
    // Step 3 is the first Go-to button ("Go to Import").
    await goButtons[0]!.trigger('click');
    expect(pushSpy).toHaveBeenCalledWith('/import');
  });

  it('navigates to /settings when the step 6 Go-to Settings button is clicked', async () => {
    const { wrapper, router } = mountZotero();
    const pushSpy = vi.spyOn(router, 'push');
    const goButtons = wrapper.findAll('.ht-step__go-btn');
    // Step 6 is the last Go-to button ("Go to Settings").
    await goButtons[goButtons.length - 1]!.trigger('click');
    expect(pushSpy).toHaveBeenCalledWith('/settings');
  });

  it('omits Go-to buttons on the external-tool steps (1, 2, 4)', () => {
    const { wrapper } = mountZotero();
    // The external-tool step cards have no `.ht-step__go-btn` inside them.
    // We confirm by checking the three steps whose titles correspond to
    // external actions do not contain a go-btn.
    const stepCards = wrapper.findAll('.ht-step__card');
    // Indices 0, 1, 3 are steps 1, 2, 4.
    expect(stepCards[0]!.find('.ht-step__go-btn').exists()).toBe(false);
    expect(stepCards[1]!.find('.ht-step__go-btn').exists()).toBe(false);
    expect(stepCards[3]!.find('.ht-step__go-btn').exists()).toBe(false);
  });

  it('does not render the demo / about / footer sections (not needed for this tab)', () => {
    const { wrapper } = mountZotero();
    expect(wrapper.find('.ht-demo').exists()).toBe(false);
    expect(wrapper.find('.ht-about').exists()).toBe(false);
    expect(wrapper.find('.ht-footer').exists()).toBe(false);
  });
});
