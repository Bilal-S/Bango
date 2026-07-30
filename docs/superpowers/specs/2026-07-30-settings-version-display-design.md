# Settings Version Display

**Date:** 2026-07-30
**Scope:** Add app version + DB migration version to the Settings page title line.

## Motivation

Currently the Settings page title is a bare `Settings`. Users have no way to see which version of the app they're running or which database schema version is applied without opening DevTools or checking file metadata. Adding `(v2.8.7 / 5-7)` to the title gives immediate diagnostics.

## Format

```
Settings (v2.8.7 / 5-7)
```

- `v2.8.7` = app version from `package.json` (already available via `__APP_VERSION__`)
- `5-7` = applied DB migration version (`PRAGMA user_version`) dash max defined migration version
- Version suffix is smaller grey text (`font-size: 14px`, `color: var(--color-on-surface-variant)`)
- Only shown when `dbMaxVersion > 0` (hidden on DB read failure)

## Backend: `get_app_flags` extension

**File:** `src-tauri/src/commands/app_settings.rs`

Add two fields to `AppFlagsResponse`:
- `db_version: i32` -- actual `PRAGMA user_version` from the live SQLite DB
- `db_max_version: i32` -- `crate::db::migrations::get_migrations().last().unwrap().version`

`get_app_flags` gains a `db_state: State<'_, DbState>` parameter. A new private `read_db_versions(&DbState) -> (i32, i32)` helper locks the DB, reads the pragma, and gets max. Falls back to `(0, 0)` on lock poison / query failure (graceful degradation -- `showVersion` hides the suffix).

## Frontend: `useFeatureFlags` extension

**File:** `src/composables/use-feature-flags.ts`

- Extend `AppFlagsResponse` interface with `dbVersion: number` and `dbMaxVersion: number`
- Add `const dbVersion = ref(0)` and `const dbMaxVersion = ref(0)`
- Populate from the IPC result
- Export in return object alongside `isPremium`

## Frontend: Settings title

**File:** `src/views/settings-view.vue`

**Script:**
```ts
import { computed } from 'vue';
import { useFeatureFlags } from '@/composables/use-feature-flags';
const appVersion = __APP_VERSION__;
const { dbVersion, dbMaxVersion } = useFeatureFlags();
const showVersion = computed(() => dbMaxVersion.value > 0);
```

**Template:**
```html
<h1 class="page-title">
  Settings
  <span v-if="showVersion" class="settings-view__version">
    (v{{ appVersion }} / {{ dbVersion }}-{{ dbMaxVersion }})
  </span>
</h1>
```

**CSS (scoped):**
```css
.settings-view__version {
  font-size: 14px;
  font-weight: 400;
  color: var(--color-on-surface-variant);
  white-space: nowrap;
}
```

## Edge cases

| Scenario | Behavior |
|----------|----------|
| DB lock fails / DB not yet opened | `(0, 0)`, `showVersion = false`, version hidden |
| Fresh DB (just migrated) | `(v2.8.7 / 7-7)` |
| DB behind code (partial migration) | `(v2.8.7 / 5-7)` |
| Future migration added (v8) | Automatically shows new max without code changes |

## Files changed

| File | Lines changed |
|------|---------------|
| `src-tauri/src/commands/app_settings.rs` | ~12 lines |
| `src/composables/use-feature-flags.ts` | ~6 lines |
| `src/views/settings-view.vue` | ~16 lines (script + template + CSS) |

## Verification

- `npm run check:all` (type-check + eslint + prettier + rustfmt + clippy + vitest)
- `cargo test`
- Manual: open Settings page, verify `(v2.8.7 / 7-7)` appears in grey next to the title
