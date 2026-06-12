/**
 * Network export utilities for the Co-Authorship graph.
 *
 * Uses the same Tauri IPC command pattern as RIS export:
 *   save() dialog → tauriCommand('write_*_to_file', { path, ... })
 *
 * Supports PNG (via @sigma/export-image → base64) and GEXF (via graphology-gexf).
 */

import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '../composables/use-tauri-command';
import { toBlob } from '@sigma/export-image';
import gexf from 'graphology-gexf';
import type Sigma from 'sigma';
import type Graph from 'graphology';

export type NetworkExportFormat = 'png' | 'gexf';

/**
 * Export the current Sigma renderer viewport as a PNG image.
 *
 * Flow: toBlob() → FileReader → base64 string → Tauri `write_base64_to_file`
 */
export async function exportNetworkPng(
  renderer: Sigma,
  defaultName = 'coauthor-network.png'
): Promise<boolean> {
  const filePath = await save({
    defaultPath: defaultName,
    filters: [{ name: 'PNG Image', extensions: ['png'] }],
  });

  if (!filePath) return false; // user cancelled

  // Render the graph to a PNG blob via @sigma/export-image
  // Use sigmaSettings to override label threshold so labels always appear in the export
  const blob: Blob = await toBlob(renderer, {
    format: 'png',
    backgroundColor: '#ffffff',
    sigmaSettings: {
      labelRenderedSizeThreshold: 0, // always show labels in export
    },
  });

  // Convert blob → base64 string
  const base64 = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onloadend = () => {
      // reader.result is "data:image/png;base64,AAAA..."
      const dataUrl = reader.result as string;
      const raw = dataUrl.split(',')[1]!; // strip the data URI prefix
      resolve(raw);
    };
    reader.onerror = reject;
    reader.readAsDataURL(blob);
  });

  // Write via Tauri IPC command (bypasses fs plugin permission issues)
  await tauriCommand('write_base64_to_file', { path: filePath, data: base64 });
  return true;
}

/**
 * Export the graphology graph as a GEXF XML file.
 *
 * Flow: gexf.write() → XML string → Tauri `write_text_to_file`
 */
export async function exportNetworkGexf(
  graph: Graph,
  defaultName = 'coauthor-network.gexf'
): Promise<boolean> {
  const filePath = await save({
    defaultPath: defaultName,
    filters: [{ name: 'GEXF File', extensions: ['gexf'] }],
  });

  if (!filePath) return false; // user cancelled

  const xml = gexf.write(graph);
  await tauriCommand('write_text_to_file', { path: filePath, content: xml });
  return true;
}
