import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';

// Mock useDemo so the component does not pull in the Tauri bridge + demo asset.
vi.mock('@/composables/use-demo', () => ({
  useDemo: () => ({
    demoLoading: { value: false },
    demoError: { value: null },
    loadDemo: vi.fn(),
  }),
}));

import HelpTabGuide from '@/components/help/help-tab-guide.vue';

/**
 * Mount helper: the component uses `useRouter()` so each mount needs a fresh
 * Pinia and a router instance installed as a plugin. Mirrors the help-tab-zotero
 * test harness pattern.
 */
function mountGuide() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div/>' } },
      { path: '/criteria', component: { template: '<div/>' } },
      { path: '/import', component: { template: '<div/>' } },
      { path: '/settings', component: { template: '<div/>' } },
    ],
  });
  return { wrapper: mount(HelpTabGuide, { global: { plugins: [pinia, router] } }), router };
}

describe('help-tab-guide.vue - Starting Points section', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders the #starting-points anchor section', () => {
    const { wrapper } = mountGuide();
    const section = wrapper.find('#starting-points');
    expect(section.exists()).toBe(true);
    expect(section.classes()).toContain('ht-guide__starting-points');
  });

  it('renders three entry-point cards', () => {
    const { wrapper } = mountGuide();
    const cards = wrapper.findAll('.ht-guide__sp-card');
    expect(cards).toHaveLength(3);
    // Each card has a header + navigation button.
    cards.forEach((card) => {
      expect(card.find('.ht-guide__sp-card-header').exists()).toBe(true);
      expect(card.find('.ht-guide__sp-btn').exists()).toBe(true);
    });
  });

  it('entry-point card headers mention the three paths (Aims, Articles, Search)', () => {
    const { wrapper } = mountGuide();
    const headers = wrapper.findAll('.ht-guide__sp-card-header h4').map((h) => h.text());
    expect(headers.some((t) => t.includes('Aims'))).toBe(true);
    expect(headers.some((t) => t.includes('Articles'))).toBe(true);
    expect(headers.some((t) => t.includes('Search'))).toBe(true);
  });

  it('overview card mentions OpenAlex + the Starting Points link', () => {
    const { wrapper } = mountGuide();
    const desc = wrapper.find('.ht-guide__overview-desc');
    expect(desc.exists()).toBe(true);
    expect(desc.text()).toContain('OpenAlex');
    expect(desc.find('a[href="#starting-points"]').exists()).toBe(true);
  });
});

describe('help-tab-guide.vue - Managing Your Project section', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders the Managing Your Project card with the 3-step workflow', () => {
    const { wrapper } = mountGuide();
    const manageCard = wrapper.find('.ht-manage-card');
    expect(manageCard.exists()).toBe(true);
    expect(manageCard.text()).toContain('one project at a time');
    const steps = manageCard.findAll('.ht-manage-steps li');
    expect(steps).toHaveLength(3);
    expect(steps[0]?.text()).toContain('Export Backup');
    expect(steps[1]?.text()).toContain('Delete All Data');
    expect(steps[2]?.text()).toContain('Begin fresh');
  });

  it('Managing Your Project card has an "Open Settings" button', async () => {
    const { wrapper, router } = mountGuide();
    const btn = wrapper.find('.ht-manage-btn');
    expect(btn.exists()).toBe(true);
    expect(btn.text()).toContain('Open Settings');
    const pushSpy = vi.spyOn(router, 'push');
    await btn.trigger('click');
    await flushPromises();
    expect(pushSpy).toHaveBeenCalledWith('/settings');
    pushSpy.mockRestore();
  });
});
