import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useProjectName, DEFAULT_PROJECT_TITLE } from '@/composables/use-project-name';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

describe('useProjectName', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('starts with no custom name and falls back to the default title', () => {
      const { projectName, displayName, hasCustomName } = useProjectName();
      expect(projectName.value).toBeNull();
      expect(displayName.value).toBe(DEFAULT_PROJECT_TITLE);
      expect(hasCustomName.value).toBe(false);
    });
  });

  describe('load', () => {
    it('populates projectName when the backend returns a name', async () => {
      vi.mocked(tauriCommand).mockResolvedValueOnce('My Review');
      const { projectName, displayName, hasCustomName, load } = useProjectName();
      await load();
      expect(projectName.value).toBe('My Review');
      expect(displayName.value).toBe('My Review');
      expect(hasCustomName.value).toBe(true);
    });

    it('keeps null + fallback when the backend returns null (unset)', async () => {
      vi.mocked(tauriCommand).mockResolvedValueOnce(null);
      const { projectName, displayName, load } = useProjectName();
      await load();
      expect(projectName.value).toBeNull();
      expect(displayName.value).toBe(DEFAULT_PROJECT_TITLE);
    });

    it('sets error and keeps the previous value on backend failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValueOnce(new Error('boom'));
      const { projectName, error, load } = useProjectName();
      await load();
      expect(projectName.value).toBeNull();
      expect(error.value).toBe('boom');
    });
  });

  describe('save', () => {
    it('persists a non-empty name and updates the reactive ref', async () => {
      const { projectName, displayName, save } = useProjectName();
      await save('New Title');
      expect(tauriCommand).toHaveBeenCalledWith('set_project_name', { value: 'New Title' });
      expect(projectName.value).toBe('New Title');
      expect(displayName.value).toBe('New Title');
    });

    it('treats a whitespace-only name as a clear (reverts to fallback)', async () => {
      const { projectName, displayName, save } = useProjectName();
      await save('   ');
      expect(tauriCommand).toHaveBeenCalledWith('set_project_name', { value: '   ' });
      expect(projectName.value).toBeNull();
      expect(displayName.value).toBe(DEFAULT_PROJECT_TITLE);
    });
  });

  describe('clear', () => {
    it('sends an empty value and resets the reactive ref to null', async () => {
      const { projectName, displayName, clear } = useProjectName();
      await clear();
      expect(tauriCommand).toHaveBeenCalledWith('set_project_name', { value: '' });
      expect(projectName.value).toBeNull();
      expect(displayName.value).toBe(DEFAULT_PROJECT_TITLE);
    });
  });
});
