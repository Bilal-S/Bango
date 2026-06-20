import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import type { WikiStatus } from '@/types/wiki';

const mockTauriCommand = vi.fn();
vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: (...args: unknown[]) => mockTauriCommand(...args),
}));

// Mock @tauri-apps/plugin-dialog (used by Add Documents -> From Local Drive).
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn().mockResolvedValue(null),
}));

import WikiToolbar from '@/components/wiki/wiki-toolbar.vue';

function makeStatus(overrides: Partial<WikiStatus> = {}): WikiStatus {
  return {
    configured: true,
    rootDir: '/tmp/wiki-root',
    isCustom: false,
    defaultPath: '/tmp/wiki-root',
    rawCount: 3,
    pageCount: 5,
    needsRefresh: false,
    includedArticleCount: 4,
    initialized: true,
    ...overrides,
  };
}

describe('wiki-toolbar.vue', () => {
  beforeEach(() => {
    mockTauriCommand.mockReset();
  });

  afterEach(() => {
    mockTauriCommand.mockReset();
  });

  it('shows "Initialize Wiki" when not initialized', () => {
    const status = makeStatus({ initialized: false });
    const wrapper = mount(WikiToolbar, { props: { status } });
    expect(wrapper.text()).toContain('Initialize Wiki');
  });

  it('shows "Rebuild Wiki" when already initialized', () => {
    const wrapper = mount(WikiToolbar, { props: { status: makeStatus() } });
    expect(wrapper.text()).toContain('Rebuild Wiki');
  });

  it('renders the status pill with page + raw counts', () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ pageCount: 12, rawCount: 7 }) },
    });
    expect(wrapper.text()).toContain('12 pages');
    expect(wrapper.text()).toContain('7 raw');
  });

  it('shows the stale badge when needsRefresh is true', () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ needsRefresh: true }) },
    });
    expect(wrapper.text()).toContain('stale');
  });

  it('does not show the stale badge when needsRefresh is false', () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ needsRefresh: false }) },
    });
    expect(wrapper.text()).not.toContain('stale');
  });

  it('shows the included-articles gate with a check icon when > 0', () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ includedArticleCount: 9 }) },
    });
    expect(wrapper.text()).toContain('9 included');
    expect(wrapper.find('.wiki-toolbar__gate--ok').exists()).toBe(true);
  });

  it('renders the gate without the ok modifier when 0 included articles', () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ includedArticleCount: 0 }) },
    });
    expect(wrapper.text()).toContain('0 included');
    expect(wrapper.find('.wiki-toolbar__gate--ok').exists()).toBe(false);
  });

  it('disables Add Documents + Ingest + Lint + Delete when not initialized', () => {
    const wrapper = mount(WikiToolbar, { props: { status: makeStatus({ initialized: false }) } });
    const buttons = wrapper.findAll('button');
    const disabledLabels = buttons
      .filter((b) => b.attributes('disabled') !== undefined)
      .map((b) => b.text());
    expect(disabledLabels.some((t) => t.includes('Add Documents'))).toBe(true);
    expect(disabledLabels.some((t) => t.includes('Ingest'))).toBe(true);
    expect(disabledLabels.some((t) => t.includes('Lint'))).toBe(true);
    expect(disabledLabels.some((t) => t.includes('Delete Wiki'))).toBe(true);
  });

  it('runs Lint via wiki_lint and returns to idle label', async () => {
    const report = {
      pageCount: 3,
      issueCount: 0,
      errors: 0,
      warnings: 0,
      infos: 0,
      issues: [],
      slugs: ['alpha', 'beta', 'gamma'],
    };
    mockTauriCommand.mockResolvedValue(report);
    const wrapper = mount(WikiToolbar, { props: { status: makeStatus() } });

    const lintBtn = wrapper.findAll('button').find((b) => b.text().includes('Lint'));
    expect(lintBtn).toBeTruthy();
    await lintBtn!.trigger('click');
    await flushPromises();

    expect(mockTauriCommand).toHaveBeenCalledWith('wiki_lint');
    // After completion the label returns to "Lint" (not "Linting..."). The
    // material-symbol glyph renders as its ligature text in happy-dom, so
    // assert substring, not exact equality.
    expect(lintBtn!.text()).toContain('Lint');
    expect(lintBtn!.text()).not.toContain('Linting');
  });

  it('does not call wiki_export_and_ingest when needsRefresh is false', async () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ needsRefresh: false }) },
    });
    const ingestBtn = wrapper.findAll('button').find((b) => b.text().includes('Ingest'));
    expect(ingestBtn).toBeTruthy();
    await ingestBtn!.trigger('click');
    await flushPromises();
    expect(mockTauriCommand).not.toHaveBeenCalledWith('wiki_export_and_ingest', expect.anything());
  });
});
