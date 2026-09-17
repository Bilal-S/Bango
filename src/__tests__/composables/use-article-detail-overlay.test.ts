import { describe, it, expect } from 'vitest';
import { useArticleDetailOverlay } from '@/composables/use-article-detail-overlay';

describe('useArticleDetailOverlay', () => {
  it('opened sets the show flag', () => {
    const overlay = useArticleDetailOverlay();
    expect(overlay.showArticleDetail.value).toBe(false);

    overlay.onArticleDetailOpened();
    expect(overlay.showArticleDetail.value).toBe(true);
  });

  it('closed clears the show flag and resets full-screen', () => {
    const overlay = useArticleDetailOverlay();
    overlay.onArticleDetailOpened();
    overlay.onArticleDetailToggleFullScreen();
    expect(overlay.isArticleDetailFullScreen.value).toBe(true);

    overlay.onArticleDetailClosed();
    expect(overlay.showArticleDetail.value).toBe(false);
    // Closing the slide-over must also exit full-screen so the next open
    // starts collapsed (the network view state contract).
    expect(overlay.isArticleDetailFullScreen.value).toBe(false);
  });

  it('toggle flips full-screen without touching the show flag', () => {
    const overlay = useArticleDetailOverlay();
    overlay.onArticleDetailToggleFullScreen();
    expect(overlay.isArticleDetailFullScreen.value).toBe(true);
    expect(overlay.showArticleDetail.value).toBe(false);

    overlay.onArticleDetailToggleFullScreen();
    expect(overlay.isArticleDetailFullScreen.value).toBe(false);
  });
});
