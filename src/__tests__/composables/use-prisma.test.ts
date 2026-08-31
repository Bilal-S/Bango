import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '@/composables/use-tauri-command';
import { buildReportHtml, usePrisma } from '@/composables/use-prisma';

const mockedCommand = vi.mocked(tauriCommand);
const mockedSave = vi.mocked(save);

const REPORT_MD = [
  '# PRISMA Screening Reasons Report',
  '',
  '| Priority | Inclusion criterion | Articles |',
  '| --- | --- | ---: |',
  '| critical | UK study | 3 |',
  '| **Total** |  | **3** |',
  '',
].join('\n');

describe('use-prisma report export', () => {
  beforeEach(() => {
    mockedCommand.mockReset();
    mockedSave.mockReset();
  });

  it('buildReportHtml_renders_markdown_tables_into_print_document', () => {
    const html = buildReportHtml(REPORT_MD);
    expect(html).toContain('<title>PRISMA Screening Reasons Report</title>');
    expect(html).toContain('<table>');
    expect(html).toContain('UK study');
    // Print styles for page-friendly tables are present.
    expect(html).toContain('page-break-inside');
  });

  it('exportReport_markdown_fetches_saves_and_writes_file', async () => {
    mockedCommand.mockResolvedValue(REPORT_MD);
    mockedSave.mockResolvedValue('/tmp/prisma-screening-report.md');
    const { exportReport, error } = usePrisma();

    await exportReport('markdown');

    expect(mockedCommand).toHaveBeenCalledWith('get_prisma_report_markdown');
    expect(mockedSave).toHaveBeenCalledWith({
      defaultPath: 'prisma-screening-report.md',
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    });
    expect(mockedCommand).toHaveBeenCalledWith('write_text_to_file', {
      path: '/tmp/prisma-screening-report.md',
      content: REPORT_MD,
    });
    expect(error.value).toBeNull();
  });

  it('exportReport_markdown_dialog_cancel_skips_write', async () => {
    mockedCommand.mockResolvedValue(REPORT_MD);
    mockedSave.mockResolvedValue(null);
    const { exportReport } = usePrisma();

    await exportReport('markdown');

    expect(mockedCommand).toHaveBeenCalledTimes(1);
    expect(mockedCommand).not.toHaveBeenCalledWith('write_text_to_file', expect.anything());
  });

  it('exportReport_markdown_failure_sets_error', async () => {
    mockedCommand.mockRejectedValue(new Error('db locked'));
    const { exportReport, error } = usePrisma();

    await exportReport('markdown');

    expect(error.value).toBe('db locked');
  });

  it('exportReport_pdf_prints_markdown_html_via_hidden_iframe', async () => {
    vi.useFakeTimers();
    const doc = { open: vi.fn(), write: vi.fn(), close: vi.fn() };
    const iframe = {
      style: {} as Record<string, string>,
      contentDocument: doc,
      contentWindow: {
        focus: vi.fn(),
        print: vi.fn(),
        document: doc,
      },
    };
    const createElementSpy = vi
      .spyOn(document, 'createElement')
      .mockReturnValue(iframe as unknown as HTMLIFrameElement);
    const appendSpy = vi
      .spyOn(document.body, 'appendChild')
      .mockReturnValue(iframe as unknown as Node);
    const removeSpy = vi
      .spyOn(document.body, 'removeChild')
      .mockReturnValue(iframe as unknown as Node);

    try {
      mockedCommand.mockResolvedValue(REPORT_MD);
      const { exportReport, error } = usePrisma();

      await exportReport('pdf');

      expect(mockedCommand).toHaveBeenCalledWith('get_prisma_report_markdown');
      expect(appendSpy).toHaveBeenCalledTimes(1);
      expect(doc.write).toHaveBeenCalledTimes(1);
      const written = doc.write.mock.calls[0]?.[0] ?? '';
      expect(written).toContain('<table>');
      expect(written).toContain('PRISMA Screening Reasons Report');

      // Render wait elapses: print dialog opens, then the iframe is removed.
      await vi.advanceTimersByTimeAsync(500);
      expect(iframe.contentWindow.print).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1000);
      expect(removeSpy).toHaveBeenCalledTimes(1);
      expect(error.value).toBeNull();
    } finally {
      createElementSpy.mockRestore();
      appendSpy.mockRestore();
      removeSpy.mockRestore();
      vi.useRealTimers();
    }
  });
});
