/* Share Bango: pure helpers for composing share messages and platform URLs.
 * Platform-aware: X/WhatsApp/Bluesky receive URL inline in `text`;
 * Telegram/Reddit/LinkedIn/Email receive URL as separate param.
 * Windows Store URL carries UTM tags; GitHub URL is bare. */

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

/** Metadata for supported share platforms, ordered by reliability. */
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

/** Look up platform metadata by id. Throws if unknown (defensive). */
export function getPlatformInfo(id: SharePlatformId): SharePlatformInfo {
  const info = SHARE_PLATFORMS.find((p) => p.id === id);
  if (!info) {
    throw new Error(`Unknown share platform: ${id}`);
  }
  return info;
}

/** Share target link: Windows Store (with UTM) or GitHub (bare). */
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

/** Compose message body for a platform. Omits bare URL when platform supports
 *  separate URL param; appends URL inline otherwise. */
export function composeMessage(platform: SharePlatformId): string {
  const info = getPlatformInfo(platform);
  if (info.supportsSeparateUrl) {
    return `${SHARE_TITLE}\n\n${SHARE_BODY}`;
  }
  const link = getShareLink(platform);
  return `${SHARE_TITLE}\n\n${SHARE_BODY}\n\n${link}`;
}

/** Build encoded share URL for a platform. `message` and `url` are pre-composed
 *  so the dialog can pass user-edited text without re-composing underneath. */
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
