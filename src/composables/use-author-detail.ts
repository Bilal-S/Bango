import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { YearCount } from './use-bibliometrics';
import type { AuthorRank } from './use-author-rankings';

/** Institution linked to an author (re-export of BiblioInstitution shape). */
interface AuthorInstitution {
  id: string;
  normalizedName: string;
  country: string | null;
  city: string | null;
  createdAt: string;
}

/** A collaborator of the selected author, with co-authorship strength. */
interface AuthorCollaborator {
  collaboratorId: string;
  collaboratorName: string;
  sharedPapers: number;
}

/** A recent paper by the selected author (for the detail panel list). */
interface AuthorPaper {
  articleId: string;
  title: string;
  publicationYear: number | null;
  journal: string | null;
  numCited: number | null;
  authorOrder: number;
  doi: string | null;
}

/** Full author profile for the detail panel. */
interface AuthorDetail {
  rank: AuthorRank;
  pubsByYear: YearCount[];
  institutions: AuthorInstitution[];
  topCollaborators: AuthorCollaborator[];
  recentPapers: AuthorPaper[];
}

/**
 * Lazy loader for a single author's full profile.
 * Per-call (non-singleton) pattern, mirroring `use-journal-info.ts`:
 * detail data is fetched only when the user clicks an author row.
 */
export function useAuthorDetail() {
  const detail = ref<AuthorDetail | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function getAuthorDetail(authorId: string): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      detail.value = await tauriCommand<AuthorDetail>('biblio_get_author_detail', { authorId });
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      detail.value = null;
    } finally {
      loading.value = false;
    }
  }

  function clear(): void {
    detail.value = null;
    error.value = null;
  }

  return { detail, loading, error, getAuthorDetail, clear };
}
