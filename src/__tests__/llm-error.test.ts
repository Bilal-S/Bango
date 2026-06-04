import { describe, it, expect } from 'vitest';
import { formatLlmError } from '@/utils/llm-error';

describe('formatLlmError', () => {
  it('matches 429 rate limit errors', () => {
    const result = formatLlmError('Error 429: rate limit exceeded');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('rate-limited');
    expect(result.solution).toContain('concurrency');
    expect(result.helpLink).toContain('#rate-limited');
  });

  it('matches authentication failed (401)', () => {
    const result = formatLlmError('401 authentication failed');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('auth-failed');
  });

  it('matches forbidden (403)', () => {
    const result = formatLlmError('403 forbidden');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('auth-failed');
  });

  it('matches API_KEY_INVALID with 400', () => {
    const result = formatLlmError('400 API_KEY_INVALID');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('api-key-invalid-400');
  });

  it('matches model not found (404)', () => {
    const result = formatLlmError('404 model not found');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('model-not-found');
  });

  it('matches connection refused', () => {
    const result = formatLlmError('Error: connection refused');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('connection-refused');
  });

  it('matches ECONNREFUSED', () => {
    const result = formatLlmError('fetch failed: ECONNREFUSED');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('connection-refused');
  });

  it('matches token limit errors', () => {
    const result = formatLlmError('token limit exceeded');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('token-limit');
  });

  it('matches malformed JSON errors', () => {
    const result = formatLlmError('malformed response from LLM');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('malformed-json');
  });

  it('matches SSL/TLS errors', () => {
    const result = formatLlmError('SSL certificate verification failed');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('ssl-error');
  });

  it('matches out of memory errors', () => {
    const result = formatLlmError('CUDA out of memory');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('out-of-memory');
  });

  it('matches location not supported', () => {
    const result = formatLlmError('not supported for the API use');
    expect(result.matched).toBe(true);
    expect(result.anchorId).toBe('location-not-supported');
  });

  it('returns unmatched for unknown errors', () => {
    const result = formatLlmError('Something completely unexpected happened');
    expect(result.matched).toBe(false);
    expect(result.anchorId).toBeNull();
    expect(result.solution).toBeNull();
    expect(result.cause).toBeNull();
    expect(result.helpLink).toBe('/#/help?tab=troubleshoot');
  });

  it('always includes original details', () => {
    const msg = 'Error 429: rate limit exceeded';
    const result = formatLlmError(msg);
    expect(result.details).toBe(msg);
  });

  it('matched prefix mentions LLM provider', () => {
    const result = formatLlmError('404 model not found');
    expect(result.prefix).toContain('LLM provider');
    expect(result.prefix).toContain('suggested resolution');
  });

  it('unmatched prefix is shorter', () => {
    const result = formatLlmError('unknown error');
    expect(result.prefix).toContain('LLM provider');
  });

  it('helpLink contains hash anchor for matched errors', () => {
    const result = formatLlmError('rate limited by provider');
    expect(result.helpLink).toMatch(/#\w/);
  });
});
