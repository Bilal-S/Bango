/**
 * Parse a hex color string (#RRGGBB) into RGB components.
 */
function parseHex(hex: string): { r: number; g: number; b: number } | null {
  const match = hex.replace('#', '').match(/^([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i);
  if (!match) return null;
  return {
    r: parseInt(match[1]!, 16),
    g: parseInt(match[2]!, 16),
    b: parseInt(match[3]!, 16),
  };
}

/**
 * Convert RGB to hex string.
 */
function toHex(r: number, g: number, b: number): string {
  const clamp = (v: number) => Math.max(0, Math.min(255, Math.round(v)));
  return `#${clamp(r).toString(16).padStart(2, '0')}${clamp(g).toString(16).padStart(2, '0')}${clamp(b).toString(16).padStart(2, '0')}`;
}

/**
 * Mix a hex color with white/black to get a lighter/darker variant.
 * @param hex - The base hex color (#RRGGBB)
 * @param amount - 0 = same, 1 = fully white, negative values darken
 */
function mixWithWhite(hex: string, amount: number): string {
  const rgb = parseHex(hex);
  if (!rgb) return hex;
  const r = rgb.r + (255 - rgb.r) * amount;
  const g = rgb.g + (255 - rgb.g) * amount;
  const b = rgb.b + (255 - rgb.b) * amount;
  return toHex(r, g, b);
}

export interface ColorScheme {
  /** Main color (the user-chosen hex) */
  base: string;
  /** Light background for chips/badges (base at ~15% opacity on white) */
  bg: string;
  /** Border color (base at ~40% opacity on white) */
  border: string;
  /** Text color (darkened base for readability) */
  text: string;
  /** Subtle background for hover/active states */
  bgHover: string;
}

/**
 * Derive a full color scheme from a single hex color.
 */
export function deriveColorScheme(hex: string): ColorScheme {
  return {
    base: hex,
    bg: mixWithWhite(hex, 0.85),
    border: mixWithWhite(hex, 0.55),
    text: mixWithWhite(hex, -0.25),
    bgHover: mixWithWhite(hex, 0.75),
  };
}

/**
 * The palette used for hash-based fallback colors when no custom color is set.
 */
const FALLBACK_PALETTE = [
  '#3b82f6', // blue
  '#10b981', // emerald
  '#8b5cf6', // violet
  '#f59e0b', // amber
  '#ef4444', // red
  '#06b6d4', // cyan
  '#ec4899', // pink
  '#84cc16', // lime
];

/**
 * Deterministic color from a string (e.g., tag/label name).
 */
export function hashColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return FALLBACK_PALETTE[Math.abs(hash) % FALLBACK_PALETTE.length]!;
}

/**
 * Get the effective color for a tag/label, considering custom color or hash fallback.
 */
export function getColorScheme(name: string, customColor: string | null | undefined): ColorScheme {
  const hex = customColor || hashColor(name);
  return deriveColorScheme(hex);
}
