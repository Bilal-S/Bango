import { ref } from 'vue';
import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from './use-tauri-command';

export interface ExclusionReason {
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

  return {
    data,
    loading,
    error,
    showExclusionReasons,
    loadDiagram,
    exportSvg,
    exportPng,
  };
}
