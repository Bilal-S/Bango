import { describe, it, expect } from 'vitest';
import { formatAuditAction, formatRelativeTimeParts } from '@/composables/use-dashboard';

describe('formatAuditAction', () => {
  it('returns human-friendly labels for known actions', () => {
    expect(formatAuditAction('import')).toBe('Import');
    expect(formatAuditAction('status_change')).toBe('Status Change');
    expect(formatAuditAction('note_add')).toBe('Note Added');
    expect(formatAuditAction('tag_add')).toBe('Tag Added');
    expect(formatAuditAction('ai_screen')).toBe('AI Screening');
    expect(formatAuditAction('manual_override')).toBe('Manual Override');
    expect(formatAuditAction('translation')).toBe('Translation');
    expect(formatAuditAction('translation_error')).toBe('Translation Failed');
  });

  it('returns the raw action string for unknown actions', () => {
    expect(formatAuditAction('unknown_action')).toBe('unknown_action');
  });

  it('covers all actions in the AuditAction type', () => {
    // Every action in the AuditAction union must have a label so the
    // dashboard never leaks raw snake_case to the user.
    const allActions = [
      'import',
      'dedup_merge',
      'dedup_flag',
      'status_change',
      'note_add',
      'tag_add',
      'tag_remove',
      'label_add',
      'label_remove',
      'criteria_match',
      'ai_screen',
      'manual_override',
      'ai_summary',
      'reference_import',
      'reference_match',
      'error',
      'translation',
      'translation_error',
    ];
    for (const action of allActions) {
      const label = formatAuditAction(action);
      // The label must differ from the raw snake_case action (i.e. it was
      // found in the LABELS map and converted to a human-friendly string).
      expect(label).not.toBe(action);
    }
  });
});

describe('formatRelativeTimeParts', () => {
  it('returns "just" / "now" for timestamps less than 60 seconds ago', () => {
    const ts = new Date(Date.now() - 30_000).toISOString();
    const parts = formatRelativeTimeParts(ts);
    expect(parts.value).toBe('just');
    expect(parts.suffix).toBe('now');
  });

  it('returns minutes with "ago" suffix', () => {
    const ts = new Date(Date.now() - 36 * 60_000).toISOString();
    const parts = formatRelativeTimeParts(ts);
    expect(parts.value).toBe('36m');
    expect(parts.suffix).toBe('ago');
  });

  it('returns hours with "ago" suffix', () => {
    const ts = new Date(Date.now() - 2 * 3_600_000).toISOString();
    const parts = formatRelativeTimeParts(ts);
    expect(parts.value).toBe('2h');
    expect(parts.suffix).toBe('ago');
  });

  it('returns days with "ago" suffix', () => {
    const ts = new Date(Date.now() - 3 * 86_400_000).toISOString();
    const parts = formatRelativeTimeParts(ts);
    expect(parts.value).toBe('3d');
    expect(parts.suffix).toBe('ago');
  });
});
