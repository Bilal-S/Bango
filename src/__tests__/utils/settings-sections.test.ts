import { describe, it, expect } from 'vitest';
import {
  SETTINGS_SECTIONS,
  activeSectionFromTops,
  resolveSettingsSectionId,
} from '@/utils/settings-sections';

describe('settings-sections', () => {
  it('declares the canonical settings order and one-word labels', () => {
    expect(SETTINGS_SECTIONS.map((section) => section.id)).toEqual([
      'settings-ai',
      'settings-embeddings',
      'settings-project-management',
      'settings-summaries',
      'settings-screening',
      'settings-search',
      'settings-storage',
      'settings-batch',
      'settings-history',
      'settings-diagnostics',
    ]);
    expect(SETTINGS_SECTIONS.map((section) => section.label)).toEqual([
      'AI',
      'Embeddings',
      'Project',
      'Summaries',
      'Screening',
      'Search',
      'Storage',
      'Batch',
      'History',
      'Diagnostics',
    ]);
  });

  it('resolves known ids and the legacy project-management focus value', () => {
    expect(resolveSettingsSectionId('settings-batch')).toBe('settings-batch');
    expect(resolveSettingsSectionId('project-management')).toBe('settings-project-management');
  });

  it('rejects unknown or non-string focus values', () => {
    expect(resolveSettingsSectionId('nope')).toBeNull();
    expect(resolveSettingsSectionId('')).toBeNull();
    expect(resolveSettingsSectionId(undefined)).toBeNull();
    expect(resolveSettingsSectionId(['settings-ai'])).toBeNull();
  });

  it('picks the last section at or above the trigger line', () => {
    // Empty / all below / all above fall back to the first section.
    expect(activeSectionFromTops([])).toBe('settings-ai');
    expect(activeSectionFromTops([200, 300])).toBe('settings-ai');
    expect(activeSectionFromTops([0, 0, 0])).toBe('settings-project-management');
    // Third section is the last one with top <= 120.
    expect(activeSectionFromTops([-500, -200, 100, 150, 400])).toBe('settings-project-management');
  });
});
