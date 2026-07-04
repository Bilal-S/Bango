import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import AbstractSummaryView from '@/components/abstract-summary-view.vue';
import { makeArticle } from '../helpers/fixtures';
import type { Article } from '@/types';

// Mock the AI-summary composable. The component imports `parseAiSummary`,
// `requestFigureDescriptions`, and `pendingFigureDescriptions` from it.
vi.mock('@/composables/use-ai-summary', () => ({
  parseAiSummary: vi.fn(() => null),
  requestFigureDescriptions: vi.fn(),
  pendingFigureDescriptions: { value: new Set<string>() },
}));

describe('abstract-summary-view.vue - "Describe Figures & Tables" button gate', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('hides the button when article has no full text', () => {
    const article = makeArticle({ hasFullText: false });
    const wrapper = mount(AbstractSummaryView, {
      props: { article },
    });
    expect(wrapper.text()).not.toContain('Describe Figures & Tables');
    expect(wrapper.text()).not.toContain('Regenerate');
  });

  it('hides the button when full text attached but hasFiguresOrTables is false', () => {
    const article = makeArticle({
      hasFullText: true,
      hasFiguresOrTables: false,
      isTranslated: false,
      translationStatus: 'none',
      translationError: null,
      translatedAt: null,
      abstractText: 'Some abstract.',
    });
    const wrapper = mount(AbstractSummaryView, {
      props: { article },
    });
    expect(wrapper.text()).not.toContain('Describe Figures & Tables');
    expect(wrapper.text()).not.toContain('Regenerate');
  });

  it('shows the button when hasFiguresOrTables is true', async () => {
    // The figures button lives inside the AI Summary tab content block, so the
    // component needs AI summary data to render that tab. Mock parseAiSummary
    // to return a minimal valid blob (no existing figures/tables yet).
    const { parseAiSummary } = await import('@/composables/use-ai-summary');
    vi.mocked(parseAiSummary).mockReturnValueOnce({
      field: 'public_health',
      subfield: 'nutrition',
      summary_150_250_words: 'A summary.',
      key_insights: [],
    } as never);

    const article = makeArticle({
      hasFullText: true,
      hasFiguresOrTables: true,
      abstractText: 'Some abstract.',
    });
    const wrapper = mount(AbstractSummaryView, {
      props: { article },
    });
    expect(wrapper.text()).toContain('Describe Figures & Tables');
  });

  it('shows the button when descriptions already exist even if flag is false (Regenerate)', async () => {
    // Simulate an older row where the flag wasn't computed but the blob already
    // carries figures/tables descriptions. The user must still be able to
    // Regenerate, so the button stays visible.
    const { parseAiSummary } = await import('@/composables/use-ai-summary');
    vi.mocked(parseAiSummary).mockReturnValueOnce({
      field: 'public_health',
      subfield: 'nutrition',
      summary_150_250_words: 'A summary.',
      key_insights: [],
      figures: [{ number: '1', caption: 'Fig 1.', description: 'desc' }],
      tables: [],
    } as never);

    const article: Article = makeArticle({
      hasFullText: true,
      hasFiguresOrTables: false,
      abstractText: 'Some abstract.',
      fullTextAiSummary: '{"figures":[]}',
    });

    const wrapper = mount(AbstractSummaryView, {
      props: { article },
    });

    expect(wrapper.text()).toContain('Regenerate');
  });
});
