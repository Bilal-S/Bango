/* Network export utilities: PNG via @sigma/export-image → base64, GEXF via graphology-gexf.
 * Callers pass a `defaultName` matching their module (e.g. `citation-network.png`).
 * Uses the same save() dialog → Tauri IPC pattern as RIS export. */

import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '../composables/use-tauri-command';
import { toBlob } from '@sigma/export-image';
import gexf from 'graphology-gexf';
import type Sigma from 'sigma';
import type Graph from 'graphology';

export type NetworkExportFormat = 'png' | 'gexf';

/** Chart source shape for the ApexCharts biblio views (PNG export). */
export interface ChartDataUriSource {
  dataURI: () => Promise<{ imgURI: string }>;
}

/**
 * Save an ApexCharts chart as PNG: save dialog, then the chart's data URI
 * (minus the `data:` prefix) written via Tauri IPC.
 * Returns false when the user cancels the dialog.
 */
export async function saveChartPng(
  chart: ChartDataUriSource,
  defaultName: string
): Promise<boolean> {
  const filePath = await save({
    defaultPath: defaultName,
    filters: [{ name: 'PNG Image', extensions: ['png'] }],
  });
  if (!filePath) return false;
  const result = await chart.dataURI();
  const base64 = result.imgURI.split(',')[1] ?? '';
  await tauriCommand('write_base64_to_file', { path: filePath, data: base64 });
  return true;
}

/**
 * Save a chart's rendered SVG as a file (ApexCharts SVG export is unreliable
 * in vue3-apexcharts, so the chart's root SVG DOM element is cloned and
 * serialized with explicit xmlns attrs). `selector` targets the chart's root
 * SVG, e.g. `.scatter-chart svg`. Returns false on cancel or missing SVG.
 */
export async function saveChartSvg(selector: string, defaultName: string): Promise<boolean> {
  const chartEl = document.querySelector(selector);
  if (!chartEl) return false;
  const clone = chartEl.cloneNode(true) as SVGElement;
  clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  clone.setAttribute('xmlns:xlink', 'http://www.w3.org/1999/xlink');
  const svgString = new XMLSerializer().serializeToString(clone);
  const filePath = await save({
    defaultPath: defaultName,
    filters: [{ name: 'SVG', extensions: ['svg'] }],
  });
  if (!filePath) return false;
  await tauriCommand('write_text_to_file', { path: filePath, content: svgString });
  return true;
}

/** Export Sigma renderer viewport as PNG. Flow: toBlob() → FileReader → base64 → IPC. */
export async function exportNetworkPng(
  renderer: Sigma,
  defaultName = 'coauthor-network.png'
): Promise<boolean> {
  const filePath = await save({
    defaultPath: defaultName,
    filters: [{ name: 'PNG Image', extensions: ['png'] }],
  });

  if (!filePath) return false;

  const blob: Blob = await toBlob(renderer, {
    format: 'png',
    backgroundColor: '#ffffff',
    sigmaSettings: { labelRenderedSizeThreshold: 0 },
  });

  const base64 = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onloadend = () => {
      const dataUrl = reader.result as string;
      resolve(dataUrl.split(',')[1]!);
    };
    reader.onerror = reject;
    reader.readAsDataURL(blob);
  });

  await tauriCommand('write_base64_to_file', { path: filePath, data: base64 });
  return true;
}

/** Export graphology graph as GEXF XML. Flow: gexf.write() → XML → IPC. */
export async function exportNetworkGexf(
  graph: Graph,
  defaultName = 'coauthor-network.gexf'
): Promise<boolean> {
  const filePath = await save({
    defaultPath: defaultName,
    filters: [{ name: 'GEXF File', extensions: ['gexf'] }],
  });

  if (!filePath) return false;

  const xml = gexf.write(graph);
  await tauriCommand('write_text_to_file', { path: filePath, content: xml });
  return true;
}
