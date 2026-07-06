<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  SHARE_PLATFORMS,
  composeMessage,
  getPlatformInfo,
  getShareLink,
  getShareUrl,
  type SharePlatformId,
} from '@/utils/share-urls';

const emit = defineEmits<{ close: [] }>();

const selectedPlatformId = ref<SharePlatformId>('x');
const message = ref(composeMessage('x'));
const copied = ref(false);
const opening = ref(false);
const error = ref<string | null>(null);

const platformInfo = computed(() => getPlatformInfo(selectedPlatformId.value));
const shareLink = computed(() => getShareLink(selectedPlatformId.value));
const showInfoNote = computed(() => !platformInfo.value.supportsFullText);

/** Recompute the message when the platform changes (only if the user has not
 *  edited the text yet - once they type, we respect their edits). */
let userEdited = false;
function onMessageInput(): void {
  userEdited = true;
}

watch(selectedPlatformId, (next) => {
  if (!userEdited) {
    message.value = composeMessage(next);
  }
});

async function copyMessage(): Promise<void> {
  try {
    await navigator.clipboard.writeText(message.value);
    copied.value = true;
    setTimeout(() => {
      copied.value = false;
    }, 2000);
  } catch (e) {
    error.value = `Could not copy: ${e instanceof Error ? e.message : String(e)}`;
  }
}

async function openPlatform(): Promise<void> {
  error.value = null;
  opening.value = true;
  try {
    const url = getShareUrl(selectedPlatformId.value, message.value, shareLink.value);
    await openUrl(url);
    emit('close');
  } catch (e) {
    error.value = `Could not open ${platformInfo.value.label}: ${
      e instanceof Error ? e.message : String(e)
    }`;
  } finally {
    opening.value = false;
  }
}
</script>

<template>
  <div class="dialog-overlay" @click.self="emit('close')">
    <div class="dialog share-dialog" tabindex="0" @keydown.escape="emit('close')">
      <button
        class="share-dialog__close"
        type="button"
        title="Close"
        aria-label="Close"
        @click="emit('close')"
      >
        <span class="material-symbols-outlined">close</span>
      </button>
      <h2>Share Bango</h2>
      <div class="simple-info">
        <p>
          Please help spread the word about Bango. Choose a platform or channel you prefer. You can,
          of course, change the message as needed.
        </p>
      </div>
      <div v-if="error" class="share-dialog__error">{{ error }}</div>

      <div class="field">
        <label class="field__label" for="share-platform">Platform</label>
        <div class="field__select-wrapper">
          <select id="share-platform" v-model="selectedPlatformId" class="field__select">
            <option v-for="p in SHARE_PLATFORMS" :key="p.id" :value="p.id">
              {{ p.label }}
            </option>
          </select>
          <span class="material-symbols-outlined field__select-arrow"> expand_more </span>
        </div>
      </div>

      <div class="field">
        <div class="field__header">
          <label class="field__label" for="share-message">Message</label>
          <button
            class="share-dialog__copy-btn"
            type="button"
            :title="copied ? 'Copied!' : 'Copy to clipboard'"
            @click="copyMessage"
          >
            <span class="material-symbols-outlined">
              {{ copied ? 'check' : 'content_copy' }}
            </span>
          </button>
        </div>
        <textarea
          id="share-message"
          v-model="message"
          class="field__input share-dialog__textarea"
          rows="6"
          spellcheck="false"
          @input="onMessageInput"
        />
      </div>

      <div v-if="showInfoNote" class="dialog__info-box">
        <span class="material-symbols-outlined">info</span>
        <p>
          {{ platformInfo.label }} does not support pre-filling the full message body. After
          opening, paste your message (already copied if you used the button above) into the compose
          window.
        </p>
      </div>

      <div class="share-dialog__link-line">
        <span class="material-symbols-outlined share-dialog__link-icon">link</span>
        <code>{{ shareLink }}</code>
      </div>

      <div class="dialog__actions">
        <button class="btn btn--outline" @click="emit('close')">Cancel</button>
        <button class="btn btn--primary" :disabled="opening" @click="openPlatform">
          <span v-if="opening" class="material-symbols-outlined spinner">progress_activity</span>
          {{ opening ? 'Opening...' : `Open ${platformInfo.label}` }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.share-dialog {
  width: 480px;
  max-width: 90vw;
  position: relative;
  padding-right: 2.75rem;
}

.share-dialog:focus {
  outline: none;
}

/* Plain black-text intro paragraph in the standard body font.
 * Deliberately NOT a colored callout - just normal paragraph styling. */
.simple-info {
  color: var(--color-on-surface, #1b1b24);
  font-family: var(--font-family, Inter, system-ui, -apple-system, sans-serif);
  font-size: var(--font-size-body, 14px);
  line-height: var(--line-height-body, 1.5);
}

.simple-info p {
  margin: 0;
  word-break: break-word;
}

/* Close (X) icon button - top-right, mirrors batch-ref-progress__close pattern */
.share-dialog__close {
  position: absolute;
  top: 0.75rem;
  right: 0.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  padding: 0;
  color: var(--color-on-surface-variant, #464555);
  background: none;
  border: none;
  border-radius: var(--radius-sm, 0.25rem);
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    color 0.15s ease;
}

.share-dialog__close:hover {
  background-color: var(--color-surface-container-high, #e4e1ee);
  color: var(--color-on-surface, #1b1b24);
}

.share-dialog__close .material-symbols-outlined {
  font-size: 20px;
}

.share-dialog__error {
  padding: 0.75rem;
  background-color: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: var(--radius-lg, 0.5rem);
  font-size: 13px;
  color: #991b1b;
}

.share-dialog__textarea {
  resize: vertical;
  font-family: inherit;
  min-height: 120px;
}

.share-dialog__copy-btn {
  background: none;
  border: none;
  cursor: pointer;
  color: var(--color-outline, #777587);
  padding: 0.125rem;
  display: inline-flex;
  align-items: center;
  transition: color 0.15s;
}

.share-dialog__copy-btn:hover {
  color: var(--color-on-surface, #1b1b24);
}

.share-dialog__copy-btn .material-symbols-outlined {
  font-size: 18px;
}

.share-dialog__link-line {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  background-color: var(--color-surface-container, #f0ecf9);
  border-radius: var(--radius-lg, 0.5rem);
  font-size: 12px;
  color: var(--color-on-surface-variant, #464555);
}

.share-dialog__link-line code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
}

.share-dialog__link-icon {
  font-size: 16px;
  flex-shrink: 0;
}
</style>
