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
 * sessionStorage key for the legacy-upgrade loop guard. Scoped to
 * `sessionStorage` (not `localStorage`) so a full app restart can retry.
 */
const UPGRADE_ATTEMPTED_KEY = 'bango:legacyUpgradeAttempted';

/** {@link decideUpgrade} outcomes. */
export type UpgradeDecision = 'run' | 'skip' | 'stale';

/**
 * Pure decision function for the legacy-upgrade boot path: `'run'` when an
 * upgrade is genuinely needed, `'skip'` when not, `'stale'` when the loop
 * guard has tripped (backend stuck reporting upgrade-needed).
 */
export function decideUpgrade(needsUpgrade: boolean, alreadyAttempted: boolean): UpgradeDecision {
  if (!needsUpgrade) return 'skip';
  /* Backend says upgrade needed, but we already tried this session. The
  schema rebuild should have flipped the live probe to Current; if it still
  reports Legacy, the signal is stale. Break the loop. */
  if (alreadyAttempted) return 'stale';
  return 'run';
}

/** Read whether the upgrade was already attempted in this session. */
export function getUpgradeAttempted(): boolean {
  try {
    return sessionStorage.getItem(UPGRADE_ATTEMPTED_KEY) === '1';
  } catch {
    // sessionStorage can throw in hardened/SSR contexts; treat as not-attempted
    // so we don't block a legitimate first upgrade.
    return false;
  }
}

/** Record that the upgrade has been attempted in this session. */
export function markUpgradeAttempted(): void {
  try {
    sessionStorage.setItem(UPGRADE_ATTEMPTED_KEY, '1');
  } catch {
    // Swallow: the loop-guard is best-effort; the backend live-probe layer is
    // the primary loop-breaker.
  }
}

/**
 * Returns true if the backend detected a legacy schema on startup and the app
 * must run the one-shot upgrade before bootstrapping its stores. Probes the
 * LIVE schema on every call so a successful upgrade is reflected immediately
 * (the backend also keeps its managed snapshot honest post-upgrade).
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
