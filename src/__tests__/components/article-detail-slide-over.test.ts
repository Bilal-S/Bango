import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia } from 'pinia';
import ArticleDetailSlideOver from '@/components/article-detail-slide-over.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import { useToast } from '@/composables/use-toast';

vi.mock('@/composables/use-tauri-command', () => ({ tauriCommand: vi.fn() }));
/* Mock the AI-summary composable: its module scope registers Tauri event
 * listeners, which reject outside the Tauri webview (abstract-summary-view
 * test precedent). */
vi.mock('@/composables/use-ai-summary', () => ({
  parseAiSummary: vi.fn(() => null),
  requestArticleAiSummary: vi.fn(),
  pendingSummaries: { value: new Set<string>() },
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const mockedCommand = vi.mocked(tauriCommand);

const ARTICLE = { id: 'a1', title: 'Sugar tax effects' } as Record<string, unknown>;

function mountSlideOver() {
  return mount(ArticleDetailSlideOver, {
    props: { fullScreen: false },
    global: {
      plugins: [createPinia()],
      stubs: { ArticleDetailPanel: true },
    },
  });
}

describe('article-detail-slide-over.vue', () => {
  beforeEach(() => {
    mockedCommand.mockReset();
  });

  it('open_mounts_panel_and_emits_opened', async () => {
    mockedCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_article') return Promise.resolve(ARTICLE);
      if (cmd === 'get_audit_trail') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const wrapper = mountSlideOver();

    expect(wrapper.findComponent(ArticleDetailPanel).exists()).toBe(false);
    await wrapper.vm.open('a1');

    const panel = wrapper.findComponent(ArticleDetailPanel);
    expect(panel.exists()).toBe(true);
    expect(panel.props('article')).toMatchObject({ id: 'a1' });
    expect(wrapper.emitted('opened')).toHaveLength(1);
  });

  it('open_failure_shows_error_toast_and_stays_closed', async () => {
    mockedCommand.mockRejectedValue(new Error('not found'));
    const wrapper = mountSlideOver();

    await wrapper.vm.open('missing');

    expect(wrapper.findComponent(ArticleDetailPanel).exists()).toBe(false);
    expect(wrapper.emitted('opened')).toBeUndefined();
    const toasts = useToast().toasts.value;
    expect(toasts[toasts.length - 1]?.message).toBe('Failed to load article details');
    expect(toasts[toasts.length - 1]?.type).toBe('error');
  });

  it('close_unmounts_panel_clears_state_and_emits_closed', async () => {
    mockedCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_article') return Promise.resolve(ARTICLE);
      if (cmd === 'get_audit_trail') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const wrapper = mountSlideOver();
    await wrapper.vm.open('a1');
    expect(wrapper.findComponent(ArticleDetailPanel).exists()).toBe(true);

    wrapper.vm.close();
    await wrapper.vm.$nextTick();

    expect(wrapper.findComponent(ArticleDetailPanel).exists()).toBe(false);
    expect(wrapper.emitted('closed')).toHaveLength(1);
  });
});
