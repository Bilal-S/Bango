import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useGapAnalysis } from '@/composables/use-gap-analysis';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

describe('useGapAnalysis', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset the module-level singleton state between tests.
    const { clearGapAnalysis } = useGapAnalysis();
    clearGapAnalysis();
  });

  it('exposes reactive state refs with null/empty defaults', () => {
    const c = useGapAnalysis();
    expect(c.gapText.value).toBeNull();
    expect(c.loading.value).toBe(false);
    expect(c.error.value).toBeNull();
    expect(c.generatedAt.value).toBeNull();
  });

  it('loadSaved hydrates state from get_saved_gap_analysis', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({
      gapText: '# Gaps\n\nContent',
      citationStyle: 'APA',
      generatedAt: '2026-07-07T10:00:00Z',
    });

    const c = useGapAnalysis();
    await c.loadSaved();
    expect(tauriCommand).toHaveBeenCalledWith('get_saved_gap_analysis', {});
    expect(c.gapText.value).toBe('# Gaps\n\nContent');
    expect(c.generatedAt.value).toBe('2026-07-07T10:00:00Z');
  });

  it('loadSaved swallows errors and leaves state null', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('boom'));
    const c = useGapAnalysis();
    await c.loadSaved();
    expect(c.gapText.value).toBeNull();
    expect(c.generatedAt.value).toBeNull();
  });

  it('loadSaved ignores a null backend response', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(null);
    const c = useGapAnalysis();
    await c.loadSaved();
    expect(c.gapText.value).toBeNull();
  });

  it('generate calls analyze_research_gaps with citationStyle and sets gapText', async () => {
    // generate() calls analyze_research_gaps, then loadSaved() to pick up the
    // server timestamp. Mock both.
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'analyze_research_gaps') return Promise.resolve('# Gaps\n\nNew content');
      if (cmd === 'get_saved_gap_analysis')
        return Promise.resolve({
          gapText: '# Gaps\n\nNew content',
          citationStyle: 'APA',
          generatedAt: '2026-07-07T11:00:00Z',
        });
      return Promise.resolve(undefined);
    });

    const c = useGapAnalysis();
    await c.generate('APA');
    expect(tauriCommand).toHaveBeenCalledWith('analyze_research_gaps', { citationStyle: 'APA' });
    expect(c.gapText.value).toBe('# Gaps\n\nNew content');
    expect(c.loading.value).toBe(false);
    expect(c.generatedAt.value).toBe('2026-07-07T11:00:00Z');
  });

  it('generate sets error on backend failure', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('LLM not configured'));

    const c = useGapAnalysis();
    await c.generate('MLA');
    expect(c.error.value).toBe('LLM not configured');
    expect(c.loading.value).toBe(false);
    expect(c.gapText.value).toBeNull();
  });

  it('generate toggles loading true then false', async () => {
    let resolveAnalyze: (v: string) => void;
    const pending = new Promise<string>((r) => {
      resolveAnalyze = r;
    });
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'analyze_research_gaps') return pending;
      if (cmd === 'get_saved_gap_analysis') return Promise.resolve(null);
      return Promise.resolve(undefined);
    });

    const c = useGapAnalysis();
    const p = c.generate('APA');
    expect(c.loading.value).toBe(true);
    resolveAnalyze!('content');
    await p;
    expect(c.loading.value).toBe(false);
  });

  it('clearGapAnalysis resets all state', () => {
    const c = useGapAnalysis();
    c.gapText.value = 'stale';
    c.generatedAt.value = 'stale-ts';
    c.error.value = 'stale-err';
    c.clearGapAnalysis();
    expect(c.gapText.value).toBeNull();
    expect(c.generatedAt.value).toBeNull();
    expect(c.error.value).toBeNull();
  });

  it('formatGeneratedAt returns null when no timestamp', () => {
    const c = useGapAnalysis();
    expect(c.formatGeneratedAt()).toBeNull();
  });

  it('formatGeneratedAt formats an ISO timestamp', () => {
    const c = useGapAnalysis();
    c.generatedAt.value = '2026-07-07T10:00:00Z';
    const formatted = c.formatGeneratedAt();
    expect(formatted).not.toBeNull();
    // The exact locale output varies; just assert it is a non-empty string
    // that contains the year.
    expect(formatted).toContain('2026');
  });

  it('formatGeneratedAt falls back to the raw value on invalid input', () => {
    const c = useGapAnalysis();
    c.generatedAt.value = 'not-a-date';
    expect(c.formatGeneratedAt()).toBe('not-a-date');
  });
});
