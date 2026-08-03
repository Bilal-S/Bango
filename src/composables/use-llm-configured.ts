// src/composables/use-llm-configured.ts
import { computed } from 'vue';
import { useLlmConfigStore } from '@/stores/llm-config';

/**
 * Single canonical "is the LLM configured?" gate for the ENTIRE frontend.
 *
 * Returns a reactive `ComputedRef<boolean>` that mirrors the backend
 * `llm_config_repo::has_config` contract via the Pinia store's `isConfigured`
 * getter: the store must be initialized, `endpointUrl` + `modelName` must be
 * non-empty, and either the provider is local (Ollama / LM Studio /
 * llama.cpp - no key required) or an API key is present.
 *
 * EVERY feature gate that depends on "LLM configured" (Chat, Wiki, Screening,
 * OpenAlex Smart Search, Dashboard CTA, AI Summary, AI buttons in the article
 * detail panel, Search Strategy Builder, etc.) MUST read this composable
 * instead of:
 *   - Holding a local `isLlmConfigured` ref populated by a one-shot
 *     `has_llm_config` IPC call (goes stale on Settings edits).
 *   - Re-deriving the local-provider check from `apiKeyEncrypted`
 *     (incorrectly disables local providers).
 *   - Calling the `has_llm_config` Tauri command directly (the store is the
 *     reactive source of truth; the IPC is a one-shot read).
 *
 * Reactivity: because this delegates to `store.isConfigured` (a Vue `computed`
 * over the store's `config` ref), any mutation of `config.apiKeyEncrypted`,
 * `config.provider`, `config.endpointUrl`, or `config.modelName` - whether
 * from the Settings v-model, the `save()` re-fetch, or `invalidate()` -
 * immediately re-evaluates every consumer.
 *
 * Initialization: `main.ts::bootstrap()` pre-warms the store via
 * `fetchIfNeeded()` on app startup. This composable also calls
 * `fetchIfNeeded()` defensively so it is safe to use in tests or in any
 * mount-order edge case where bootstrap has not yet completed. The call is
 * idempotent (no-op when already initialized).
 *
 * @returns A reactive `ComputedRef<boolean>` - `true` when the LLM is
 *   configured and ready to serve requests, `false` otherwise.
 *
 * @example
 * // In a <script setup> block:
 * const isLlmConfigured = useLlmConfigured();
 * // Then bind in template: v-if="isLlmConfigured"
 */
export function useLlmConfigured() {
  const store = useLlmConfigStore();
  // Belt-and-suspenders: ensure the store is initialized. In the production
  // app this is a no-op because main.ts::bootstrap() pre-warms it, but this
  // covers tests / SSR / any mount-order edge case.
  void store.fetchIfNeeded();
  return computed(() => store.isConfigured);
}
