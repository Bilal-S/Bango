import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { useSummary, type CitationStyle } from '@/composables/use-summary';

describe('useSummary', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const { clearSummary } = useSummary();
    clearSummary();
  });

  describe('loadSaved', () => {
    it('populates state from get_saved_summary response', async () => {
      const saved = {
        summaryText: 'A literature review on testing.',
        citationStyle: 'MLA',
        generatedAt: '2025-01-15T10:30:00Z',
      };
      vi.mocked(tauriCommand).mockResolvedValueOnce(saved);

      const { summaryText, generatedAt, citationStyle, loadSaved } = useSummary();
      await loadSaved();

      expect(tauriCommand).toHaveBeenCalledWith('get_saved_summary', {});
      expect(summaryText.value).toBe('A literature review on testing.');
      expect(citationStyle.value).toBe('MLA');
      expect(generatedAt.value).toBe('2025-01-15T10:30:00Z');
    });

    it('leaves state unchanged when saved summary is null', async () => {
      vi.mocked(tauriCommand).mockResolvedValueOnce(null);

      const { summaryText, generatedAt, citationStyle, loadSaved } = useSummary();
      await loadSaved();

      expect(summaryText.value).toBeNull();
      expect(generatedAt.value).toBeNull();
      expect(citationStyle.value).toBe('APA');
    });

    it('silently ignores backend errors', async () => {
      vi.mocked(tauriCommand).mockRejectedValueOnce(new Error('DB down'));

      const { summaryText, loadSaved } = useSummary();
      await loadSaved();

      expect(summaryText.value).toBeNull();
    });
  });

  describe('generate', () => {
    it('calls generate_summary with citation style, then reloads saved', async () => {
      const resultText = 'Generated summary for the project.';
      const savedAfter = {
        summaryText: 'Generated summary for the project.',
        citationStyle: 'IEEE',
        generatedAt: '2025-06-01T08:00:00Z',
      };

      vi.mocked(tauriCommand).mockResolvedValueOnce(resultText).mockResolvedValueOnce(savedAfter);

      const { summaryText, generatedAt, citationStyle, generate } = useSummary();
      await generate({ style: 'IEEE' });

      expect(tauriCommand).toHaveBeenNthCalledWith(1, 'generate_summary', {
        citationStyle: 'IEEE',
      });
      expect(tauriCommand).toHaveBeenNthCalledWith(2, 'get_saved_summary', {});

      expect(summaryText.value).toBe('Generated summary for the project.');
      expect(citationStyle.value).toBe('IEEE');
      expect(generatedAt.value).toBe('2025-06-01T08:00:00Z');
    });

    it('uses APA as default when no style is passed', async () => {
      vi.mocked(tauriCommand).mockResolvedValueOnce('result').mockResolvedValueOnce(null);

      const { generate } = useSummary();
      await generate();

      expect(tauriCommand).toHaveBeenCalledWith('generate_summary', {
        citationStyle: 'APA',
      });
    });

    it('sets error on backend failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValueOnce(new Error('LLM timeout'));

      const { error, generate } = useSummary();
      await generate({ style: 'MLA' });

      expect(error.value).toBe('LLM timeout');
    });
  });

  describe('clearSummary', () => {
    it('resets all state to defaults', () => {
      const {
        summaryText,
        generatedAt,
        citationStyle,
        additionalInstructions,
        targetWordCount,
        error,
        clearSummary,
      } = useSummary();

      summaryText.value = 'Some text';
      generatedAt.value = '2025-01-01T00:00:00Z';
      citationStyle.value = 'Chicago' as CitationStyle;
      additionalInstructions.value = 'Focus on RCTs.';
      targetWordCount.value = '1200';
      error.value = 'Previous error';

      clearSummary();

      expect(summaryText.value).toBeNull();
      expect(generatedAt.value).toBeNull();
      expect(citationStyle.value).toBe('APA');
      expect(additionalInstructions.value).toBe('');
      expect(targetWordCount.value).toBe('');
      expect(error.value).toBeNull();
    });
  });

  describe('premium guidance extras', () => {
    it('forwards trimmed instructions and floored positive word counts', async () => {
      vi.mocked(tauriCommand).mockResolvedValueOnce('result').mockResolvedValueOnce(null);

      const { generate } = useSummary();
      await generate({
        style: 'MLA',
        additionalInstructions: '  Focus on RCTs.  ',
        targetWordCount: 1200.9,
      });

      expect(tauriCommand).toHaveBeenCalledWith('generate_summary', {
        citationStyle: 'MLA',
        additionalInstructions: 'Focus on RCTs.',
        targetWordCount: 1200,
      });
    });

    it('omits blank instructions and non-positive word counts', async () => {
      vi.mocked(tauriCommand).mockResolvedValueOnce('result').mockResolvedValueOnce(null);

      const { generate } = useSummary();
      await generate({ additionalInstructions: '   ', targetWordCount: 0 });

      expect(tauriCommand).toHaveBeenCalledWith('generate_summary', { citationStyle: 'APA' });
    });

    it('exposes guidance refs with empty-string defaults', () => {
      const { additionalInstructions, targetWordCount } = useSummary();
      expect(additionalInstructions.value).toBe('');
      expect(targetWordCount.value).toBe('');
    });
  });

  describe('formatGeneratedAt', () => {
    it('formats a valid ISO timestamp', () => {
      const { generatedAt, formatGeneratedAt } = useSummary();
      generatedAt.value = '2025-03-15T14:30:00Z';

      const result = formatGeneratedAt();
      expect(result).toBeTruthy();
      expect(typeof result).toBe('string');
    });

    it('returns null when generatedAt is null', () => {
      const { formatGeneratedAt } = useSummary();
      expect(formatGeneratedAt()).toBeNull();
    });
  });
});
