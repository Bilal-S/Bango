import { describe, it, expect, afterEach, vi } from 'vitest';

// Re-import after manipulating navigator so each test sees the active value.
async function importFresh(): Promise<typeof import('@/utils/share-urls')> {
  vi.resetModules();
  return (await import('@/utils/share-urls')) as typeof import('@/utils/share-urls');
}

describe('share-urls', () => {
  const originalDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  const original = (globalThis.navigator ?? {}) as Navigator;

  afterEach(() => {
    vi.restoreAllMocks();
    if (originalDescriptor) {
      Object.defineProperty(globalThis, 'navigator', originalDescriptor);
    }
  });

  /** Replace globalThis.navigator with a fake carrying the given platform. */
  function setNavigatorPlatform(platform: string | undefined): void {
    Object.defineProperty(globalThis, 'navigator', {
      value: { ...(original ?? {}), platform },
      configurable: true,
      writable: true,
    });
  }

  describe('SHARE_PLATFORMS', () => {
    it('contains all seven platforms in a stable order', async () => {
      const { SHARE_PLATFORMS } = await importFresh();
      const ids = SHARE_PLATFORMS.map((p) => p.id);
      expect(ids).toEqual(['x', 'whatsapp', 'telegram', 'bluesky', 'reddit', 'linkedin', 'email']);
    });

    it('every platform has a unique id, label, and icon', async () => {
      const { SHARE_PLATFORMS } = await importFresh();
      const ids = new Set(SHARE_PLATFORMS.map((p) => p.id));
      const labels = new Set(SHARE_PLATFORMS.map((p) => p.label));
      const icons = new Set(SHARE_PLATFORMS.map((p) => p.icon));
      expect(ids.size).toBe(SHARE_PLATFORMS.length);
      expect(labels.size).toBe(SHARE_PLATFORMS.length);
      expect(icons.size).toBe(SHARE_PLATFORMS.length);
    });
  });

  describe('getPlatformInfo', () => {
    it('returns metadata for a known id', async () => {
      const { getPlatformInfo } = await importFresh();
      const info = getPlatformInfo('linkedin');
      expect(info.label).toBe('LinkedIn');
      expect(info.supportsFullText).toBe(false);
      expect(info.supportsSeparateUrl).toBe(true);
    });

    it('throws for an unknown id', async () => {
      const { getPlatformInfo } = await importFresh();
      expect(() => getPlatformInfo('myspace' as never)).toThrow(/Unknown share platform/);
    });
  });

  describe('getShareLink', () => {
    it('returns the GitHub URL on macOS', async () => {
      setNavigatorPlatform('MacIntel');
      const { getShareLink, GITHUB_URL } = await importFresh();
      expect(getShareLink('x')).toBe(GITHUB_URL);
    });

    it('returns the GitHub URL on Linux', async () => {
      setNavigatorPlatform('Linux x86_64');
      const { getShareLink, GITHUB_URL } = await importFresh();
      expect(getShareLink('x')).toBe(GITHUB_URL);
    });

    it('returns the Microsoft Store URL with UTM tags on Windows', async () => {
      setNavigatorPlatform('Win32');
      const { getShareLink } = await importFresh();
      const url = getShareLink('twitter_x' as never);
      // The platform arg is passed straight through as utm_source; for this
      // test we use a valid id to keep it realistic.
      const real = getShareLink('x');
      expect(real).toContain('https://apps.microsoft.com/detail/9np2bhgxt8h3');
      expect(real).toContain('utm_source=x');
      expect(real).toContain('utm_medium=app_share');
      expect(real).toContain('utm_campaign=bango_app_share');
      // Sanity: the dummy call also produces the store base.
      expect(url).toContain('apps.microsoft.com');
    });

    it('encodes the platform id into utm_source for each platform', async () => {
      setNavigatorPlatform('Win32');
      const { getShareLink, SHARE_PLATFORMS } = await importFresh();
      for (const p of SHARE_PLATFORMS) {
        const url = getShareLink(p.id);
        expect(url, `${p.id} should carry utm_source`).toContain(`utm_source=${p.id}`);
      }
    });
  });

  describe('composeMessage', () => {
    it('includes the URL inline for platforms without a separate url param', async () => {
      setNavigatorPlatform('MacIntel');
      const { composeMessage, GITHUB_URL } = await importFresh();
      for (const id of ['x', 'whatsapp', 'bluesky'] as const) {
        const msg = composeMessage(id);
        expect(msg, `${id} should embed the URL`).toContain(GITHUB_URL);
      }
    });

    it('omits the bare URL for platforms that take a separate url param', async () => {
      setNavigatorPlatform('MacIntel');
      const { composeMessage, GITHUB_URL } = await importFresh();
      for (const id of ['telegram', 'reddit', 'linkedin', 'email'] as const) {
        const msg = composeMessage(id);
        expect(msg, `${id} should not embed the URL`).not.toContain(GITHUB_URL);
        // But still contains the title and body.
        expect(msg).toContain('Bango');
        expect(msg).toContain('systematic literature review');
      }
    });

    it('embeds the Windows Store URL (with UTM) on Windows for inline platforms', async () => {
      setNavigatorPlatform('Win32');
      const { composeMessage } = await importFresh();
      const msg = composeMessage('x');
      expect(msg).toContain('apps.microsoft.com');
      expect(msg).toContain('utm_source=x');
    });
  });

  describe('getShareUrl', () => {
    const msg = 'Hello world';
    const url = 'https://example.com/foo';

    it('builds a twitter intent URL with the message as text', async () => {
      const { getShareUrl } = await importFresh();
      const out = getShareUrl('x', msg, url);
      expect(out).toBe(`https://twitter.com/intent/tweet?text=${encodeURIComponent(msg)}`);
    });

    it('builds a WhatsApp URL with the message as text', async () => {
      const { getShareUrl } = await importFresh();
      const out = getShareUrl('whatsapp', msg, url);
      expect(out).toBe(`https://wa.me/?text=${encodeURIComponent(msg)}`);
    });

    it('builds a Telegram URL with separate url and text params', async () => {
      const { getShareUrl } = await importFresh();
      const out = getShareUrl('telegram', msg, url);
      expect(out).toBe(
        `https://t.me/share/url?url=${encodeURIComponent(url)}&text=${encodeURIComponent(msg)}`
      );
    });

    it('builds a Bluesky compose URL with the message as text', async () => {
      const { getShareUrl } = await importFresh();
      const out = getShareUrl('bluesky', msg, url);
      expect(out).toBe(`https://bsky.app/intent/compose?text=${encodeURIComponent(msg)}`);
    });

    it('builds a Reddit submit URL with title and url params', async () => {
      const { getShareUrl, SHARE_TITLE } = await importFresh();
      const out = getShareUrl('reddit', msg, url);
      expect(out).toBe(
        `https://www.reddit.com/submit?title=${encodeURIComponent(SHARE_TITLE)}&url=${encodeURIComponent(url)}`
      );
    });

    it('builds a LinkedIn share URL with the url param', async () => {
      const { getShareUrl } = await importFresh();
      const out = getShareUrl('linkedin', msg, url);
      expect(out).toBe(
        `https://www.linkedin.com/sharing/share-offsite/?url=${encodeURIComponent(url)}`
      );
    });

    it('builds a mailto URL with subject and body (message + blank line + url)', async () => {
      const { getShareUrl, SHARE_TITLE } = await importFresh();
      const out = getShareUrl('email', msg, url);
      const expectedBody = `${encodeURIComponent(msg)}%0A%0A${encodeURIComponent(url)}`;
      expect(out).toBe(`mailto:?subject=${encodeURIComponent(SHARE_TITLE)}&body=${expectedBody}`);
    });

    it('URL-encodes special characters in the message', async () => {
      const { getShareUrl } = await importFresh();
      const tricky = 'Hello & world <script>';
      const out = getShareUrl('x', tricky, url);
      expect(out).toContain(encodeURIComponent(tricky));
      // The raw special chars must not leak unencoded.
      expect(out).not.toContain('<script>');
    });
  });

  describe('capability allowlist alignment', () => {
    // The emitted URLs must match the patterns in
    // src-tauri/capabilities/default.json so the Tauri opener permits them.
    it('every emitted URL host matches the documented allowlist', async () => {
      setNavigatorPlatform('MacIntel');
      const { getShareUrl, composeMessage, getShareLink, SHARE_PLATFORMS } = await importFresh();
      const allowedHosts: string[] = [
        'https://twitter.com/',
        'https://wa.me/',
        'https://t.me/',
        'https://bsky.app/',
        'https://www.reddit.com/',
        'https://www.linkedin.com/',
        'mailto:',
      ];
      for (const p of SHARE_PLATFORMS) {
        const msg = composeMessage(p.id);
        const link = getShareLink(p.id);
        const out = getShareUrl(p.id, msg, link);
        const ok = allowedHosts.some((h) => out.startsWith(h));
        expect(ok, `${p.id}: ${out} must start with an allowlisted host`).toBe(true);
      }
    });
  });
});
