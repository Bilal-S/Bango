import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface ExclusionReason {
  criterionId: string;
  criterionText: string;
  count: number;
}

export interface PrismaData {
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
  const svgContent = ref<string | null>(null);
  const data = ref<PrismaData | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const showExclusionReasons = ref(false);

  async function loadDiagram(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const [svg, prismaData] = await Promise.all([
        tauriCommand<string>('get_prisma_svg'),
        tauriCommand<PrismaData>('get_prisma_data'),
      ]);
      svgContent.value = svg;
      data.value = prismaData;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to load PRISMA data';
      error.value = msg;
    } finally {
      loading.value = false;
    }
  }

  async function exportSvg(): Promise<void> {
    if (!svgContent.value) return;
    const blob = new Blob([svgContent.value], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'prisma-flow-diagram.svg';
    a.click();
    URL.revokeObjectURL(url);
  }

  async function exportPng(): Promise<void> {
    if (!svgContent.value) return;
    const blob = new Blob([svgContent.value], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement('canvas');
      canvas.width = img.naturalWidth * 2;
      canvas.height = img.naturalHeight * 2;
      const ctx = canvas.getContext('2d');
      if (ctx) {
        ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
        canvas.toBlob((pngBlob) => {
          if (pngBlob) {
            const pngUrl = URL.createObjectURL(pngBlob);
            const a = document.createElement('a');
            a.href = pngUrl;
            a.download = 'prisma-flow-diagram.png';
            a.click();
            URL.revokeObjectURL(pngUrl);
          }
        });
      }
      URL.revokeObjectURL(url);
    };
    img.src = url;
  }

  return {
    svgContent,
    data,
    loading,
    error,
    showExclusionReasons,
    loadDiagram,
    exportSvg,
    exportPng,
  };
}
