import { tauriCommand } from './use-tauri-command';

/** Response shape from the `get_startup_status` command. */
interface StartupStatusResponse {
  needsLegacyUpgrade: boolean;
}

/** Result of the `perform_legacy_upgrade` command. */
export interface LegacyUpgradeResult {
  backupPath: string;
  articleCount: number;
}

/**
 * Returns true if the backend detected a legacy schema on startup and the app
 * must run the one-shot upgrade before bootstrapping its stores.
 */
export async function getStartupStatus(): Promise<boolean> {
  try {
    const status = await tauriCommand<StartupStatusResponse>('get_startup_status');
    return status.needsLegacyUpgrade;
  } catch (e) {
    console.error('[startup_upgrade] failed to read startup status:', e);
    return false;
  }
}

/**
 * Runs the legacy -> current schema upgrade. The backend backs up the legacy DB
 * to app_data_dir, rebuilds the schema, reloads the journal index, and restores
 * user data. Returns the backup path + restored article count on success.
 */
export async function performLegacyUpgrade(): Promise<LegacyUpgradeResult> {
  return tauriCommand<LegacyUpgradeResult>('perform_legacy_upgrade');
}
