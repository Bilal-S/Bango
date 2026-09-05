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

  it('renders the two path section titles', () => {
    const { wrapper } = mountZotero();
    const titles = wrapper.findAll('.ht-zotero__path-title').map((t) => t.text());
    expect(titles).toEqual([
      'Automatically Moving Data via API',
      'Manually Moving Data via Import/Export',
    ]);
  });

  it('renders the big - OR - separator between the two paths', () => {
    const { wrapper } = mountZotero();
    const separator = wrapper.find('.ht-zotero__or');
    expect(separator.exists()).toBe(true);
    expect(separator.text()).toContain('- OR -');
    expect(wrapper.findAll('.ht-zotero__or-line')).toHaveLength(2);
  });

  it('renders one API step numbered 1 and six manual steps numbered 1-6', () => {
    const { wrapper } = mountZotero();
    const paths = wrapper.findAll('.ht-zotero__path');
    expect(paths).toHaveLength(2);
    const apiNumbers = paths[0]!.findAll('.ht-step__number').map((n) => n.text());
    const manualNumbers = paths[1]!.findAll('.ht-step__number').map((n) => n.text());
    expect(apiNumbers).toEqual(['1']);
    expect(manualNumbers).toEqual(['1', '2', '3', '4', '5', '6']);
  });

  it('renders the step titles in order within each path', () => {
    const { wrapper } = mountZotero();
    const paths = wrapper.findAll('.ht-zotero__path');
    const apiTitles = paths[0]!.findAll('.ht-step__title').map((t) => t.text());
    const manualTitles = paths[1]!.findAll('.ht-step__title').map((t) => t.text());
    expect(apiTitles).toEqual(['Enable the Zotero local API (recommended)']);
    expect(manualTitles).toEqual([
      'Collect articles in Zotero',
      'Set up automatic file renaming',
      'Export articles as RIS',
      'Copy PDF files to Bango full-text folder',
      'Import the RIS file into Bango',
      'Run Batch Import to attach full text',
    ]);
  });

  it('renders the enable-local-API preference path on step 1', () => {
    const { wrapper } = mountZotero();
    expect(wrapper.text()).toContain(
      'Allow other applications on this computer to communicate with Zotero'
    );
    expect(wrapper.text()).toContain('Import from Zotero');
  });

  it('renders the no-DOI warning callout inside the manual path', () => {
    const { wrapper } = mountZotero();
    const callout = wrapper.find('.ht-zotero__callout');
    expect(callout.exists()).toBe(true);
    // The callout concerns Batch Import DOI matching, so it must live in the
    // manual (import/export) path section, not the API path.
    const paths = wrapper.findAll('.ht-zotero__path');
    expect(paths[0]!.find('.ht-zotero__callout').exists()).toBe(false);
    expect(paths[1]!.find('.ht-zotero__callout').exists()).toBe(true);
    // The callout must mention the DOI-only matching limitation so users are
    // not silently misled by Zotero's title-fallback filename behavior.
    expect(wrapper.text()).toContain('only articles with a DOI');
    expect(wrapper.text()).toContain('no DOI');
  });

  it('renders the Zotero template inside the file-renaming step card', () => {
    const { wrapper } = mountZotero();
    const cards = wrapper.findAll('.ht-step__card');
    const renameCard = cards.find((c) =>
      c.find('.ht-step__title').text().includes('Set up automatic file renaming')
    );
    // Regression: the template block previously rendered on the wrong card
    // (hard-coded step number after the API step was prepended).
    expect(renameCard).toBeDefined();
    const pre = renameCard!.find('.ht-zotero__pre');
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

  it('renders Go-to buttons only on the steps that route into Bango (API 1 + manual 3, 5, 6)', () => {
    const { wrapper } = mountZotero();
    const goButtons = wrapper.findAll('.ht-step__go-btn');
    expect(goButtons).toHaveLength(4);
    const labels = goButtons.map((b) => b.text());
    expect(labels.some((t) => t.includes('Go to Import'))).toBe(true);
    expect(labels.some((t) => t.includes('Go to Settings'))).toBe(true);
  });

  it('navigates to /import when the API step Go-to Import button is clicked', async () => {
    const { wrapper, router } = mountZotero();
    const pushSpy = vi.spyOn(router, 'push');
    const goButtons = wrapper.findAll('.ht-step__go-btn');
    // The API step (enable the local API) is the first Go-to button.
    await goButtons[0]!.trigger('click');
    expect(pushSpy).toHaveBeenCalledWith('/import');
  });

  it('navigates to /settings when the manual step 6 Go-to Settings button is clicked', async () => {
    const { wrapper, router } = mountZotero();
    const pushSpy = vi.spyOn(router, 'push');
    const goButtons = wrapper.findAll('.ht-step__go-btn');
    // Manual step 6 (run Batch Import) is the last Go-to button.
    await goButtons[goButtons.length - 1]!.trigger('click');
    expect(pushSpy).toHaveBeenCalledWith('/settings');
  });

  it('omits Go-to buttons on the external-tool steps (manual 1, 2, 4)', () => {
    const { wrapper } = mountZotero();
    // The external-tool step cards have no `.ht-step__go-btn` inside them.
    const manualCards = wrapper.findAll('.ht-zotero__path')[1]!.findAll('.ht-step__card');
    // Indices 0, 1, 3 are manual steps 1 (collect), 2 (rename), 4 (copy files).
    expect(manualCards[0]!.find('.ht-step__go-btn').exists()).toBe(false);
    expect(manualCards[1]!.find('.ht-step__go-btn').exists()).toBe(false);
    expect(manualCards[3]!.find('.ht-step__go-btn').exists()).toBe(false);
  });

  it('does not render the demo / about / footer sections (not needed for this tab)', () => {
    const { wrapper } = mountZotero();
    expect(wrapper.find('.ht-demo').exists()).toBe(false);
    expect(wrapper.find('.ht-about').exists()).toBe(false);
    expect(wrapper.find('.ht-footer').exists()).toBe(false);
  });
});
