import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useFeatureFlags, initFeatureFlags } from '@/composables/use-feature-flags';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: vi.fn(() => true),
  tauriCommand: vi.fn(),
}));

import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

describe('useFeatureFlags', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset module-level refs by re-initializing
  });

  it('exposes premium and initialized refs', () => {
    const { isPremium, initialized } = useFeatureFlags();
    expect(typeof isPremium.value).toBe('boolean');
    expect(typeof initialized.value).toBe('boolean');
  });

  it('initFeatureFlags loads premium flag from backend', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(tauriCommand).mockResolvedValue({ premium: true });

    await initFeatureFlags();
    const { isPremium, initialized } = useFeatureFlags();
    expect(isPremium.value).toBe(true);
    expect(initialized.value).toBe(true);
  });

  it('initFeatureFlags sets premium=false when not in Tauri', async () => {
    vi.mocked(isTauri).mockReturnValue(false);

    await initFeatureFlags();
    const { isPremium, initialized } = useFeatureFlags();
    expect(isPremium.value).toBe(false);
    expect(initialized.value).toBe(true);
  });

  it('initFeatureFlags handles command error gracefully', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(tauriCommand).mockRejectedValue(new Error('cmd failed'));

    await initFeatureFlags();
    const { isPremium, initialized } = useFeatureFlags();
    expect(isPremium.value).toBe(false);
    expect(initialized.value).toBe(true);
  });
});
