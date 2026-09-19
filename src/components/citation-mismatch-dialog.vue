<script setup lang="ts">
/**
 * Citation Finder model-mismatch dialog: pops before a search when stored
 * embeddings were generated with a different model than the current
 * `embedding_model` setting (so recall would silently return zero hits).
 * Three options (emitted; the parent owns the follow-up):
 * - `regenerate` -> delete + re-embed the scope (background)
 * - `continue`   -> proceed anyway with partial recall
 * - `cancel`     -> abort, no dismissal recorded
 */
import type { EmbeddingModelMismatch } from '@/types/citation-finder';

defineProps<{
  /** Active mismatch payload. The dialog renders while non-null. */
  mismatch: EmbeddingModelMismatch | null;
  /** True while the Regenerate action is dispatching (disables buttons). */
  regenerating: boolean;
}>();

const emit = defineEmits<{
  regenerate: [];
  continue: [];
  cancel: [];
}>();
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="mismatch"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4"
        @click.self="emit('cancel')"
      >
        <div
          class="mismatch-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="mismatch-title"
        >
          <div class="mismatch-dialog__icon">
            <span class="material-symbols-outlined">sync_problem</span>
          </div>
          <h3 id="mismatch-title" class="mismatch-dialog__title">
            Embeddings were generated with a different model
          </h3>
          <p class="mismatch-dialog__body">
            Your stored embeddings were generated with
            <code>{{ mismatch.storedModel }}</code> but the current embedding model is
            <code>{{ mismatch.currentModel || '(unknown)' }}</code
            >. For consistent results, regenerate your embeddings ({{ mismatch.storedRowCount }}
            row(s) will be re-embedded). Otherwise Citation Finder may silently return zero matches.
          </p>
          <div class="mismatch-dialog__actions">
            <button
              type="button"
              class="mismatch-dialog__btn mismatch-dialog__btn--ghost"
              :disabled="regenerating"
              @click="emit('cancel')"
            >
              Cancel
            </button>
            <button
              type="button"
              class="mismatch-dialog__btn mismatch-dialog__btn--secondary"
              :disabled="regenerating"
              @click="emit('continue')"
            >
              Continue anyway
            </button>
            <button
              type="button"
              class="mismatch-dialog__btn mismatch-dialog__btn--primary"
              :disabled="regenerating"
              @click="emit('regenerate')"
            >
              <span
                v-if="regenerating"
                class="mismatch-dialog__spinner"
                aria-label="Regenerating"
              ></span>
              <span v-else class="material-symbols-outlined text-[16px]">refresh</span>
              {{ regenerating ? 'Starting…' : 'Regenerate' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.mismatch-dialog {
  background: #fff;
  border-radius: 1rem;
  box-shadow: 0 10px 40px rgb(0 0 0 / 0.18);
  padding: 1.5rem;
  max-width: 32rem;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.mismatch-dialog__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.75rem;
  height: 2.75rem;
  border-radius: 9999px;
  background: rgb(254 243 199); /* amber-100 */
  color: rgb(180 83 9); /* amber-700 */
}

.mismatch-dialog__icon span.material-symbols-outlined {
  font-size: 28px;
}

.mismatch-dialog__title {
  font-size: 1rem;
  font-weight: 700;
  color: rgb(15 23 42); /* slate-900 */
  margin: 0;
}

.mismatch-dialog__body {
  font-size: 0.8rem;
  line-height: 1.5;
  color: rgb(71 85 105); /* slate-600 */
  margin: 0;
}

.mismatch-dialog__body code {
  background: rgb(241 245 249);
  padding: 0.0625rem 0.25rem;
  border-radius: 0.1875rem;
  font-family: monospace;
  font-size: 0.85em;
  color: rgb(15 23 42);
}

.mismatch-dialog__actions {
  display: flex;
  gap: 0.5rem;
  justify-content: flex-end;
  margin-top: 0.25rem;
  flex-wrap: wrap;
}

.mismatch-dialog__btn {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.4375rem 0.875rem;
  border-radius: 0.5rem;
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
  transition:
    background-color 0.15s,
    opacity 0.15s;
  border: 1px solid transparent;
}

.mismatch-dialog__btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.mismatch-dialog__btn--ghost {
  background: transparent;
  color: rgb(100 116 139); /* slate-500 */
  border-color: rgb(203 213 225); /* slate-300 */
}

.mismatch-dialog__btn--ghost:hover:not(:disabled) {
  background: rgb(241 245 249); /* slate-100 */
}

.mismatch-dialog__btn--secondary {
  background: #fff;
  color: rgb(71 85 105); /* slate-600 */
  border-color: rgb(203 213 225); /* slate-300 */
}

.mismatch-dialog__btn--secondary:hover:not(:disabled) {
  background: rgb(241 245 249); /* slate-100 */
}

.mismatch-dialog__btn--primary {
  background: rgb(99 102 241); /* indigo-600 */
  color: #fff;
  border-color: rgb(79 70 229); /* indigo-700 */
}

.mismatch-dialog__btn--primary:hover:not(:disabled) {
  background: rgb(79 70 229); /* indigo-700 */
}

.mismatch-dialog__spinner {
  display: inline-block;
  width: 0.875rem;
  height: 0.875rem;
  border: 1.5px solid rgb(255 255 255 / 0.4);
  border-top-color: #fff;
  border-radius: 9999px;
  animation: mismatch-dialog-spin 0.7s linear infinite;
}

@keyframes mismatch-dialog-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
