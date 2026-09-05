import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';

vi.mock('@/composables/use-export', () => ({
  useExport: () => ({
    exporting: { value: false },
    error: { value: null },
    exportRis: vi.fn().mockResolvedValue(true),
    exportRisForTab: vi.fn().mockResolvedValue(true),
    exportProject: vi.fn().mockResolvedValue(true),
    generateWikiSite: vi.fn().mockResolvedValue(false),
    openWikiExport: vi.fn(),
    downloadWikiZip: vi.fn().mockResolvedValue(true),
    defaultWikiTitle: vi.fn().mockReturnValue('Wiki'),
  }),
}));
vi.mock('@/composables/use-toast', () => ({
  useToast: () => ({ show: vi.fn() }),
}));

import ExportDialog from '@/components/export-dialog.vue';

function mountDialog(props: { activeTab?: string; statusCounts?: Record<string, number> }) {
  return mount(ExportDialog, { props });
}

describe('export-dialog.vue Zotero button', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('zotero_button_visible_in_tab_context', () => {
    const wrapper = mountDialog({ activeTab: 'working', statusCounts: { working: 5 } });
    const button = wrapper.find('[data-test="zotero-export-button"]');
    expect(button.exists()).toBe(true);
    expect(button.text()).toBe('Export Articles (Zotero)');
    // Sits beside the tab RIS export.
    expect(wrapper.text()).toContain('Export Working Articles (RIS)');
  });

  it('zotero_button_visible_in_prisma_context', () => {
    const wrapper = mountDialog({ activeTab: 'prisma' });
    expect(wrapper.find('[data-test="zotero-export-button"]').exists()).toBe(true);
  });

  it('zotero_button_hidden_when_tab_empty', () => {
    // A 0-article tab hides the Zotero button exactly like the RIS export.
    const wrapper = mountDialog({ activeTab: 'working', statusCounts: { working: 0 } });
    expect(wrapper.find('[data-test="zotero-export-button"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('No Working articles found to export');
  });
});
