import { computed, ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';

/** The fallback title shown when no project name is set. */
export const DEFAULT_PROJECT_TITLE = 'Project Dashboard';

/** Mirrors the backend `PROJECT_NAME_MAX_LEN` (60). The `<input maxlength>`
 *  is the primary gate; this constant keeps the frontend + tests in sync with
 *  the backend hard-cap. */
export const PROJECT_NAME_MAX_LEN = 60;

/**
 * Dashboard project-name controller. Loads the persisted name (if any),
 * exposes a reactive `displayName` that falls back to `DEFAULT_PROJECT_TITLE`
 * when unset, and provides `save` + `clear` for the inline edit UI.
 *
 * Pure IPC wrapper (no DOM): the dashboard view owns the dblclick/input/blur
 * wiring. Mirrors the shape of `use-dashboard` (singleton-friendly: each
 * caller gets its own reactive state but shares the same backend row).
 *
 * @returns reactive refs + the IPC-backed actions.
 */
export function useProjectName() {
  /** The persisted project name, or `null` when unset. */
  const projectName = ref<string | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  /** The text the dashboard header renders: the user's name or the fallback. */
  const displayName = computed(() => projectName.value ?? DEFAULT_PROJECT_TITLE);

  /** True when the user has set a custom name (vs showing the fallback). */
  const hasCustomName = computed(() => projectName.value !== null);

  /**
   * Load the persisted project name from the backend. Safe to call on every
   * dashboard mount (the backend read is a single-row SELECT). Sets `error`
   * instead of throwing so a transient backend hiccup never breaks the page.
   */
  async function load(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const name = await tauriCommand<string | null>('get_project_name');
      projectName.value = name;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      // Keep the previous value (or null) so the dashboard still renders.
    } finally {
      loading.value = false;
    }
  }

  /**
   * Persist a new project name. Empty/whitespace strings are treated as a
   * clear (the backend stores NULL and the dashboard reverts to the fallback).
   * Throws on backend failure so the inline edit UI can surface a toast and
   * keep the draft editable for retry.
   */
  async function save(name: string): Promise<void> {
    await tauriCommand('set_project_name', { value: name });
    projectName.value = name.trim() === '' ? null : name.trim();
  }

  /**
   * Clear the project name (revert to the fallback). Thin wrapper over
   * `save('')` so callers read as intent ("clear") rather than mechanism.
   */
  async function clear(): Promise<void> {
    await save('');
  }

  return {
    projectName,
    displayName,
    hasCustomName,
    loading,
    error,
    load,
    save,
    clear,
  };
}
