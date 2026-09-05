import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { nextTick } from 'vue';

type ProgressHandler = (event: { event: string; id: number; payload: unknown }) => void;
let progressHandler: ProgressHandler | null = null;

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockImplementation(async (_event: string, handler: ProgressHandler) => {
    progressHandler = handler;
    return () => {};
  }),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import ZoteroExportPanel from '@/components/zotero-export-panel.vue';
import type { ZoteroExportPreview, ZoteroExportResult } from '@/types/zotero';

const collections = [
  { key: 'ROOT', name: 'Super Collection', parentKey: null },
  { key: 'CHILD', name: 'More Stuff', parentKey: 'ROOT' },
  { key: 'OTHER', name: 'Another Collection', parentKey: null },
];

const okConnection = {
  status: 'ok',
  apiVersion: '3',
  zoteroVersion: '10.0.1',
  serverId: 'SID1',
  hint: null,
};

const preview: ZoteroExportPreview = {
  totalArticles: 6,
  missingCount: 3,
  alreadyPresentCount: 2,
  noDoiCount: 1,
  fileCount: 2,
};

const exportResult: ZoteroExportResult = {
  exportedCount: 3,
  failedCount: 0,
  unchangedCount: 1,
  alreadyPresentCount: 2,
  noDoiCount: 1,
  fileAttachedCount: 2,
  fileFailedCount: 0,
  fileSkippedCount: 1,
  collectionName: 'Super Collection',
  libraryVersion: 42,
};

/** Mock plan for a healthy open: connection -> collections + defaults -> preview. */
function mockHealthyOpen(overrides: {
  connection?: typeof okConnection;
  defaults?: Record<string, unknown> | null;
  collectionList?: typeof collections;
}) {
  vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'check_zotero_connection':
        return Promise.resolve(overrides.connection ?? okConnection);
      case 'get_zotero_collections':
        return Promise.resolve(overrides.collectionList ?? collections);
      case 'get_zotero_selected_collection':
        return Promise.resolve(overrides.defaults ?? null);
      case 'export_zotero_preview':
        return Promise.resolve(preview);
      default:
        return Promise.reject(new Error(`unexpected: ${cmd}`));
    }
  });
}

function mountPanel() {
  return mount(ZoteroExportPanel, {
    props: { scopeLabel: 'Included articles', status: 'included', screeningErrorsOnly: false },
  });
}

describe('zotero-export-panel.vue', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    progressHandler = null;
  });

  it('opens_panel_loads_collections', async () => {
    mockHealthyOpen({ defaults: null });
    mountPanel();
    await flushPromises();

    expect(tauriCommand).toHaveBeenCalledWith('check_zotero_connection');
    expect(tauriCommand).toHaveBeenCalledWith('get_zotero_collections');
    expect(tauriCommand).toHaveBeenCalledWith('get_zotero_selected_collection');
  });

  it('dropdown_defaults_to_zotero_selection', async () => {
    mockHealthyOpen({ defaults: { name: 'Super Collection', lastCollectionKey: 'OTHER' } });
    const wrapper = mountPanel();
    await flushPromises();

    // The connector-reported exact-name match wins over the last-used key.
    const select = wrapper.find('.zep__select');
    expect((select.element as HTMLSelectElement).value).toBe('ROOT');
    // Selecting a default also loads the DOI-diff preview.
    expect(tauriCommand).toHaveBeenCalledWith('export_zotero_preview', {
      collectionKey: 'ROOT',
      status: 'included',
      screeningErrorsOnly: false,
    });
  });

  it('dropdown_falls_back_to_last_used', async () => {
    // No connector name: fall back to the last used collection.
    mockHealthyOpen({ defaults: { name: null, lastCollectionKey: 'CHILD' } });
    const wrapper = mountPanel();
    await flushPromises();

    const select = wrapper.find('.zep__select');
    expect((select.element as HTMLSelectElement).value).toBe('CHILD');
  });

  it('api_disabled_shows_enable_instructions', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) =>
      cmd === 'check_zotero_connection'
        ? Promise.resolve({
            status: 'api_disabled',
            apiVersion: null,
            zoteroVersion: null,
            serverId: null,
            hint: 'Enable the local API in Zotero under Settings -> Advanced -> "Allow other applications on this computer to communicate with Zotero", then try again.',
          })
        : Promise.reject(new Error(`unexpected: ${cmd}`))
    );
    const wrapper = mountPanel();
    await flushPromises();

    const card = wrapper.find('.zep__card--error');
    expect(card.exists()).toBe(true);
    expect(card.text()).toContain('Settings -> Advanced');
    expect(card.text()).toContain('Allow other applications');
    expect(card.find('button').text()).toBe('Retry');
  });

  it('communication_error_shows_enable_hint', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) =>
      cmd === 'check_zotero_connection'
        ? Promise.resolve({
            status: 'error',
            apiVersion: null,
            zoteroVersion: null,
            serverId: null,
            hint: 'Zotero request failed: HTTP 500',
          })
        : Promise.reject(new Error(`unexpected: ${cmd}`))
    );
    const wrapper = mountPanel();
    await flushPromises();

    // Any other communication error repeats the enable-API hint plus the
    // backend message.
    const card = wrapper.find('.zep__card--error');
    expect(card.text()).toContain('Allow other applications');
    expect(card.text()).toContain('HTTP 500');
  });

  it('older_zotero_shows_version_gate', async () => {
    mockHealthyOpen({ connection: { ...okConnection, zoteroVersion: '9.0.1' } });
    const wrapper = mountPanel();
    await flushPromises();

    expect(wrapper.text()).toContain('requires Zotero 10 or newer');
    const exportButton = wrapper.find('[data-test="export-button"]');
    expect(exportButton.exists()).toBe(true);
    expect((exportButton.element as HTMLButtonElement).disabled).toBe(true);
  });

  it('shows_sync_summary_counts', async () => {
    mockHealthyOpen({ defaults: { name: null, lastCollectionKey: 'ROOT' } });
    const wrapper = mountPanel();
    await flushPromises();

    expect(wrapper.find('[data-test="missing"]').text()).toContain('3 to export');
    expect(wrapper.find('[data-test="already"]').text()).toContain('2 already present');
    expect(wrapper.find('[data-test="no-doi"]').text()).toContain('1 without DOI');
    expect(wrapper.find('[data-test="files"]').text()).toContain('2 full-text files');
  });

  it('export_invokes_command_with_scope', async () => {
    mockHealthyOpen({ defaults: { name: null, lastCollectionKey: 'ROOT' } });
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'export_zotero_collection') return Promise.resolve(exportResult);
      if (cmd === 'check_zotero_connection') return Promise.resolve(okConnection);
      if (cmd === 'get_zotero_collections') return Promise.resolve(collections);
      if (cmd === 'get_zotero_selected_collection')
        return Promise.resolve({ name: null, lastCollectionKey: 'ROOT' });
      if (cmd === 'export_zotero_preview') return Promise.resolve(preview);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const wrapper = mountPanel();
    await flushPromises();

    await wrapper.find('[data-test="export-button"]').trigger('click');
    await flushPromises();

    expect(tauriCommand).toHaveBeenCalledWith('export_zotero_collection', {
      collectionKey: 'ROOT',
      status: 'included',
      screeningErrorsOnly: false,
      includeFiles: true,
    });
  });

  it('authorize_state_prompts_remember', async () => {
    mockHealthyOpen({ defaults: null });
    const wrapper = mountPanel();
    await flushPromises();

    // No stored key -> the backend authorize phase fires (dialog blocks in
    // Zotero); the panel asks the user to tick Remember.
    progressHandler?.({
      event: 'zotero-export:progress',
      id: 0,
      payload: { phase: 'authorize', done: 0, total: 0, failed: 0 },
    });
    await nextTick();

    const card = wrapper.find('[data-test="authorize"]');
    expect(card.exists()).toBe(true);
    expect(card.text()).toContain('Check Remember in the Zotero dialog');
  });

  it('progress_events_update_bar', async () => {
    mockHealthyOpen({ defaults: null });
    const wrapper = mountPanel();
    await flushPromises();

    progressHandler?.({
      event: 'zotero-export:progress',
      id: 1,
      payload: { phase: 'items', done: 5, total: 10, failed: 0 },
    });
    await nextTick();

    expect(wrapper.find('[data-test="progress"]').text()).toContain('items: 5/10');
    const fill = wrapper.find('.zep__progress-fill');
    expect(fill.attributes('style')).toContain('width: 50%');
  });

  it('result_summary_rendered', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'export_zotero_collection') return Promise.resolve(exportResult);
      if (cmd === 'check_zotero_connection') return Promise.resolve(okConnection);
      if (cmd === 'get_zotero_collections') return Promise.resolve(collections);
      if (cmd === 'get_zotero_selected_collection')
        return Promise.resolve({ name: null, lastCollectionKey: 'ROOT' });
      if (cmd === 'export_zotero_preview') return Promise.resolve(preview);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const wrapper = mountPanel();
    await flushPromises();

    await wrapper.find('[data-test="export-button"]').trigger('click');
    await flushPromises();

    const result = wrapper.find('[data-test="result"]');
    expect(result.exists()).toBe(true);
    expect(result.text()).toContain('Exported 3');
    expect(result.text()).toContain('Super Collection');
    expect(result.text()).toContain('2 already present');
    expect(result.text()).toContain('1 skipped (no DOI)');
    expect(result.text()).toContain('Files: 2 attached, 0 failed, 1 skipped');
  });

  it('button_becomes_close_after_completion', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'export_zotero_collection') return Promise.resolve(exportResult);
      if (cmd === 'check_zotero_connection') return Promise.resolve(okConnection);
      if (cmd === 'get_zotero_collections') return Promise.resolve(collections);
      if (cmd === 'get_zotero_selected_collection')
        return Promise.resolve({ name: null, lastCollectionKey: 'ROOT' });
      if (cmd === 'export_zotero_preview') return Promise.resolve(preview);
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const wrapper = mountPanel();
    await flushPromises();

    const button = wrapper.find('[data-test="export-button"]');
    expect(button.text()).toBe('Export');
    expect(wrapper.emitted('close')).toBeUndefined();

    await button.trigger('click');
    await flushPromises();

    // Completed: the primary action renames to Close (enabled) and the next
    // click dismisses the dialog instead of re-running the export.
    expect(button.text()).toBe('Close');
    expect((button.element as HTMLButtonElement).disabled).toBe(false);

    await button.trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('version_gate_precedence_over_enable_api', async () => {
    // Zotero 9 with the API disabled: the connector-ping version wins, so the
    // panel shows the version gate, not the enable-API card.
    vi.mocked(tauriCommand).mockImplementation((cmd: string) =>
      cmd === 'check_zotero_connection'
        ? Promise.resolve({
            status: 'api_disabled',
            apiVersion: null,
            zoteroVersion: '9.0.1',
            serverId: null,
            hint: 'Enable the local API in Zotero under Settings -> Advanced.',
          })
        : Promise.reject(new Error(`unexpected: ${cmd}`))
    );
    const wrapper = mountPanel();
    await flushPromises();

    expect(wrapper.text()).toContain('requires Zotero 10 or newer');
    expect(wrapper.find('.zep__card--error').exists()).toBe(false);
    expect(
      (wrapper.find('[data-test="export-button"]').element as HTMLButtonElement).disabled
    ).toBe(true);
  });

  it('open_panel_maps_transport_error', async () => {
    // An IPC rejection must map to an error connection state - never an
    // unhandled promise rejection with a silent, stuck panel.
    vi.mocked(tauriCommand).mockRejectedValue(new Error('IPC transport failed'));
    const wrapper = mountPanel();
    await flushPromises();

    const card = wrapper.find('.zep__card--error');
    expect(card.exists()).toBe(true);
    expect(card.text()).toContain('Allow other applications');
    expect(card.text()).toContain('IPC transport failed');
  });
});
