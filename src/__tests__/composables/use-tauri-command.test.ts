import { describe, it, expect, beforeEach, vi } from 'vitest';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

// Helper to set/unset the Tauri marker on window without TS complaints.
function setTauriMarker(present: boolean): void {
  const w = window as unknown as Record<string, unknown>;
  if (present) {
    w.__TAURI_INTERNALS__ = {};
  } else {
    delete w.__TAURI_INTERNALS__;
  }
}

describe('use-tauri-command', () => {
  beforeEach(() => {
    setTauriMarker(false);
  });

  describe('isTauri', () => {
    it('returns false when __TAURI_INTERNALS__ is absent', () => {
      expect(isTauri()).toBe(false);
    });

    it('returns true when __TAURI_INTERNALS__ is present', () => {
      setTauriMarker(true);
      expect(isTauri()).toBe(true);
    });
  });

  describe('tauriCommand', () => {
    it('throws when not in Tauri environment', async () => {
      await expect(tauriCommand('some_cmd')).rejects.toThrow('Tauri is not available');
    });

    it('throws an error mentioning the command name', async () => {
      await expect(tauriCommand('my_command')).rejects.toThrow('my_command');
    });

    it('invokes the Tauri core invoke when in Tauri environment', async () => {
      setTauriMarker(true);
      const mockInvoke = vi.fn().mockResolvedValue('result');
      vi.doMock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

      const mod = await import('@/composables/use-tauri-command');
      const result = await mod.tauriCommand<string>('get_articles', { foo: 1 });

      expect(result).toBe('result');
      vi.doUnmock('@tauri-apps/api/core');
      setTauriMarker(false);
    });
  });
});
