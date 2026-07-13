import { describe, it, expect, beforeEach, vi } from 'vitest';

const mockOpen = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => mockOpen(...args),
}));

import {
  useFullTextAttachment,
  type FullTextAttachmentOptions,
} from '@/composables/use-full-text-attachment';
import { useToast } from '@/composables/use-toast';

describe('useFullTextAttachment', () => {
  let attachFullText: ReturnType<typeof vi.fn>;
  let onAttached: ReturnType<typeof vi.fn>;

  function createOptions(
    overrides: Partial<FullTextAttachmentOptions> = {}
  ): FullTextAttachmentOptions {
    attachFullText = vi.fn().mockResolvedValue(undefined);
    onAttached = vi.fn();
    return {
      attachFullText: attachFullText as unknown as (
        articleId: string,
        filePath: string
      ) => Promise<void>,
      onAttached: onAttached as unknown as (articleId: string) => void,
      ...overrides,
    };
  }

  beforeEach(() => {
    vi.clearAllMocks();
    const { clearHistory, toasts, dismiss } = useToast();
    for (const t of [...toasts.value]) dismiss(t.id);
    clearHistory();
  });

  it('opens file dialog, attaches, shows success toast, calls onAttached', async () => {
    mockOpen.mockResolvedValueOnce('/path/to/test.pdf');
    const opts = createOptions();

    const { handleAttachFullText } = useFullTextAttachment(opts);
    await handleAttachFullText('article-1');

    expect(mockOpen).toHaveBeenCalledWith({
      multiple: false,
      filters: [{ name: 'Documents', extensions: ['pdf', 'txt'] }],
    });
    expect(attachFullText).toHaveBeenCalledWith('article-1', '/path/to/test.pdf');
    expect(onAttached).toHaveBeenCalledWith('article-1');

    const { history } = useToast();
    expect(history.value.some((t) => t.message === 'Full text attached successfully.')).toBe(true);
  });

  it('does nothing when file dialog is cancelled', async () => {
    mockOpen.mockResolvedValueOnce(null);
    const opts = createOptions();

    const { handleAttachFullText } = useFullTextAttachment(opts);
    await handleAttachFullText('article-1');

    expect(attachFullText).not.toHaveBeenCalled();
    expect(onAttached).not.toHaveBeenCalled();
  });

  it('shows error toast when attachFullText throws', async () => {
    mockOpen.mockResolvedValueOnce('/path/to/test.pdf');
    const opts = createOptions();
    attachFullText.mockRejectedValueOnce(new Error('Copy failed'));

    const { handleAttachFullText } = useFullTextAttachment(opts);
    await handleAttachFullText('article-1');

    expect(onAttached).not.toHaveBeenCalled();
    const { history } = useToast();
    expect(
      history.value.some((t) => t.message.includes('Failed to attach full text: Copy failed'))
    ).toBe(true);
  });

  it('handles non-Error throw values in error toast', async () => {
    mockOpen.mockResolvedValueOnce('/path/to/test.pdf');
    const opts = createOptions();
    attachFullText.mockRejectedValueOnce('plain string error');

    const { handleAttachFullText } = useFullTextAttachment(opts);
    await handleAttachFullText('article-1');

    const { history } = useToast();
    expect(
      history.value.some((t) =>
        t.message.includes('Failed to attach full text: plain string error')
      )
    ).toBe(true);
  });

  it('does not crash when onAttached is undefined', async () => {
    mockOpen.mockResolvedValueOnce('/path/to/test.pdf');
    const { attachFullText: af } = createOptions();
    const opts: FullTextAttachmentOptions = { attachFullText: af };

    const { handleAttachFullText } = useFullTextAttachment(opts);
    await handleAttachFullText('article-1');

    expect(af).toHaveBeenCalled();
    const { history } = useToast();
    expect(history.value.some((t) => t.message === 'Full text attached successfully.')).toBe(true);
  });
});
