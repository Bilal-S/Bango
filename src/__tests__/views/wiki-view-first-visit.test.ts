import { describe, it, expect } from 'vitest';
// `shouldPromptInit` is a pure decision helper in a standalone module
// (extracted out of the SFC because `<script setup>` forbids exports).
import { shouldPromptInit } from '@/views/wiki-init-prompt';

describe('shouldPromptInit (wiki first-visit prompt decision)', () => {
  const ready = { includedArticleCount: 5, initialized: false };
  const initialized = { includedArticleCount: 5, initialized: true };
  const noArticles = { includedArticleCount: 0, initialized: false };

  it('returns true when LLM configured + included articles + not initialized', () => {
    expect(shouldPromptInit(ready, true)).toBe(true);
  });

  it('returns false when LLM is not configured', () => {
    expect(shouldPromptInit(ready, false)).toBe(false);
  });

  it('returns false when there are no included articles', () => {
    expect(shouldPromptInit(noArticles, true)).toBe(false);
  });

  it('returns false when the wiki is already initialized', () => {
    // Once initialized, the prompt must not re-show on every visit.
    expect(shouldPromptInit(initialized, true)).toBe(false);
  });

  it('returns false when status is null (still loading)', () => {
    expect(shouldPromptInit(null, true)).toBe(false);
  });
});
