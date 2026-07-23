import { describe, it, expect } from 'vitest';
import {
  BIBLIO_RETURN_MAP,
  resolveBiblioReturn,
  resolveCollaboratorAuthor,
} from '@/utils/biblio-links';

describe('biblio-links utils', () => {
  describe('resolveBiblioReturn', () => {
    it('resolves the timeline origin', () => {
      expect(resolveBiblioReturn('timeline')).toEqual({
        name: 'timeline',
        label: 'Back to Timeline',
      });
    });

    it('resolves the authors origin', () => {
      expect(resolveBiblioReturn('authors')).toEqual({
        name: 'authors',
        label: 'Back to Authors',
      });
    });

    it('resolves the coauthors origin', () => {
      expect(resolveBiblioReturn('coauthors')).toEqual({
        name: 'coauthors',
        label: 'Back to Co-Authorship',
      });
    });

    it('returns null for an unknown origin', () => {
      expect(resolveBiblioReturn('keywords')).toBeNull();
      expect(resolveBiblioReturn('nonsense')).toBeNull();
    });

    it('returns null for an absent/empty from value', () => {
      expect(resolveBiblioReturn(undefined)).toBeNull();
      expect(resolveBiblioReturn(null)).toBeNull();
      expect(resolveBiblioReturn('')).toBeNull();
    });

    it('covers every key in BIBLIO_RETURN_MAP (no dead entries)', () => {
      for (const key of Object.keys(BIBLIO_RETURN_MAP)) {
        expect(resolveBiblioReturn(key)).not.toBeNull();
      }
    });
  });

  describe('resolveCollaboratorAuthor', () => {
    const rankings = [
      { displayName: 'Alice Smith' },
      { displayName: 'Bob Jones' },
      { displayName: 'Carol Lee' },
    ];

    it('finds an exact-name match', () => {
      expect(resolveCollaboratorAuthor(rankings, 'Bob Jones')).toEqual({
        displayName: 'Bob Jones',
      });
    });

    it('matches case-insensitively', () => {
      expect(resolveCollaboratorAuthor(rankings, 'alice smith')).toEqual({
        displayName: 'Alice Smith',
      });
      expect(resolveCollaboratorAuthor(rankings, 'CAROL LEE')).toEqual({
        displayName: 'Carol Lee',
      });
    });

    it('returns undefined when no ranking matches', () => {
      expect(resolveCollaboratorAuthor(rankings, 'Dave Wong')).toBeUndefined();
    });

    it('returns undefined for an empty rankings list', () => {
      expect(resolveCollaboratorAuthor([], 'Alice Smith')).toBeUndefined();
    });

    it('returns the first match when duplicates exist', () => {
      const dupes = [
        { displayName: 'Alice Smith', id: '1' },
        { displayName: 'Alice Smith', id: '2' },
      ];
      expect(resolveCollaboratorAuthor(dupes, 'Alice Smith')).toEqual({
        displayName: 'Alice Smith',
        id: '1',
      });
    });
  });
});
