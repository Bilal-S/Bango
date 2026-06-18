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
 * sessionStorage key recording that the legacy upgrade has already been
 * attempted in this webview session. Used as the loop-guard safety net so a
 * stale/stuck backend signal can never trigger an endless reload cycle.
 *
 * Scoped to `sessionStorage` (not `localStorage`) intentionally: a full app
 * restart starts a fresh session and should be able to retry the upgrade
 * cleanly. Within a single process/session, an upgrade only ever needs to run
 * once.
 */
const UPGRADE_ATTEMPTED_KEY = 'bango:legacyUpgradeAttempted';

/**
 * Outcome of {@link decideUpgrade}. The bootstrap() caller branches on this:
 *
 * - `'run'`: a legacy upgrade is genuinely required; run it then reload.
 * - `'skip'`: no upgrade needed; proceed with normal store pre-warming.
 * - `'stale'`: the backend reported an upgrade is needed, but we already
 *   attempted one this session. The reload loop guard has tripped. Do NOT
 *   reload again; surface a restart-required error instead.
 */
export type UpgradeDecision = 'run' | 'skip' | 'stale';

/**
 * Pure decision function for the legacy-upgrade boot path. Extracted from
 * `main.ts` so it can be unit-tested without a Tauri runtime.
 *
 * @param needsUpgrade the live `get_startup_status` result from the backend.
 * @param alreadyAttempted whether the upgrade has already run this session
 *   (read from `sessionStorage` by the caller).
 * @returns the {@link UpgradeDecision} the caller should act on.
 */
export function decideUpgrade(needsUpgrade: boolean, alreadyAttempted: boolean): UpgradeDecision {
  if (!needsUpgrade) return 'skip';
  // Backend says upgrade is needed, but we already tried this session. The
  // schema rebuild should have flipped the live probe to Current; if it still
  // reports Legacy, the backend signal is stale (or the rebuild silently
  // failed). Break the loop either way and ask the user to restart.
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
