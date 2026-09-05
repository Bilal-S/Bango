import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, type VueWrapper, type DOMWrapper } from '@vue/test-utils';
import { nextTick, ref } from 'vue';

/* Header export controls: the standalone Export SVG / Export PNG buttons are
 * replaced by the Export Diagram dropdown (Export Report pattern) and Export
 * RIS is renamed to Export Data. The composable + ExportDialog are mocked so
 * assertions target the view's own menu wiring. */

const { mockLoadDiagram, mockExportSvg, mockExportPng, mockExportReport } = vi.hoisted(() => ({
  mockLoadDiagram: vi.fn(),
  mockExportSvg: vi.fn(),
  mockExportPng: vi.fn(),
  mockExportReport: vi.fn(),
}));

vi.mock('@/composables/use-prisma', () => ({
  usePrisma: () => ({
    data: ref({
      recordsIdentified: 10,
      duplicatesRemoved: 2,
      recordsScreened: 8,
      recordsExcluded: 3,
      recordsExcludedGeneral: 1,
      recordsExcludedWithReasons: 2,
      recordsAssessed: 5,
      recordsInProgress: 0,
      studiesIncluded: 4,
      exclusionReasons: [],
    }),
    loading: ref(false),
    error: ref(null),
    showExclusionReasons: ref(false),
    loadDiagram: mockLoadDiagram,
    exportSvg: mockExportSvg,
    exportPng: mockExportPng,
    exportReport: mockExportReport,
  }),
}));

vi.mock('@/components/export-dialog.vue', () => ({
  default: { name: 'ExportDialog', template: '<div data-test="export-dialog" />' },
}));

import PrismaDiagram from '@/views/prisma-diagram.vue';

/** The first header dropdown is Export Diagram, the second Export Report. */
function diagramMenu(wrapper: VueWrapper): DOMWrapper<Element> {
  const menu = wrapper.findAll('.export-menu')[0];
  expect(menu).toBeDefined();
  return menu!;
}

function headerButton(wrapper: VueWrapper, label: string) {
  const button = wrapper
    .findAll('.prisma-header__actions button')
    .find((btn) => btn.text().includes(label));
  expect(button).toBeDefined();
  return button!;
}

/** Collapse icon-ligature + text whitespace for substring assertions. */
function norm(text: string): string {
  return text.replace(/\s+/g, ' ').trim();
}

describe('prisma-diagram.vue export controls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads_diagram_on_mount', () => {
    mount(PrismaDiagram);
    expect(mockLoadDiagram).toHaveBeenCalledTimes(1);
  });

  it('header_uses_dropdown_and_renamed_buttons', () => {
    const wrapper = mount(PrismaDiagram);
    const actions = wrapper.find('.prisma-header__actions').text();
    expect(actions).toContain('Export Diagram');
    expect(actions).toContain('Export Data');
    expect(actions).toContain('Export Report');
    expect(actions).toContain('Refresh');
    // The diagram menu items stay hidden until the anchor is clicked.
    expect(actions).not.toContain('Export to PNG');
  });

  it('export_diagram_menu_dispatches_png_and_svg_exports', async () => {
    const wrapper = mount(PrismaDiagram);
    const menu = diagramMenu(wrapper);
    await menu.find('button').trigger('click');
    const items = menu.findAll('li');
    expect(items).toHaveLength(2);
    expect(norm(items[0]!.text())).toContain('Export to PNG');
    expect(norm(items[1]!.text())).toContain('Export SVG');

    await items[0]!.trigger('click');
    expect(mockExportPng).toHaveBeenCalledTimes(1);
    expect(mockExportSvg).not.toHaveBeenCalled();
    // Item pick closes the menu.
    expect(menu.findAll('li')).toHaveLength(0);

    await menu.find('button').trigger('click');
    await menu.findAll('li')[1]!.trigger('click');
    expect(mockExportSvg).toHaveBeenCalledTimes(1);
    expect(mockExportPng).toHaveBeenCalledTimes(1);
  });

  it('export_data_button_opens_export_dialog', async () => {
    const wrapper = mount(PrismaDiagram);
    expect(wrapper.find('[data-test="export-dialog"]').exists()).toBe(false);
    await headerButton(wrapper, 'Export Data').trigger('click');
    expect(wrapper.find('[data-test="export-dialog"]').exists()).toBe(true);
  });

  it('escape_closes_open_diagram_menu', async () => {
    const wrapper = mount(PrismaDiagram);
    const menu = diagramMenu(wrapper);
    await menu.find('button').trigger('click');
    expect(menu.findAll('li')).toHaveLength(2);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await nextTick();
    expect(menu.findAll('li')).toHaveLength(0);
  });

  it('outside_click_closes_open_diagram_menu', async () => {
    const wrapper = mount(PrismaDiagram);
    const menu = diagramMenu(wrapper);
    await menu.find('button').trigger('click');
    expect(menu.findAll('li')).toHaveLength(2);
    document.dispatchEvent(new MouseEvent('click'));
    await nextTick();
    expect(menu.findAll('li')).toHaveLength(0);
  });
});
