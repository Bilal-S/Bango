import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';

import CitationMismatchDialog from '@/components/citation-mismatch-dialog.vue';
import type { CitationFinderProgress, EmbeddingModelMismatch } from '@/types/citation-finder';

const MISMATCH: EmbeddingModelMismatch = {
  currentModel: 'new-model',
  storedModel: 'old-model',
  storedRowCount: 5,
};

function progress(): CitationFinderProgress {
  return {
    phase: 'preparing_embeddings',
    done: 3,
    total: 10,
    overallPercent: 27,
    message: 'Regenerating embeddings… 3/10 articles',
    isRunning: true,
    isCancelled: false,
  };
}

describe('citation-mismatch-dialog', () => {
  it('renders_regeneration_progress_while_running', () => {
    const wrapper = mount(CitationMismatchDialog, {
      props: { mismatch: MISMATCH, regenerating: true, regeneratingProgress: progress() },
      global: { stubs: { Teleport: true } },
    });

    expect(wrapper.text()).toContain('Regenerating embeddings… 3/10 articles');
    expect(wrapper.find('.mismatch-dialog__progress-fill').attributes('style')).toContain(
      'width: 27%'
    );
  });

  it('omits_progress_without_a_payload_and_relabels_the_button', () => {
    const wrapper = mount(CitationMismatchDialog, {
      props: { mismatch: MISMATCH, regenerating: true, regeneratingProgress: null },
      global: { stubs: { Teleport: true } },
    });

    expect(wrapper.find('.mismatch-dialog__progress').exists()).toBe(false);
    expect(wrapper.text()).toContain('Regenerating…');
  });
});
