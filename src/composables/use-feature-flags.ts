import { ref } from 'vue';
import { isTauri, tauriCommand } from './use-tauri-command';

interface AppFlagsResponse {
  premium: boolean;
  dbVersion: number;
  dbMaxVersion: number;
}

const premium = ref(false);
const dbVersion = ref(0);
const dbMaxVersion = ref(0);
const initialized = ref(false);

async function fetchFlags(): Promise<void> {
  if (!isTauri()) {
    premium.value = false;
    dbVersion.value = 0;
    dbMaxVersion.value = 0;
    initialized.value = true;
    return;
  }
  try {
    const flags = await tauriCommand<AppFlagsResponse>('get_app_flags');
    premium.value = flags.premium;
    dbVersion.value = flags.dbVersion;
    dbMaxVersion.value = flags.dbMaxVersion;
  } catch {
    premium.value = false;
    dbVersion.value = 0;
    dbMaxVersion.value = 0;
  }
  initialized.value = true;
}

/**
 * Composable for accessing application-level feature flags.
 *
 * Flags are read once during app bootstrap (see `main.ts`).
 * The returned refs are reactive and safe to use in any component.
 */
export function useFeatureFlags() {
  return {
    /** Whether the app was started with `--premium` (persists across restarts). */
    isPremium: premium,
    /** Actual DB migration version (PRAGMA user_version). */
    dbVersion,
    /** Highest defined DB migration version. */
    dbMaxVersion,
    /** Whether flags have been loaded from the backend. */
    initialized,
  };
}

/**
 * Internal - called once from `main.ts` during bootstrap.
 * Do not call from components; use `useFeatureFlags()` instead.
 */
export function initFeatureFlags(): Promise<void> {
  return fetchFlags();
}
