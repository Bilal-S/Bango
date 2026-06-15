import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '../composables/use-tauri-command';
import type { JournalYearData, YearCount } from '../composables/use-bibliometrics';

/**
 * Timeline export utilities. Uses the established `save()` dialog +
 * `write_text_to_file` / `write_base64_to_file` IPC pattern (same as
 * `src/utils/network-export.ts`).
 */

/** Export the timeline as a two-section CSV (publications + journals). */
export async function exportTimelineCsv(
  pubs: YearCount[],
  citations: YearCount[],
  journalDistribution: JournalYearData[],
  yearFrom: number,
  yearTo: number
): Promise<boolean> {
  const filePath = await save({
    defaultPath: 'publication-timeline.csv',
    filters: [{ name: 'CSV', extensions: ['csv'] }],
  });
  if (!filePath) return false; // user cancelled

  const lines: string[] = ['section,year,count,citations,journal'];
  const citByYear = new Map<number, number>(citations.map((c) => [c.year, c.count]));
  for (const p of pubs) {
    lines.push(`publications,${p.year},${p.count},${citByYear.get(p.year) ?? 0},`);
  }
  for (const jy of journalDistribution) {
    if (jy.year < yearFrom || jy.year > yearTo) continue;
    // CSV-escape the journal title (wrap in quotes, double internal quotes)
    const safe = `"${jy.journal.replace(/"/g, '""')}"`;
    lines.push(`journals,${jy.year},${jy.count},0,${safe}`);
  }
  await tauriCommand('write_text_to_file', { path: filePath, content: lines.join('\n') });
  return true;
}

/** Serialize an SVG element to a file via the Tauri save dialog. */
export async function exportTimelineSvg(svgEl: SVGSVGElement): Promise<boolean> {
  const filePath = await save({
    defaultPath: 'publication-timeline.svg',
    filters: [{ name: 'SVG', extensions: ['svg'] }],
  });
  if (!filePath) return false; // user cancelled

  const serializer = new XMLSerializer();
  const svgString = serializer.serializeToString(svgEl);
  await tauriCommand('write_text_to_file', { path: filePath, content: svgString });
  return true;
}
