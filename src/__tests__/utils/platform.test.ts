import { describe, it, expect, afterEach, vi } from 'vitest';

// Re-import after manipulating navigator so each test sees the active value.
async function importFresh(): Promise<typeof import('@/utils/platform')> {
  vi.resetModules();
  return (await import('@/utils/platform')) as typeof import('@/utils/platform');
}

describe('platform helpers', () => {
  const original = (globalThis.navigator ?? {}) as Navigator;
  const originalDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator');

  afterEach(() => {
    vi.restoreAllMocks();
    // Restore the original navigator reference entirely.
    if (originalDescriptor) {
      Object.defineProperty(globalThis, 'navigator', originalDescriptor);
    }
  });

  /** Replace globalThis.navigator with a fake carrying the given platform. */
  function setNavigatorPlatform(platform: string | undefined): void {
    Object.defineProperty(globalThis, 'navigator', {
      value: { ...(original ?? {}), platform },
      configurable: true,
      writable: true,
    });
  }

  describe('isMacPlatform', () => {
    it('returns true for macOS / iOS platform strings', async () => {
      for (const p of ['MacIntel', 'Macintosh', 'iPhone', 'iPad', 'iPod']) {
        setNavigatorPlatform(p);
        const { isMacPlatform } = await importFresh();
        expect(isMacPlatform(), `${p} should be Mac`).toBe(true);
      }
    });

    it('returns false for Windows / Linux platform strings', async () => {
      for (const p of ['Win32', 'Win64', 'Linux x86_64', 'Linux armv7l']) {
        setNavigatorPlatform(p);
        const { isMacPlatform } = await importFresh();
        expect(isMacPlatform(), `${p} should not be Mac`).toBe(false);
      }
    });

    it('returns false when navigator is absent (SSR / tests)', async () => {
      const desc = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
      const g = globalThis as unknown as { navigator?: Navigator };
      delete g.navigator;
      try {
        const { isMacPlatform } = await importFresh();
        expect(isMacPlatform()).toBe(false);
      } finally {
        if (desc) Object.defineProperty(globalThis, 'navigator', desc);
      }
    });

    it('returns false when navigator.platform is not a string', async () => {
      Object.defineProperty(globalThis, 'navigator', {
        value: { ...(original ?? {}) },
        configurable: true,
        writable: true,
      });
      const { isMacPlatform } = await importFresh();
      expect(isMacPlatform()).toBe(false);
    });
  });

  describe('SHORTCUT_MODIFIER', () => {
    it('resolves to "Cmd" on an Apple platform', async () => {
      setNavigatorPlatform('MacIntel');
      const { SHORTCUT_MODIFIER } = await importFresh();
      expect(SHORTCUT_MODIFIER).toBe('Cmd');
    });

    it('resolves to "Alt" on a non-Apple platform', async () => {
      setNavigatorPlatform('Win32');
      const { SHORTCUT_MODIFIER } = await importFresh();
      expect(SHORTCUT_MODIFIER).toBe('Alt');
    });
  });
});
