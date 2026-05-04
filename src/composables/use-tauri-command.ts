import { invoke } from '@tauri-apps/api/core';

export async function tauriCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, args);
}
