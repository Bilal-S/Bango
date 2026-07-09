import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import AuditTimeline from '@/components/audit-timeline.vue';
import type { AuditEntry } from '@/types';

function makeEntry(overrides: Partial<AuditEntry> = {}): AuditEntry {
  return {
    id: 'e1',
    articleId: 'a1',
    timestamp: '2026-01-01T12:00:00Z',
    action: 'import',
    fromStatus: null,
    toStatus: null,
    details: null,
    source: 'system',
    articleTitle: null,
    ...overrides,
  } as AuditEntry;
}

describe('audit-timeline.vue - translation actions (language-plan-v2)', () => {
  it('renders_translation_actions', () => {
    // TC-13: 'translation' and 'translation_error' actions render with the
    // human-readable labels from the actionLabels map (added in Phase 1).
    const wrapper = mount(AuditTimeline, {
      props: {
        entries: [
          makeEntry({ id: 'e1', action: 'translation', source: 'ai' }),
          makeEntry({ id: 'e2', action: 'translation_error', source: 'ai' }),
        ],
      },
    });
    const html = wrapper.html();
    expect(html).toContain('Translation');
    expect(html).toContain('Translation Failed');
  });
});
