import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createRouter, createMemoryHistory } from 'vue-router';

/*
 * Card stubs: settings-view's own behavior (rail, scroll-spy, deep links) is
 * under test, not the 10 cards (each has its own spec). Every stub is a single
 * root section so the `id` attribute falls through exactly like production.
 */
vi.mock('@/components/settings/settings-ai-section.vue', () => ({
  default: { name: 'SettingsAiSection', template: '<section class="ai-section" />' },
}));
vi.mock('@/components/settings/settings-embeddings.vue', () => ({
  default: { name: 'SettingsEmbeddings', template: '<section class="settings-card" />' },
}));
vi.mock('@/components/settings/settings-ai-summaries.vue', () => ({
  default: { name: 'SettingsAiSummaries', template: '<section class="settings-card" />' },
}));
vi.mock('@/components/settings/settings-screening-preferences.vue', () => ({
  default: { name: 'SettingsScreeningPreferences', template: '<section class="settings-card" />' },
}));
vi.mock('@/components/settings/settings-storage.vue', () => ({
  default: { name: 'SettingsStorage', template: '<section class="settings-card" />' },
}));
vi.mock('@/components/settings/settings-reprocessing.vue', () => ({
  default: { name: 'SettingsReprocessing', template: '<section class="settings-card" />' },
}));
vi.mock('@/components/settings/settings-project-management.vue', () => ({
  default: { name: 'SettingsProjectManagement', template: '<section class="settings-card" />' },
}));
vi.mock('@/components/settings/settings-openalex.vue', () => ({
  default: { name: 'SettingsOpenAlex', template: '<section class="settings-card" />' },
}));
vi.mock('@/components/settings/settings-notification-history.vue', () => ({
  default: { name: 'SettingsNotificationHistory', template: '<section class="settings-card" />' },
}));
vi.mock('@/components/settings/settings-diagnostics.vue', () => ({
  default: { name: 'SettingsDiagnostics', template: '<section class="settings-card" />' },
}));

import SettingsView from '@/views/settings-view.vue';
import { SETTINGS_SECTIONS } from '@/utils/settings-sections';

function domRect(top: number): DOMRect {
  return {
    top,
    bottom: top,
    left: 0,
    right: 0,
    width: 0,
    height: 0,
    x: 0,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

async function mountView(path = '/settings') {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/settings', component: { template: '<div />' } }],
  });
  await router.push(path);
  await router.isReady();
  const wrapper = mount(SettingsView, {
    attachTo: document.body,
    global: { plugins: [router] },
  });
  await flushPromises();
  return { wrapper, router };
}

/** Wait one animation frame (the deep-link scroll defers to rAF). */
function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

let scrollContainer: HTMLElement;

beforeEach(() => {
  /* Vite's build-time define is absent under vitest; settings-view reads it. */
  vi.stubGlobal('__APP_VERSION__', '0.0.0-test');
  scrollContainer = document.createElement('div');
  scrollContainer.className = 'app-shell__content';
  document.body.appendChild(scrollContainer);
  if (!Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = () => undefined;
  }
});

afterEach(() => {
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('settings-view section rail', () => {
  it('renders_the_rail_and_cards_in_canonical_order', async () => {
    const { wrapper } = await mountView();
    expect(wrapper.findAll('.settings-view__nav-link').map((b) => b.text())).toEqual(
      SETTINGS_SECTIONS.map((section) => section.label)
    );
    expect(wrapper.findAll('.settings-view__cards > *').map((el) => el.attributes('id'))).toEqual(
      SETTINGS_SECTIONS.map((section) => section.id)
    );
  });

  it('clicking_a_rail_entry_scrolls_marks_active_and_writes_the_focus_query', async () => {
    const { wrapper, router } = await mountView();
    const target = wrapper.find('#settings-batch').element;
    const scrollSpy = vi.spyOn(target, 'scrollIntoView');
    const button = wrapper.findAll('.settings-view__nav-link')[7]!;

    await button.trigger('click');
    await flushPromises();

    expect(scrollSpy).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });
    expect(button.attributes('aria-current')).toBe('true');
    expect(router.currentRoute.value.query.focus).toBe('settings-batch');
  });

  it('focus_query_scrolls_to_the_named_card_on_mount', async () => {
    const { wrapper } = await mountView('/settings?focus=settings-history');
    const target = wrapper.find('#settings-history').element;
    const scrollSpy = vi.spyOn(target, 'scrollIntoView');

    await nextFrame();
    await flushPromises();

    expect(scrollSpy).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });
  });

  it('legacy_project_management_focus_still_scrolls', async () => {
    const { wrapper } = await mountView('/settings?focus=project-management');
    const target = wrapper.find('#settings-project-management').element;
    const scrollSpy = vi.spyOn(target, 'scrollIntoView');

    await nextFrame();
    await flushPromises();

    expect(scrollSpy).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });
  });

  it('re_asserts_the_deep_link_scroll_while_cards_settle', async () => {
    // Regression: async status loads change card heights above the target
    // after the first scroll, so the deep link must follow the resize.
    const observers: Array<() => void> = [];
    class ResizeObserverStub {
      constructor(callback: () => void) {
        observers.push(callback);
      }
      observe(): void {}
      disconnect(): void {}
    }
    vi.stubGlobal('ResizeObserver', ResizeObserverStub);

    const { wrapper } = await mountView('/settings?focus=settings-batch');
    const target = wrapper.find('#settings-batch').element;
    const scrollSpy = vi.spyOn(target, 'scrollIntoView');

    await nextFrame();
    await flushPromises();
    expect(scrollSpy).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });

    scrollSpy.mockClear();
    observers.forEach((callback) => callback());
    expect(scrollSpy).toHaveBeenCalledWith({ behavior: 'auto', block: 'start' });
  });

  it('scrolling_updates_the_active_rail_entry', async () => {
    const { wrapper } = await mountView();
    const buttons = wrapper.findAll('.settings-view__nav-link');
    const tops = [-400, -200, 100, 150, 300, 400, 500, 600, 700, 800];
    SETTINGS_SECTIONS.forEach((section, index) => {
      vi.spyOn(wrapper.find(`#${section.id}`).element, 'getBoundingClientRect').mockReturnValue(
        domRect(tops[index]!)
      );
    });
    vi.spyOn(scrollContainer, 'getBoundingClientRect').mockReturnValue(domRect(0));

    scrollContainer.dispatchEvent(new Event('scroll'));
    await flushPromises();

    // Project is the last section at/above the 120px trigger line.
    expect(buttons[2]!.attributes('aria-current')).toBe('true');
    expect(buttons[1]!.attributes('aria-current')).toBeUndefined();
  });
});
