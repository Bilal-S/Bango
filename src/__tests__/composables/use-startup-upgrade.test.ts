import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// Hoisted, shared mock for the Tauri IPC boundary. Each test resets it and
// configures its own resolved/rejected value. This avoids the inter-test
// leakage that per-test `vi.mock` factories cause.
const mockTauriCommand = vi.fn();

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: (...args: unknown[]) => mockTauriCommand(...args),
}));

import {
  decideUpgrade,
  getUpgradeAttempted,
  markUpgradeAttempted,
  getStartupStatus,
  performLegacyUpgrade,
} from '@/composables/use-startup-upgrade';

describe('use-startup-upgrade', () => {
  describe('decideUpgrade', () => {
    it("returns 'skip' when no upgrade is needed and none was attempted", () => {
      expect(decideUpgrade(false, false)).toBe('skip');
    });

    it("returns 'skip' when no upgrade is needed even if one was attempted", () => {
      // The successful path: after the reload, the live probe reports Current.
      expect(decideUpgrade(false, true)).toBe('skip');
    });

    it("returns 'run' when an upgrade is needed and none was attempted", () => {
      expect(decideUpgrade(true, false)).toBe('run');
    });

    it("returns 'stale' when an upgrade is needed but one was already attempted", () => {
      // The loop-breaker: this is the exact condition that was looping forever
      // before the fix. Backend still says Legacy after we already ran the
      // upgrade this session -> must NOT re-run.
      expect(decideUpgrade(true, true)).toBe('stale');
    });
  });

  describe('sessionStorage loop-guard tokens', () => {
    beforeEach(() => {
      sessionStorage.clear();
    });

    afterEach(() => {
      sessionStorage.clear();
    });

    it('getUpgradeAttempted returns false when no token is set', () => {
      expect(getUpgradeAttempted()).toBe(false);
    });

    it('markUpgradeAttempted then getUpgradeAttempted returns true', () => {
      markUpgradeAttempted();
      expect(getUpgradeAttempted()).toBe(true);
    });

    it('getUpgradeAttempted is resilient to sessionStorage throwing', () => {
      // Force sessionStorage.getItem to throw (simulating a hardened context).
      const spy = vi.spyOn(sessionStorage, 'getItem').mockImplementation(() => {
        throw new Error('SecurityError');
      });
      try {
        expect(getUpgradeAttempted()).toBe(false);
      } finally {
        spy.mockRestore();
      }
      // After the spy is removed, the read works normally again.
      sessionStorage.clear();
      expect(getUpgradeAttempted()).toBe(false);
    });

    it('markUpgradeAttempted swallows sessionStorage.setItem errors', () => {
      const spy = vi.spyOn(sessionStorage, 'setItem').mockImplementation(() => {
        throw new Error('QuotaExceeded');
      });
      expect(() => markUpgradeAttempted()).not.toThrow();
      spy.mockRestore();
    });
  });

  describe('getStartupStatus / performLegacyUpgrade (Tauri boundary)', () => {
    beforeEach(() => {
      mockTauriCommand.mockReset();
      sessionStorage.clear();
    });

    afterEach(() => {
      mockTauriCommand.mockReset();
      sessionStorage.clear();
    });

    it('getStartupStatus returns the needsLegacyUpgrade flag from the backend', async () => {
      mockTauriCommand.mockResolvedValue({ needsLegacyUpgrade: true });
      expect(await getStartupStatus()).toBe(true);
    });

    it('getStartupStatus returns false on command error (fail-open for normal boot)', async () => {
      // The production code logs the error via console.error before returning
      // false. Silence it here so it doesn't pollute the test runner output,
      // and assert it was actually called (proves the catch-block ran).
      const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      mockTauriCommand.mockRejectedValue(new Error('ipc failed'));
      try {
        expect(await getStartupStatus()).toBe(false);
        expect(errSpy).toHaveBeenCalledWith(
          '[startup_upgrade] failed to read startup status:',
          expect.any(Error)
        );
      } finally {
        errSpy.mockRestore();
      }
    });

    it('performLegacyUpgrade returns the backend result on success', async () => {
      mockTauriCommand.mockResolvedValue({
        backupPath: '/tmp/backup.bango.json',
        articleCount: 42,
      });
      const result = await performLegacyUpgrade();
      expect(result.articleCount).toBe(42);
      expect(result.backupPath).toBe('/tmp/backup.bango.json');
    });

    it('performLegacyUpgrade propagates the backend error on failure', async () => {
      mockTauriCommand.mockRejectedValue(new Error('rebuild failed'));
      await expect(performLegacyUpgrade()).rejects.toThrow('rebuild failed');
    });

    it('performLegacyUpgrade forwards the command name to the boundary', async () => {
      mockTauriCommand.mockResolvedValue({ backupPath: '', articleCount: 0 });
      await performLegacyUpgrade();
      expect(mockTauriCommand).toHaveBeenCalledWith('perform_legacy_upgrade');
    });

    it('getStartupStatus forwards the command name to the boundary', async () => {
      mockTauriCommand.mockResolvedValue({ needsLegacyUpgrade: false });
      await getStartupStatus();
      expect(mockTauriCommand).toHaveBeenCalledWith('get_startup_status');
    });

    it('exports the expected surface', () => {
      expect(typeof decideUpgrade).toBe('function');
      expect(typeof getStartupStatus).toBe('function');
      expect(typeof performLegacyUpgrade).toBe('function');
      expect(typeof getUpgradeAttempted).toBe('function');
      expect(typeof markUpgradeAttempted).toBe('function');
    });
  });
});
