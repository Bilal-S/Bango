import { ref } from 'vue';
import { save } from '@tauri-apps/plugin-dialog';
import { marked } from 'marked';
import { tauriCommand } from './use-tauri-command';

/** Export format for the screening reasons report. */
export type PrismaReportFormat = 'markdown' | 'pdf';

interface ExclusionReason {
  criterionId: string;
  criterionText: string;
  count: number;
}

interface PrismaData {
  recordsIdentified: number;
  duplicatesRemoved: number;
  recordsScreened: number;
  recordsExcluded: number;
  recordsExcludedGeneral: number;
  recordsExcludedWithReasons: number;
  recordsAssessed: number;
  recordsInProgress: number;
  studiesIncluded: number;
  exclusionReasons: ExclusionReason[];
}

/** Print-document title for the PDF path (also the suggested filename stem). */
const REPORT_TITLE = 'PRISMA Screening Reasons Report';

/**
 * Build the standalone HTML document for the report print (PDF) path.
 * @param markdown - backend-generated report markdown
 * @returns full HTML document string
 */
export function buildReportHtml(markdown: string): string {
  const body = marked.parse(markdown) as string;
  return `<!DOCTYPE html>
<html>
<head>
  <title>${REPORT_TITLE}</title>
  <style>
    body {
      font-family: 'Segoe UI', 'Helvetica Neue', Arial, sans-serif;
      max-width: 800px;
      margin: 40px auto;
      padding: 0 20px;
      line-height: 1.6;
      color: #1b1b24;
      font-size: 13px;
    }
    h1 { font-size: 20px; margin-bottom: 8px; }
    h2 { font-size: 15px; margin-top: 26px; margin-bottom: 10px; }
    p { margin-bottom: 10px; }
    table { border-collapse: collapse; width: 100%; margin: 10px 0 14px; font-size: 12px; }
    th, td { border: 1px solid #ccc; padding: 5px 8px; text-align: left; }
    th { background: #f3f2f7; }
    td:last-child, th:last-child { text-align: right; }
    @media print {
      body { margin: 0; padding: 20px; }
      table { page-break-inside: auto; }
      tr { page-break-inside: avoid; }
    }
  </style>
</head>
<body>
  ${body}
</body>
</html>`;
}

/**
 * Write HTML into a hidden iframe and open the webview print dialog, where
 * the user chooses "Save as PDF" (AI Summary `exportPdf` pattern).
 * @param html - full HTML document string
 */
function printHtml(html: string): void {
  const iframe = document.createElement('iframe');
  iframe.style.position = 'fixed';
  iframe.style.right = '0';
  iframe.style.bottom = '0';
  iframe.style.width = '0';
  iframe.style.height = '0';
  iframe.style.border = 'none';
  document.body.appendChild(iframe);

  const doc = iframe.contentDocument || iframe.contentWindow?.document;
  if (!doc) {
    document.body.removeChild(iframe);
    return;
  }

  doc.open();
  doc.write(html);
  doc.close();

  // Wait for content to render, then print; clean up after the dialog closes.
  window.setTimeout(() => {
    iframe.contentWindow?.focus();
    iframe.contentWindow?.print();
    window.setTimeout(() => {
      document.body.removeChild(iframe);
    }, 1000);
  }, 500);
}

export function usePrisma() {
  const data = ref<PrismaData | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const showExclusionReasons = ref(false);

  async function loadDiagram(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      data.value = await tauriCommand<PrismaData>('get_prisma_data');
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to load PRISMA data';
      error.value = msg;
    } finally {
      loading.value = false;
    }
  }

  async function exportSvg(): Promise<void> {
    const filePath = await save({
      defaultPath: 'bango-prisma-diagram.svg',
      filters: [{ name: 'SVG', extensions: ['svg'] }],
    });
    if (filePath) {
      await tauriCommand('export_prisma_svg_to_file', { path: filePath });
    }
  }

  async function exportPng(): Promise<void> {
    const filePath = await save({
      defaultPath: 'bango-prisma-diagram.png',
      filters: [{ name: 'PNG', extensions: ['png'] }],
    });
    if (filePath) {
      await tauriCommand('export_prisma_png_to_file', { path: filePath });
    }
  }

  /** Fetch the backend-generated screening reasons report (Markdown). */
  async function fetchReportMarkdown(): Promise<string> {
    return tauriCommand<string>('get_prisma_report_markdown');
  }

  /** Export the screening reasons report as a Markdown file via save dialog. */
  async function exportReportMarkdown(): Promise<void> {
    error.value = null;
    try {
      const content = await fetchReportMarkdown();
      const filePath = await save({
        defaultPath: 'prisma-screening-report.md',
        filters: [{ name: 'Markdown', extensions: ['md'] }],
      });
      if (filePath) {
        await tauriCommand('write_text_to_file', { path: filePath, content });
      }
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to export report';
    }
  }

  /** Export the screening reasons report as PDF via the webview print dialog. */
  async function exportReportPdf(): Promise<void> {
    error.value = null;
    try {
      const markdown = await fetchReportMarkdown();
      printHtml(buildReportHtml(markdown));
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to export report';
    }
  }

  /**
   * Export the screening reasons report in the chosen format.
   * @param format - 'markdown' (save dialog) or 'pdf' (print dialog)
   */
  function exportReport(format: PrismaReportFormat): Promise<void> {
    return format === 'pdf' ? exportReportPdf() : exportReportMarkdown();
  }

  return {
    data,
    loading,
    error,
    showExclusionReasons,
    loadDiagram,
    exportSvg,
    exportPng,
    exportReport,
  };
}
