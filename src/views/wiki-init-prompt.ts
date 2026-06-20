/**
 * Pure decision helpers for the Wiki view, extracted into a standalone module
 * so they can be imported by both the `wiki-view.vue` SFC and unit tests.
 * (Vue's `<script setup>` forbids ES module exports, so testable pure logic
 * lives here rather than in the SFC.)
 */

/** The shape of `WikiStatus` fields the decision depends on. */
export interface InitReadiness {
  includedArticleCount: number;
  initialized: boolean;
}

/**
 * Should the first-visit "initialize wiki?" prompt be shown?
 *
 * True when ALL of:
 * - `status` is loaded (non-null)
 * - an LLM provider is configured (`isLlmConfigured`)
 * - there is at least one included article (`includedArticleCount > 0`)
 * - the wiki has NOT been initialized yet (`initialized === false`)
 *
 * Once the user initializes (or rebuilds), `initialized` flips to true and the
 * prompt stops re-appearing on subsequent visits. If the user dismisses it,
 * it will re-show on the next visit (acceptable, since it disappears once the
 * wiki is built).
 */
export function shouldPromptInit(status: InitReadiness | null, isLlmConfigured: boolean): boolean {
  return !!status && isLlmConfigured && status.includedArticleCount > 0 && !status.initialized;
}
