import { tauriCommand } from './use-tauri-command';

export interface FullTextDeps {
  /** Called after mutation to refresh the article detail. */
  selectArticle: (id: string) => Promise<void>;
  /** Called after mutation to patch the article in the list. */
  syncArticleToList: (id: string) => void;
  /** Called after mutation to refresh counts. */
  fetchCounts: () => Promise<void>;
}

export function useArticleFullText(deps: FullTextDeps) {
  const { selectArticle, syncArticleToList, fetchCounts } = deps;

  async function attachFullText(articleId: string, filePath: string): Promise<void> {
    await tauriCommand('attach_full_text', { articleId, filePath });
    await selectArticle(articleId);
    syncArticleToList(articleId);
    await fetchCounts();
  }

  async function deleteFullTextAttachment(articleId: string): Promise<void> {
    await tauriCommand('delete_full_text', { articleId });
    await selectArticle(articleId);
    syncArticleToList(articleId);
    await fetchCounts();
  }

  async function readFullTextContent(articleId: string): Promise<string | null> {
    return await tauriCommand<string | null>('read_full_text', { articleId });
  }

  async function getFullTextFilePath(articleId: string): Promise<string | null> {
    return await tauriCommand<string | null>('get_full_text_file_path', { articleId });
  }

  return {
    attachFullText,
    deleteFullTextAttachment,
    readFullTextContent,
    getFullTextFilePath,
  };
}
