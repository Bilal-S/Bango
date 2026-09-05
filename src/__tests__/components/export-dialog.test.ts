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

function mountDialog(props?: { activeTab?: string; statusCounts?: Record<string, number> }) {
  return mount(ExportDialog, props ? { props } : undefined);
}

/** Labels of the main option buttons, in DOM order. */
function optionLabels(wrapper: ReturnType<typeof mountDialog>): string[] {
  return wrapper.findAll('.dialog__options button').map((button) => button.text());
}

describe('export-dialog.vue option list', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('fixed_option_order_prisma_context', () => {
    const wrapper = mountDialog({ activeTab: 'prisma' });
    expect(optionLabels(wrapper)).toEqual([
      'Export Included Articles (RIS)',
      'Export Included Articles (Zotero)',
      'Export Project Backup',
      'Export Wiki Website',
    ]);
  });

  it('fixed_option_order_default_context', () => {
    const wrapper = mountDialog();
    expect(optionLabels(wrapper)).toEqual([
      'Export Included Articles (RIS)',
      'Export Included Articles (Zotero)',
      'Export Project Backup',
      'Export Wiki Website',
    ]);
  });

  it('fixed_option_order_tab_context_uses_scope_aware_labels', () => {
    const wrapper = mountDialog({ activeTab: 'working', statusCounts: { working: 5 } });
    expect(optionLabels(wrapper)).toEqual([
      'Export Working Articles (RIS)',
      'Export Working Articles (Zotero)',
      'Export Project Backup',
      'Export Wiki Website',
    ]);
  });

  it('all_option_buttons_share_the_wiki_button_secondary_style', () => {
    const contexts: { activeTab?: string; statusCounts?: Record<string, number> }[] = [
      { activeTab: 'prisma' },
      { activeTab: 'working', statusCounts: { working: 5 } },
    ];
    for (const props of contexts) {
      const wrapper = mountDialog(props);
      const buttons = wrapper.findAll('.dialog__options button');
      expect(buttons).toHaveLength(4);
      for (const button of buttons) {
        expect(button.classes()).toContain('btn--secondary');
        expect(button.classes()).not.toContain('btn--primary');
      }
    }
  });
});
