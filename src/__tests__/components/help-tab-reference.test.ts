import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import HelpTabReference from '@/components/help/help-tab-reference.vue';

/**
 * Mount helper: the component uses `useRouter()` so each mount needs a fresh
 * Pinia and a router instance installed as a plugin. Mirrors the help-tab-guide
 * test harness pattern. `scrollIntoView` is stubbed because jsdom does not
 * implement it and the sidebar nav calls it on section selection.
 */
function mountReference() {
  // jsdom lacks scrollIntoView; stub before mount (onMounted deep-link may call it).
  window.HTMLElement.prototype.scrollIntoView = vi.fn();
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div/>' } }],
  });
  return mount(HelpTabReference, { global: { plugins: [pinia, router] } });
}

/** Index of the sidebar nav entry whose text contains the given label. */
function navIndex(wrapper: ReturnType<typeof mountReference>, label: string): number {
  return wrapper.findAll('.ref-nav__link').findIndex((b) => b.text().includes(label));
}

describe('help-tab-reference.vue - Articles, OpenAlex Search, References Tab sections', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders the three new sections with their titles', () => {
    const wrapper = mountReference();
    const titles = wrapper.findAll('.ref-section__title').map((t) => t.text());
    expect(titles).toContain('Articles');
    expect(titles).toContain('OpenAlex Search');
    expect(titles).toContain('References Tab');
  });

  it('orders the new sections around Article Detail Panels in the content', () => {
    const wrapper = mountReference();
    const ids = wrapper.findAll('section.ref-section').map((s) => s.attributes('id'));
    const expected = [
      'ref-translation',
      'ref-articles',
      'ref-review',
      'ref-openalex-search',
      'ref-references-tab',
      'ref-references-citations',
    ];
    const positions = expected.map((id) => ids.indexOf(id));
    expect(positions.every((p) => p >= 0)).toBe(true);
    expect([...positions].sort((a, b) => a - b)).toEqual(positions);
  });

  it('lists the new sidebar entries around Article Detail Panels', () => {
    const wrapper = mountReference();
    const articles = navIndex(wrapper, 'Articles');
    const detail = navIndex(wrapper, 'Article Detail Panels');
    const openalex = navIndex(wrapper, 'OpenAlex Search');
    const referencesTab = navIndex(wrapper, 'References Tab');
    expect([articles, detail, openalex, referencesTab].every((p) => p >= 0)).toBe(true);
    expect(articles).toBeLessThan(detail);
    expect(detail).toBeLessThan(openalex);
    expect(openalex).toBeLessThan(referencesTab);
  });

  it('cross-links the Articles section to the References Tab and OpenAlex Search sections', () => {
    const wrapper = mountReference();
    const links = wrapper
      .find('#ref-articles')
      .findAll('a')
      .map((a) => a.text());
    expect(links).toContain('References Tab');
    expect(links).toContain('OpenAlex Search');
  });

  it('marks a new section active when its sidebar entry is clicked', async () => {
    const wrapper = mountReference();
    const openalexBtn = wrapper
      .findAll('.ref-nav__link')
      .find((b) => b.text().includes('OpenAlex Search'));
    expect(openalexBtn).toBeTruthy();
    await openalexBtn!.trigger('click');
    expect(openalexBtn!.classes()).toContain('ref-nav__link--active');
  });
});

describe('help-tab-reference.vue - Embeddings section', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders the Embeddings section with its title', () => {
    const wrapper = mountReference();
    const titles = wrapper.findAll('.ref-section__title').map((t) => t.text());
    expect(titles).toContain('Embeddings');
  });

  it('places Embeddings below Backup & Restore in content and nav', () => {
    const wrapper = mountReference();
    const ids = wrapper.findAll('section.ref-section').map((s) => s.attributes('id'));
    expect(ids).toContain('ref-embeddings');
    expect(ids.indexOf('ref-backup')).toBeGreaterThan(-1);
    expect(ids.indexOf('ref-backup')).toBeLessThan(ids.indexOf('ref-embeddings'));
    expect(navIndex(wrapper, 'Backup & Restore')).toBeLessThan(navIndex(wrapper, 'Embeddings'));
  });

  it('explains the two providers and processing locations in non-technical terms', () => {
    const wrapper = mountReference();
    const text = wrapper.find('#ref-embeddings').text();
    expect(text).toContain('Configured Provider');
    expect(text).toContain('Bango Local');
    expect(text).toContain('How Bango Decides');
    expect(text).toContain('Where Your Text Is Processed');
    // Scope note carried over from the removed in-card privacy table.
    expect(text).toContain('the embedding step only');
  });
});
