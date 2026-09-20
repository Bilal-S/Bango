/**
 * Settings page section registry: the canonical card order, the short rail
 * labels, and the `?focus=` deep-link resolver. Single source consumed by
 * `settings-view.vue` and its tests.
 */

/** One jump-nav / deep-link target on the Settings page. */
export interface SettingsSection {
  /** DOM id of the card root (assigned via attribute fallthrough in the view). */
  id: string;
  /** One-word rail label. */
  label: string;
}

/** Canonical Settings card order. */
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  { id: 'settings-ai', label: 'AI' },
  { id: 'settings-embeddings', label: 'Embeddings' },
  { id: 'settings-project-management', label: 'Project' },
  { id: 'settings-summaries', label: 'Summaries' },
  { id: 'settings-screening', label: 'Screening' },
  { id: 'settings-search', label: 'Search' },
  { id: 'settings-storage', label: 'Storage' },
  { id: 'settings-batch', label: 'Batch' },
  { id: 'settings-history', label: 'History' },
  { id: 'settings-diagnostics', label: 'Diagnostics' },
];

/** Scroll trigger line (px below the scroll container top) for scroll-spy. */
export const SETTINGS_SECTION_TRIGGER_PX = 120;

/* Legacy `?focus=` values kept working after card renames. */
const LEGACY_FOCUS_ALIASES: Readonly<Record<string, string>> = {
  'project-management': 'settings-project-management',
};

/**
 * Resolve a `?focus=` value to a known settings card id.
 * @param value Raw `route.query.focus` value (string, array, or undefined).
 * @returns The matching section id, or null for unknown/invalid values.
 */
export function resolveSettingsSectionId(value: unknown): string | null {
  if (typeof value !== 'string' || value === '') return null;
  const id = LEGACY_FOCUS_ALIASES[value] ?? value;
  return SETTINGS_SECTIONS.some((section) => section.id === id) ? id : null;
}

/**
 * Pick the active section from each card's top offset relative to the scroll
 * container.
 * @param tops Card top offsets in canonical order (Infinity for missing cards).
 * @param trigger Scroll trigger line in px below the container top.
 * @returns The last section id at or above the trigger, else the first section.
 */
export function activeSectionFromTops(
  tops: readonly number[],
  trigger = SETTINGS_SECTION_TRIGGER_PX
): string {
  const first = SETTINGS_SECTIONS[0];
  if (!first) return '';
  let active = first.id;
  for (let index = 1; index < SETTINGS_SECTIONS.length && index < tops.length; index += 1) {
    const section = SETTINGS_SECTIONS[index];
    const top = tops[index];
    if (!section || top === undefined || top > trigger) break;
    active = section.id;
  }
  return active;
}
