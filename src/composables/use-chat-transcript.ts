/**
 * Transcript scroll behavior for the Chat view: submit-time scroll-to-bottom
 * and the citation result-arrival anchor (the user's claim message pins to
 * the top of the scroll area so result cards stay visible beneath it, spec
 * §8.7). The scroll container ref is owned here and bound by the view.
 */

import { nextTick, watch, type Ref } from 'vue';
import { scrollAnchorToContainerTop } from '@/utils/chat-scroll';
import type { ChatMessage } from '@/stores/chat';

export function useChatTranscript(args: {
  /** The transcript (from the chat store). */
  messages: Ref<ChatMessage[]>;
  /** The scrollable transcript container element (owned + bound by the view). */
  chatScrollContainer: Ref<HTMLElement | null>;
}) {
  const { chatScrollContainer } = args;

  /** Scroll the transcript to the bottom (after a send). */
  function scrollToBottom(): void {
    void nextTick(() => {
      if (chatScrollContainer.value) {
        chatScrollContainer.value.scrollTop = chatScrollContainer.value.scrollHeight;
      }
    });
  }

  /**
   * When citation results arrive (`citation:done` pushes the assistant
   * bubble), pin the user's claim entry - the user message immediately
   * preceding the citation bubble - to the top of the chat scroll area so
   * the result cards are visible beneath it without manual scrolling. In
   * per-statement mode the first claim group renders directly under the
   * same anchor.
   */
  function scrollClaimEntryToTop(): void {
    void nextTick(() => {
      const container = chatScrollContainer.value;
      if (!container) return;
      const msgs = args.messages.value;
      const last = msgs.length - 1;
      if (last < 0 || !msgs[last]?.citations) return;
      // Anchor on the nearest preceding user message; fall back to the
      // citation bubble itself when no user message precedes it.
      let anchorIdx = last;
      for (let i = last - 1; i >= 0; i -= 1) {
        if (msgs[i]?.role === 'user') {
          anchorIdx = i;
          break;
        }
      }
      const anchor = container.querySelector<HTMLElement>(`[data-msg-idx="${anchorIdx}"]`);
      if (anchor) {
        scrollAnchorToContainerTop(container, anchor);
      }
    });
  }

  /* Fire when the trailing message gains a non-empty citations array - the
   * exact moment citation results land in the transcript (`citation:done`). */
  watch(
    () => args.messages.value[args.messages.value.length - 1]?.citations,
    (citations) => {
      if (citations && citations.length > 0) scrollClaimEntryToTop();
    }
  );

  return {
    scrollToBottom,
    scrollClaimEntryToTop,
  };
}
