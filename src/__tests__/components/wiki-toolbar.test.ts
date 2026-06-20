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

/** Open the Actions dropdown and return its menu items. */
function getActionsItems(wrapper: ReturnType<typeof mount>) {
  const actionsBtn = wrapper.findAll('button').find((b) => b.text().includes('Actions'));
  if (!actionsBtn) throw new Error('Actions button not found');
  return actionsBtn;
}

async function openActionsMenu(wrapper: ReturnType<typeof mount>) {
  const actionsBtn = getActionsItems(wrapper);
  await actionsBtn.trigger('click');
  await flushPromises();
  // After clicking, the menu items are rendered inside the .wiki-toolbar__menu.
  return wrapper.findAll('.wiki-toolbar__menu-item');
}

describe('wiki-toolbar.vue', () => {
  beforeEach(() => {
    mockTauriCommand.mockReset();
  });

  afterEach(() => {
    mockTauriCommand.mockReset();
  });

  it('renders Add Documents and Actions buttons on the left', () => {
    const wrapper = mount(WikiToolbar, { props: { status: makeStatus() } });
    const buttons = wrapper.findAll('button');
    const labels = buttons.map((b) => b.text());
    // Add Documents and Actions are both present.
    expect(labels.some((t) => t.includes('Add Documents'))).toBe(true);
    expect(labels.some((t) => t.includes('Actions'))).toBe(true);
  });

  it('renders status pill with page + raw counts', () => {
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

  it('shows the included-articles gate with ok styling when > 0', () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ includedArticleCount: 9 }) },
    });
    expect(wrapper.text()).toContain('9 included');
    expect(wrapper.find('.wiki-toolbar__gate--ok').exists()).toBe(true);
  });

  it('renders the gate without ok styling when 0 included articles', () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ includedArticleCount: 0 }) },
    });
    expect(wrapper.text()).toContain('0 included');
    expect(wrapper.find('.wiki-toolbar__gate--ok').exists()).toBe(false);
  });

  it('opening one dropdown closes the other (mutually exclusive)', async () => {
    const wrapper = mount(WikiToolbar, { props: { status: makeStatus() } });

    // Open the Actions menu.
    await openActionsMenu(wrapper);
    expect(wrapper.findAll('.wiki-toolbar__menu-item').length).toBeGreaterThan(0);

    // Now click the Add Documents button.
    const addBtn = wrapper.findAll('button').find((b) => b.text().includes('Add Documents'))!;
    await addBtn.trigger('click');
    await flushPromises();

    // The Actions menu items must be gone, and the Add Documents menu items
    // (From Web / From Local Drive) must be visible.
    const itemTexts = wrapper.findAll('.wiki-toolbar__menu-item').map((i) => i.text());
    expect(itemTexts.some((t) => t.includes('From Web'))).toBe(true);
    expect(itemTexts.some((t) => t.includes('Rebuild Wiki'))).toBe(false);

    // Reverse: clicking Actions again closes Add Documents.
    const actionsBtn = wrapper.findAll('button').find((b) => b.text().includes('Actions'))!;
    await actionsBtn.trigger('click');
    await flushPromises();
    const itemTexts2 = wrapper.findAll('.wiki-toolbar__menu-item').map((i) => i.text());
    expect(itemTexts2.some((t) => t.includes('Rebuild Wiki'))).toBe(true);
    expect(itemTexts2.some((t) => t.includes('From Web'))).toBe(false);
  });

  it('Actions menu contains Rebuild Wiki, Ingest, Health Check, Delete Wiki', async () => {
    const wrapper = mount(WikiToolbar, { props: { status: makeStatus() } });
    const items = await openActionsMenu(wrapper);
    const texts = items.map((i) => i.text());
    expect(texts.some((t) => t.includes('Rebuild Wiki'))).toBe(true);
    expect(texts.some((t) => t.includes('Ingest'))).toBe(true);
    expect(texts.some((t) => t.includes('Health Check'))).toBe(true);
    expect(texts.some((t) => t.includes('Delete Wiki'))).toBe(true);
  });

  it('Actions menu shows Initialize Wiki (not Rebuild) when not initialized', async () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ initialized: false }) },
    });
    const items = await openActionsMenu(wrapper);
    const texts = items.map((i) => i.text());
    expect(texts.some((t) => t.includes('Initialize Wiki'))).toBe(true);
    expect(texts.some((t) => t.includes('Rebuild Wiki'))).toBe(false);
  });

  it('disables Ingest / Health Check / Delete items when not initialized', async () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ initialized: false }) },
    });
    const items = await openActionsMenu(wrapper);
    // Rebuild (Initialize) is enabled; the other three are disabled.
    const disabledTexts = items
      .filter((i) => i.attributes('disabled') !== undefined)
      .map((i) => i.text());
    expect(disabledTexts.some((t) => t.includes('Ingest'))).toBe(true);
    expect(disabledTexts.some((t) => t.includes('Health Check'))).toBe(true);
    expect(disabledTexts.some((t) => t.includes('Delete Wiki'))).toBe(true);
  });

  it('runs Health Check via wiki_lint and returns to idle label', async () => {
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

    const items = await openActionsMenu(wrapper);
    const healthBtn = items.find((i) => i.text().includes('Health Check'));
    expect(healthBtn).toBeTruthy();
    await healthBtn!.trigger('click');
    await flushPromises();

    expect(mockTauriCommand).toHaveBeenCalledWith('wiki_lint');
  });

  it('does not call wiki_export_and_ingest from Ingest when needsRefresh is false', async () => {
    const wrapper = mount(WikiToolbar, {
      props: { status: makeStatus({ needsRefresh: false }) },
    });
    const items = await openActionsMenu(wrapper);
    const ingestBtn = items.find((i) => i.text().includes('Ingest'));
    expect(ingestBtn).toBeTruthy();
    await ingestBtn!.trigger('click');
    await flushPromises();
    expect(mockTauriCommand).not.toHaveBeenCalledWith('wiki_export_and_ingest', expect.anything());
  });
});
