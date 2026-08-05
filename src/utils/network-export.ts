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
