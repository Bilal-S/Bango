import { describe, it, expect, vi, beforeEach } from 'vitest';
import { formatCitation, firstAuthor, findCitations } from '@/composables/use-citation-finder';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { CitationMatch } from '@/types/citation-finder';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

function makeMatch(overrides: Partial<CitationMatch> = {}): CitationMatch {
  return {
    articleId: 'art-1',
    title: 'Sugar levy effects on childhood obesity',
    authors: ['Smith, J.', 'Jones, A.'],
    publicationYear: 2024,
    journal: 'BMJ Global Health',
    doi: '10.1136/bmjgh-2024-009999',
    matchedPassage: 'The sugar tax reduced obesity significantly.',
    sectionOrigin: 'Results',
    classification: 'validating',
    relevanceExplanation: 'Directly supports the claim.',
    misrepresentsSource: false,
    highlightedSentences: [],
    confidence: 0.92,
    ...overrides,
  };
}

describe('use-citation-finder (pure helpers)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('firstAuthor', () => {
    it('returns the surname from "Last, F." format', () => {
      expect(firstAuthor(makeMatch({ authors: ['Smith, J.'] }))).toBe('Smith');
    });

    it('returns the first whitespace token from "Last First" format', () => {
      expect(firstAuthor(makeMatch({ authors: ['Smith John'] }))).toBe('Smith');
    });

    it('returns "Unknown" when the author list is empty', () => {
      expect(firstAuthor(makeMatch({ authors: [] }))).toBe('Unknown');
    });
  });

  describe('formatCitation', () => {
    // The binding test inventory pins the exact name
    // `formatCitation_outputs_valid_string_per_style` (no suffix), so this one
    // `it` block consolidates all 5 styles. Style-specific edge cases live in
    // the sibling `it` blocks below.
    it('formatCitation_outputs_valid_string_per_style', () => {
      const apa = formatCitation(makeMatch(), 'APA');
      expect(apa.startsWith('(Smith, 2024)')).toBe(true);
      expect(apa).toContain('Smith, J.');
      expect(apa).toContain('Sugar levy effects on childhood obesity');
      expect(apa).toContain('BMJ Global Health');
      expect(apa).toContain('doi:10.1136/bmjgh-2024-009999');

      // MLA / Chicago / AMA: (Smith 2024) — no comma before the year.
      expect(formatCitation(makeMatch(), 'MLA').startsWith('(Smith 2024)')).toBe(true);
      expect(formatCitation(makeMatch(), 'Chicago').startsWith('(Smith 2024)')).toBe(true);
      expect(formatCitation(makeMatch(), 'AMA').startsWith('(Smith 2024)')).toBe(true);

      // IEEE: numeric prefix from the ieeeIndex arg.
      expect(formatCitation(makeMatch(), 'IEEE', 3).startsWith('[3]')).toBe(true);
    });

    it('IEEE falls back to [1] when no index provided', () => {
      const out = formatCitation(makeMatch(), 'IEEE');
      expect(out.startsWith('[1]')).toBe(true);
    });

    it('omits year from prefix when publicationYear is null', () => {
      const out = formatCitation(makeMatch({ publicationYear: null }), 'APA');
      expect(out.startsWith('(Smith)')).toBe(true);
    });

    it('omits doi segment when doi is null', () => {
      const out = formatCitation(makeMatch({ doi: null }), 'APA');
      expect(out).not.toContain('doi:');
    });

    it('omits journal segment when journal is null', () => {
      const out = formatCitation(makeMatch({ journal: null }), 'APA');
      expect(out).not.toContain('BMJ Global Health');
    });
  });

  describe('findCitations IPC wiring', () => {
    it('findCitations_dispatches_command_and_listens_for_done', async () => {
      // The command returns an initial progress snapshot; the assistant bubble
      // arrives via the citation:done event listener (mocked as a no-op
      // unlisten here). We verify the command is invoked with the right
      // payload shape + that send_chat_message / wiki_chat are NOT invoked.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (tauriCommand as any).mockResolvedValue({
        phase: 'searching',
        done: 0,
        total: 0,
        overallPercent: 0,
        message: 'Starting',
        isRunning: true,
        isCancelled: false,
      });

      await findCitations({
        text: 'Sugar taxes reduce obesity.',
        mode: 'whole_block',
        statusFilter: ['working', 'included'],
      });

      expect(tauriCommand).toHaveBeenCalledWith('find_citations', {
        text: 'Sugar taxes reduce obesity.',
        // Snake_case wire token — matches the Rust enum's
        // `#[serde(rename_all = "snake_case")]`.
        mode: 'whole_block',
        statusFilter: ['working', 'included'],
      });
      const calls = (tauriCommand as unknown as { mock: { calls: unknown[][] } }).mock.calls;
      for (const c of calls) {
        const cmd = c[0] as string;
        expect(cmd).not.toBe('send_chat_message');
        expect(cmd).not.toBe('wiki_chat');
      }
    });
  });
});
