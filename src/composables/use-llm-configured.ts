// src/composables/use-llm-configured.ts
import { computed } from 'vue';
import { useLlmConfigStore } from '@/stores/llm-config';

/**
 * Single canonical "is the LLM configured?" gate for the ENTIRE frontend.
 *
 * Reactive `ComputedRef<boolean>` that mirrors the backend
 * `llm_config_repo::has_config` contract via the Pinia store's `isConfigured`.
 *
 * EVERY feature gate (Chat, Wiki, Screening, OpenAlex Smart Search, Dashboard
 * CTA, AI Summary, Search Strategy Builder, Citation Finder) MUST read this
 * composable instead of:
 *  - One-shot `has_llm_config` IPC call (goes stale on Settings edits).
 *  - Re-deriving the local-provider check from `apiKeyEncrypted` (wrong).
 *  - Calling `has_llm_config` directly (the store is the reactive source of truth).
 *
 * Reactivity: delegates to `store.isConfigured` (Vue `computed`), so any
 * mutation of API key, provider, endpoint, or model re-evaluates every consumer.
 */
export function useLlmConfigured() {
  const store = useLlmConfigStore();
  /* Belt-and-suspenders: ensure the store is initialized. In production this
  is a no-op (main.ts pre-warms it), but covers tests / SSR edge cases. */
  void store.fetchIfNeeded();
  return computed(() => store.isConfigured);
}
