/**
 * Share Bango - pure helpers for composing share messages and platform URLs.
 *
 * Platform-aware composition:
 * - X / WhatsApp / Bluesky receive a single `text` param with the URL inline.
 * - Telegram / Reddit / LinkedIn / Email receive the URL as a separate param,
 *   so the message body omits the bare URL to avoid duplication.
 *
 * Attribution:
 * - Windows Store URL carries UTM tags (`utm_source=<platform>`,
 *   `utm_medium=app_share`, `utm_campaign=bango_app_share`) so in-app shares
 *   are distinguishable from landing-page clicks. The GitHub URL is bare
 *   (no attribution infrastructure on that side).
 */

import { isWindowsPlatform } from './platform';

export type SharePlatformId =
  | 'x'
  | 'whatsapp'
  | 'telegram'
  | 'bluesky'
  | 'reddit'
  | 'linkedin'
  | 'email';

export interface SharePlatformInfo {
  id: SharePlatformId;
  label: string;
  /** Material Symbols Outlined icon name. */
  icon: string;
  /** Whether the platform accepts a fully pre-populated text body. */
  supportsFullText: boolean;
  /** Whether the URL is passed as a dedicated share param (vs. inline in text). */
  supportsSeparateUrl: boolean;
}

export const SHARE_TITLE = 'Open Source Bango Literature Assistant';
export const SHARE_BODY =
  "I'm using free Bango software for my systematic literature review! Take a look!";
const SHARE_CAMPAIGN = 'bango_app_share';
const SHARE_MEDIUM = 'app_share';
const WINDOWS_STORE_BASE = 'https://apps.microsoft.com/detail/9np2bhgxt8h3';
export const GITHUB_URL = 'https://github.com/Bilal-S/Bango';

/**
 * Metadata for every supported share platform. Order is preserved in the
 * dropdown so the most reliable platforms appear first.
 */
export const SHARE_PLATFORMS: SharePlatformInfo[] = [
  {
    id: 'x',
    label: 'X (Twitter)',
    icon: 'tag',
    supportsFullText: true,
    supportsSeparateUrl: false,
  },
  {
    id: 'whatsapp',
    label: 'WhatsApp',
    icon: 'chat',
    supportsFullText: true,
    supportsSeparateUrl: false,
  },
  {
    id: 'telegram',
    label: 'Telegram',
    icon: 'send',
    supportsFullText: true,
    supportsSeparateUrl: true,
  },
  {
    id: 'bluesky',
    label: 'Bluesky',
    icon: 'cloud',
    supportsFullText: true,
    supportsSeparateUrl: false,
  },
  {
    id: 'reddit',
    label: 'Reddit',
    icon: 'forum',
    supportsFullText: false,
    supportsSeparateUrl: true,
  },
  {
    id: 'linkedin',
    label: 'LinkedIn',
    icon: 'work',
    supportsFullText: false,
    supportsSeparateUrl: true,
  },
  {
    id: 'email',
    label: 'Email',
    icon: 'mail',
    supportsFullText: true,
    supportsSeparateUrl: true,
  },
];

/** Look up platform metadata by id; throws if unknown (defensive). */
export function getPlatformInfo(id: SharePlatformId): SharePlatformInfo {
  const info = SHARE_PLATFORMS.find((p) => p.id === id);
  if (!info) {
    throw new Error(`Unknown share platform: ${id}`);
  }
  return info;
}

/**
 * Returns the share target link for the active platform:
 * - Windows: Microsoft Store URL with per-platform UTM attribution.
 * - macOS / Linux / other: bare GitHub project URL.
 *
 * The UTM `utm_source` reflects the chosen platform so store-side analytics can
 * distinguish a share sourced from X vs. LinkedIn vs. email.
 */
export function getShareLink(platform: SharePlatformId): string {
  if (isWindowsPlatform()) {
    const params = new URLSearchParams({
      hl: 'en-US',
      gl: 'US',
      utm_source: platform,
      utm_medium: SHARE_MEDIUM,
      utm_campaign: SHARE_CAMPAIGN,
    });
    return `${WINDOWS_STORE_BASE}?${params.toString()}`;
  }
  return GITHUB_URL;
}

/**
 * Composes the message body for a given platform.
 *
 * For platforms where the URL is passed as a dedicated param
 * (`supportsSeparateUrl === true`), the body omits the bare URL to avoid
 * duplication. For platforms that only accept a single `text` param, the URL
 * is appended inline so the recipient can follow it.
 */
export function composeMessage(platform: SharePlatformId): string {
  const info = getPlatformInfo(platform);
  if (info.supportsSeparateUrl) {
    return `${SHARE_TITLE}\n\n${SHARE_BODY}`;
  }
  const link = getShareLink(platform);
  return `${SHARE_TITLE}\n\n${SHARE_BODY}\n\n${link}`;
}

/**
 * Builds the encoded share URL for the given platform, message, and link.
 *
 * `message` and `url` are passed already-composed so the dialog can hand the
 * user's textarea edits straight through without re-composing underneath them.
 */
export function getShareUrl(platform: SharePlatformId, message: string, url: string): string {
  const encodedMsg = encodeURIComponent(message);
  const encodedUrl = encodeURIComponent(url);
  const encodedTitle = encodeURIComponent(SHARE_TITLE);

  switch (platform) {
    case 'x':
      // twitter.com/intent/tweet redirects to x.com and is the canonical path.
      return `https://twitter.com/intent/tweet?text=${encodedMsg}`;
    case 'whatsapp':
      return `https://wa.me/?text=${encodedMsg}`;
    case 'telegram':
      return `https://t.me/share/url?url=${encodedUrl}&text=${encodedMsg}`;
    case 'bluesky':
      return `https://bsky.app/intent/compose?text=${encodedMsg}`;
    case 'reddit':
      return `https://www.reddit.com/submit?title=${encodedTitle}&url=${encodedUrl}`;
    case 'linkedin':
      return `https://www.linkedin.com/sharing/share-offsite/?url=${encodedUrl}`;
    case 'email':
      return `mailto:?subject=${encodedTitle}&body=${encodedMsg}%0A%0A${encodedUrl}`;
  }
}
