import { ref, type Ref } from 'vue';

/**
 * Generic inline-edit controller for double-click-to-edit text rows.
 *
 * Extracted as pure reactive logic so it can be unit-tested without any DOM or
 * Tauri dependencies, following the project pattern (see `use-nav-history.ts`,
 * `use-startup-upgrade.ts`). The owning view is responsible for:
 *   - rendering the read-only text with `@dblclick="startEdit(item)"`
 *   - rendering the `<input>` when `isEditing(item.id)` is true, bound to
 *     `draftText` and wired to `@keydown.enter`, `@keydown.escape`, `@blur`
 *   - focusing + selecting the input on mount (the view owns the ref because
 *     template-ref wiring is component-local)
 *
 * Standard inline-edit semantics:
 *   - `startEdit(item)` enters edit mode: sets `editingId` and seeds
 *     `draftText` with the item's current text.
 *   - `commitEdit(item)` trims the draft:
 *       - empty  -> calls `deleteItem(item)` (deletes the row)
 *       - unchanged -> exits edit mode without any backend call
 *       - changed -> calls `saveItem(item, trimmed)`.
 *     On success the edit state is cleared. On failure it is left intact so
 *     the user can retry without losing their draft.
 *   - `cancelEdit()` clears the edit state unconditionally and never deletes,
 *     even if the draft is empty.
 *   - Starting an edit on a new item while another is being edited simply
 *     switches the editing target (the previous draft is discarded; this is
 *     intentional - the user explicitly chose to edit something else).
 *
 * @typeParam T - The item type. Must expose an `id: string`.
 */
export interface InlineEditable {
  id: string;
}

export function useInlineEdit<T extends InlineEditable>(opts: {
  /** Persist the edited text. Called only when the draft is non-empty + changed. */
  saveItem: (item: T, newText: string) => Promise<void>;
  /** Delete the item. Called when the committed draft is empty/whitespace. */
  deleteItem: (item: T) => Promise<void>;
  /** Read the item's current text (used to detect "unchanged"). */
  getText: (item: T) => string;
}): {
  editingId: Ref<string | null>;
  draftText: Ref<string>;
  saving: Ref<boolean>;
  /** The id currently being edited, or null. Reactive for template branching. */
  isEditing: (id: string) => boolean;
  /** Enter edit mode for `item`. Seeds the draft with the item's text. */
  startEdit: (item: T) => void;
  /** Commit the draft. See file-level docstring for trim/empty/changed rules. */
  commitEdit: (item: T) => Promise<void>;
  /** Discard the draft and exit edit mode without saving or deleting. */
  cancelEdit: () => void;
} {
  const editingId = ref<string | null>(null);
  const draftText = ref('');
  const saving = ref(false);

  function isEditing(id: string): boolean {
    return editingId.value === id;
  }

  function startEdit(item: T): void {
    editingId.value = item.id;
    draftText.value = opts.getText(item);
  }

  async function commitEdit(item: T): Promise<void> {
    // Guard: only act if this item is the one being edited. The blur handler
    // can fire after the user has already navigated away (e.g. started editing
    // a different row); in that case do nothing.
    if (editingId.value !== item.id) return;
    if (saving.value) return;

    const trimmed = draftText.value.trim();

    // Empty draft -> delete the item.
    if (trimmed === '') {
      saving.value = true;
      try {
        await opts.deleteItem(item);
        editingId.value = null;
        draftText.value = '';
      } finally {
        saving.value = false;
      }
      return;
    }

    // Unchanged -> exit without a backend call.
    if (trimmed === opts.getText(item)) {
      editingId.value = null;
      draftText.value = '';
      return;
    }

    // Changed -> persist.
    saving.value = true;
    try {
      await opts.saveItem(item, trimmed);
      editingId.value = null;
      draftText.value = '';
    } catch {
      // Leave edit state intact so the user can correct + retry.
      // Re-throw so the view can surface a toast if it wants.
      throw new Error('inline-edit save failed');
    } finally {
      saving.value = false;
    }
  }

  function cancelEdit(): void {
    editingId.value = null;
    draftText.value = '';
  }

  return {
    editingId,
    draftText,
    saving,
    isEditing,
    startEdit,
    commitEdit,
    cancelEdit,
  };
}
