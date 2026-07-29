import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import AbstractSummaryView from '@/components/abstract-summary-view.vue';
import { makeArticle } from '../helpers/fixtures';
import type { Article } from '@/types';

// Mock the AI-summary composable. The component imports `parseAiSummary`,
// `requestArticleAiSummary`, `pendingSummaries`, `requestFigureDescriptions`,
// and `pendingFigureDescriptions` from it.
vi.mock('@/composables/use-ai-summary', () => ({
  parseAiSummary: vi.fn(() => null),
  requestArticleAiSummary: vi.fn(),
  pendingSummaries: { value: new Set<string>() },
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

describe('abstract-summary-view.vue - "Regenerate Summary" button', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('hides the Regenerate Summary button when no AI summary exists', () => {
    // No parseAiSummary mock override -> default returns null -> the entire AI
    // Summary tab (including the Regenerate button) is not rendered.
    const article = makeArticle({
      hasFullText: true,
      abstractText: 'Some abstract.',
    });

    const wrapper = mount(AbstractSummaryView, {
      props: { article },
    });

    // The Regenerate button lives only inside the AI Summary content block, so
    // it must be absent here. Use a selector scoped to the button's title to
    // avoid matching the figures "Regenerate" label which is also absent here.
    expect(
      wrapper.find('button[title="Regenerate the AI summary from the full text"]').exists()
    ).toBe(false);
    // Confirm the article rendered at all (Abstract tab).
    expect(wrapper.text()).toContain('Some abstract.');
  });

  it('shows the Regenerate Summary button when an AI summary exists', async () => {
    const { parseAiSummary } = await import('@/composables/use-ai-summary');
    vi.mocked(parseAiSummary).mockReturnValueOnce({
      field: 'public_health',
      subfield: 'nutrition',
      summary_150_250_words: 'A summary.',
      key_insights: [],
    } as never);

    const article = makeArticle({
      hasFullText: true,
      abstractText: 'Some abstract.',
      title: 'Test Article',
    });

    const wrapper = mount(AbstractSummaryView, {
      props: { article },
    });

    const btn = wrapper.find('button[title="Regenerate the AI summary from the full text"]');
    // @vue/test-utils `find` always returns a wrapper (use exists()).
    expect(btn.exists()).toBe(true);
    expect(btn.text()).toContain('Regenerate');
  });

  it('invokes requestArticleAiSummary with the article id on click', async () => {
    const { parseAiSummary, requestArticleAiSummary } =
      await import('@/composables/use-ai-summary');
    vi.mocked(parseAiSummary).mockReturnValueOnce({
      field: 'public_health',
      subfield: 'nutrition',
      summary_150_250_words: 'A summary.',
      key_insights: [],
    } as never);

    const article = makeArticle({
      id: 'art-123',
      hasFullText: true,
      abstractText: 'Some abstract.',
      title: 'Test Article',
    });

    const wrapper = mount(AbstractSummaryView, {
      props: { article },
    });

    const btn = wrapper.find('button[title="Regenerate the AI summary from the full text"]');
    expect(btn.exists()).toBe(true);
    await btn.trigger('click');

    expect(requestArticleAiSummary).toHaveBeenCalledTimes(1);
    // The composable signature is (articleId, articleTitle, onComplete?, includeSections?).
    expect(requestArticleAiSummary).toHaveBeenCalledWith(
      'art-123',
      'Test Article',
      expect.any(Function)
    );
  });

  it('disables the button while a summary is pending and shows a spinner', async () => {
    const { parseAiSummary, pendingSummaries } = await import('@/composables/use-ai-summary');
    vi.mocked(parseAiSummary).mockReturnValueOnce({
      field: 'public_health',
      subfield: 'nutrition',
      summary_150_250_words: 'A summary.',
      key_insights: [],
    } as never);

    // Simulate the pending state for this article.
    const articleId = 'art-pending';
    pendingSummaries.value = new Set<string>([articleId]);

    const article = makeArticle({
      id: articleId,
      hasFullText: true,
      abstractText: 'Some abstract.',
    });

    const wrapper = mount(AbstractSummaryView, {
      props: { article },
    });

    const btn = wrapper.find('button[title="Regenerate the AI summary from the full text"]');
    expect(btn.exists()).toBe(true);
    expect(btn.attributes('disabled')).toBeDefined();
    // Spinner icon renders instead of the refresh icon while pending.
    expect(btn.text()).toContain('progress_activity');

    // Cleanup: reset the shared singleton so other tests start clean.
    pendingSummaries.value = new Set<string>();
  });
});
