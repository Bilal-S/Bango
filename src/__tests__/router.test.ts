import { describe, it, expect, vi } from 'vitest';

// The router statically imports Dashboard + ArticleList, which call Tauri APIs
// (event listeners, invoke) at module load. Mock those to avoid happy-dom errors.
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => false,
  tauriCommand: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

vi.mock('@/composables/use-ai-summary', () => ({
  useAiSummary: () => ({
    summaries: { value: [] },
    loading: { value: false },
    error: { value: null },
    generateSummary: vi.fn(),
    getSummary: vi.fn(),
    clearSummary: vi.fn(),
  }),
}));

import router from '@/router';

describe('router', () => {
  it('exports a router with routes', () => {
    expect(router).toBeDefined();
    expect(router.getRoutes().length).toBeGreaterThan(0);
  });

  it('has a dashboard route at root', () => {
    expect(router.resolve({ name: 'dashboard' }).path).toBe('/');
  });

  it('has an articles route', () => {
    expect(router.resolve({ name: 'articles' }).path).toBe('/articles');
  });

  it('has a bibliometrics parent with nested children', () => {
    const biblio = router.resolve({ name: 'bibliometrics' });
    expect(biblio.path).toBe('/bibliometrics');
    const children = router.getRoutes().filter((r) => r.path.startsWith('/bibliometrics/'));
    const childNames = children.map((c) => c.name).filter(Boolean);
    expect(childNames).toContain('coauthors');
    expect(childNames).toContain('citations');
    expect(childNames).toContain('keywords');
    expect(childNames).toContain('timeline');
    expect(childNames).toContain('authors');
    expect(childNames).toContain('cocitations');
  });

  it('resolves nested bibliometric child paths', () => {
    expect(router.resolve({ name: 'coauthors' }).path).toBe('/bibliometrics/coauthors');
    expect(router.resolve({ name: 'timeline' }).path).toBe('/bibliometrics/timeline');
  });

  it('has all expected top-level named routes', () => {
    const names = router
      .getRoutes()
      .map((r) => r.name)
      .filter(Boolean) as string[];
    for (const expected of [
      'dashboard',
      'articles',
      'import',
      'dedup',
      'criteria',
      'screening',
      'tags',
      'prisma',
      'summary',
      'bibliometrics',
      'chat',
      'settings',
      'help',
    ]) {
      expect(names).toContain(expected);
    }
  });
});
