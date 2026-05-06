export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function tauriCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw new Error(
      `Tauri is not available. Cannot execute command "${command}". ` +
        'Run this app inside the Tauri desktop shell to use backend commands.'
    );
  }
  // Dynamic import to avoid crash when __TAURI_INTERNALS__ is undefined
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}
