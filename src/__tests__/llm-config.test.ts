import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useLlmConfigStore } from '@/stores/llm-config';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => false,
  tauriCommand: vi.fn(),
}));

describe('useLlmConfigStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('starts with default config', () => {
    const store = useLlmConfigStore();
    expect(store.config.provider).toBe('openai');
    expect(store.config.temperature).toBe(0.2);
    expect(store.loading).toBe(false);
    expect(store.initialized).toBe(false);
  });

  it('default config has required fields', () => {
    const store = useLlmConfigStore();
    expect(store.config).toHaveProperty('endpointUrl');
    expect(store.config).toHaveProperty('apiKeyEncrypted');
    expect(store.config).toHaveProperty('modelName');
    expect(store.config).toHaveProperty('maxConcurrentRequests');
    expect(store.config).toHaveProperty('requestDelayMs');
    expect(store.config).toHaveProperty('contextWindowTokens');
  });

  it('invalidate resets to defaults', () => {
    const store = useLlmConfigStore();
    store.config.provider = 'anthropic';
    store.config.modelName = 'claude-3';
    store.initialized = true;
    store.invalidate();
    expect(store.config.provider).toBe('openai');
    expect(store.config.modelName).toBe('gpt-5-mini');
    expect(store.initialized).toBe(false);
  });

  it('clearTestResult resets testResult', () => {
    const store = useLlmConfigStore();
    store.testResult = { success: true, message: 'ok' };
    store.clearTestResult();
    expect(store.testResult).toBeNull();
  });

  it('fetchIfNeeded does nothing in non-Tauri mode', async () => {
    const store = useLlmConfigStore();
    await store.fetchIfNeeded();
    expect(store.initialized).toBe(false);
  });

  it('apiKeyEncrypted defaults to null', () => {
    const store = useLlmConfigStore();
    expect(store.config.apiKeyEncrypted).toBeNull();
  });

  it('maxConcurrentRequests has sensible default', () => {
    const store = useLlmConfigStore();
    expect(store.config.maxConcurrentRequests).toBeGreaterThan(0);
  });

  it('temperature is between 0 and 1', () => {
    const store = useLlmConfigStore();
    expect(store.config.temperature).toBeGreaterThanOrEqual(0);
    expect(store.config.temperature).toBeLessThanOrEqual(1);
  });
});
