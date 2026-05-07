export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

// Lazy singleton: resolved once on first Tauri call, then reused.
// Avoids a dynamic import round-trip on every IPC call.
type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
let _invoke: InvokeFn | null = null;

async function getInvoke(): Promise<InvokeFn> {
  if (!_invoke) {
    const mod = await import('@tauri-apps/api/core');
    _invoke = mod.invoke as InvokeFn;
  }
  return _invoke;
}

export async function tauriCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw new Error(
      `Tauri is not available. Cannot execute command "${command}". ` +
        'Run this app inside the Tauri desktop shell to use backend commands.'
    );
  }
  const invoke = await getInvoke();
  return invoke<T>(command, args);
}
