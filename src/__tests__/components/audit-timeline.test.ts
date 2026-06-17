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

describe('audit-timeline.vue', () => {
  it('shows empty message when no entries', () => {
    const wrapper = mount(AuditTimeline, { props: { entries: [] } });
    expect(wrapper.text()).toContain('No audit entries');
  });

  it('renders the header by default', () => {
    const wrapper = mount(AuditTimeline, { props: { entries: [] } });
    expect(wrapper.text()).toContain('Audit Trail');
  });

  it('hides header when showHeader is false', () => {
    const wrapper = mount(AuditTimeline, {
      props: { entries: [], showHeader: false },
    });
    expect(wrapper.text()).not.toContain('Audit Trail');
  });

  it('renders action label for each entry', () => {
    const wrapper = mount(AuditTimeline, {
      props: {
        entries: [
          makeEntry({ id: 'e1', action: 'import' }),
          makeEntry({ id: 'e2', action: 'status_change' }),
        ],
      },
    });
    expect(wrapper.text()).toContain('Article Imported');
    expect(wrapper.text()).toContain('Status Changed');
  });

  it('renders source attribution', () => {
    const wrapper = mount(AuditTimeline, {
      props: {
        entries: [
          makeEntry({ id: 'e1', source: 'ai' }),
          makeEntry({ id: 'e2', source: 'user' }),
          makeEntry({ id: 'e3', source: 'system' }),
        ],
      },
    });
    expect(wrapper.text()).toContain('by AI');
    expect(wrapper.text()).toContain('by User');
    expect(wrapper.text()).toContain('via System');
  });

  it('renders from/to status transition when present', () => {
    const wrapper = mount(AuditTimeline, {
      props: {
        entries: [makeEntry({ id: 'e1', fromStatus: 'working', toStatus: 'included' })],
      },
    });
    expect(wrapper.text()).toContain('working');
    expect(wrapper.text()).toContain('included');
  });

  it('renders details text stripped of UUIDs', () => {
    const wrapper = mount(AuditTimeline, {
      props: {
        entries: [
          makeEntry({
            id: 'e1',
            details: 'Status changed of article 550e8400-e29b-41d4-a716-446655440000',
          }),
        ],
      },
    });
    expect(wrapper.text()).not.toContain('550e8400');
    expect(wrapper.text()).toContain('Status changed');
  });

  it('renders view link for duplicate references and emits navigateToArticle', async () => {
    const dupUuid = '550e8400-e29b-41d4-a716-446655440000';
    const wrapper = mount(AuditTimeline, {
      props: {
        entries: [
          makeEntry({
            id: 'e1',
            details: `Auto-detected duplicate of article ${dupUuid}`,
          }),
        ],
      },
    });
    const link = wrapper.find('button');
    expect(link.text()).toContain('view');
    await link.trigger('click');
    const events = wrapper.emitted('navigateToArticle');
    expect(events).toBeTruthy();
    expect(events![0]).toEqual([dupUuid]);
  });
});
