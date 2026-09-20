import { describe, it, expect } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import ChatWelcomeCards from '@/components/chat-welcome-cards.vue';

type ToggleState = 'enabled' | 'unknown' | 'disabled' | 'hidden';

function mountCards(overrides: { wikiReady?: boolean; citationToggleState?: ToggleState } = {}) {
  return mount(ChatWelcomeCards, {
    props: {
      wikiReady: overrides.wikiReady ?? true,
      citationToggleState: overrides.citationToggleState ?? 'enabled',
    },
  });
}

/** Hint button by card index: article (0), wiki (1), citation (2). */
function hintAt(wrapper: VueWrapper, card: number) {
  return wrapper.findAll('button.chat-welcome-card__hint')[card]!;
}

describe('chat-welcome-cards.vue - clickable hints', () => {
  it('renders every hint line as a button', () => {
    const wrapper = mountCards();
    expect(wrapper.findAll('button.chat-welcome-card__hint').length).toBe(3);
  });

  it('article card hint emits openArticlePicker', async () => {
    const wrapper = mountCards();
    await hintAt(wrapper, 0).trigger('click');
    expect(wrapper.emitted('openArticlePicker')).toHaveLength(1);
  });

  it('wiki card hint emits toggleWiki when the wiki is ready', async () => {
    const wrapper = mountCards({ wikiReady: true });
    await hintAt(wrapper, 1).trigger('click');
    expect(wrapper.emitted('toggleWiki')).toHaveLength(1);
  });

  it('wiki card hint emits openWikiScreen when the wiki is not initialized', async () => {
    const wrapper = mountCards({ wikiReady: false });
    await hintAt(wrapper, 1).trigger('click');
    expect(wrapper.emitted('openWikiScreen')).toHaveLength(1);
    expect(wrapper.emitted('toggleWiki')).toBeUndefined();
  });

  it.each(['enabled', 'unknown'] as const)(
    'citation card hint emits activateCitation when the toggle state is %s',
    async (state) => {
      const wrapper = mountCards({ citationToggleState: state });
      await hintAt(wrapper, 2).trigger('click');
      expect(wrapper.emitted('activateCitation')).toHaveLength(1);
    }
  );

  it.each(['disabled', 'hidden'] as const)(
    'citation card hint emits openSettings when the toggle state is %s',
    async (state) => {
      const wrapper = mountCards({ citationToggleState: state });
      await hintAt(wrapper, 2).trigger('click');
      expect(wrapper.emitted('openSettings')).toHaveLength(1);
      expect(wrapper.emitted('activateCitation')).toBeUndefined();
    }
  );
});
