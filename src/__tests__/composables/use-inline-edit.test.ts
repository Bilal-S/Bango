import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useInlineEdit } from '@/composables/use-inline-edit';
import type { ResearchAim } from '@/types';

/** Build a controller backed by vi.fn mocks so we can assert call semantics. */
function makeController(initialText = 'original') {
  const saveItem = vi.fn<(item: ResearchAim, newText: string) => Promise<void>>();
  const deleteItem = vi.fn<(item: ResearchAim) => Promise<void>>();
  const getText = vi.fn<(item: ResearchAim) => string>().mockImplementation((i) => i.text);
  const controller = useInlineEdit<ResearchAim>({ saveItem, deleteItem, getText });
  const item: ResearchAim = { id: 'a1', text: initialText, createdAt: '2023-01-01' };
  return { controller, item, saveItem, deleteItem, getText };
}

describe('useInlineEdit', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('starts with no editing id, empty draft, and saving=false', () => {
      const { controller } = makeController();
      expect(controller.editingId.value).toBeNull();
      expect(controller.draftText.value).toBe('');
      expect(controller.saving.value).toBe(false);
    });

    it('isEditing returns false for any id when nothing is being edited', () => {
      const { controller } = makeController();
      expect(controller.isEditing('a1')).toBe(false);
    });
  });

  describe('startEdit', () => {
    it('sets editingId and seeds draftText from getText', () => {
      const { controller, item } = makeController('hello world');
      controller.startEdit(item);
      expect(controller.editingId.value).toBe('a1');
      expect(controller.draftText.value).toBe('hello world');
      expect(controller.isEditing('a1')).toBe(true);
    });

    it('switching to a new item mid-edit replaces the editing target', () => {
      const { controller, item } = makeController();
      const item2: ResearchAim = { id: 'a2', text: 'second', createdAt: '2023-01-02' };
      controller.startEdit(item);
      controller.startEdit(item2);
      expect(controller.editingId.value).toBe('a2');
      expect(controller.draftText.value).toBe('second');
      expect(controller.isEditing('a1')).toBe(false);
      expect(controller.isEditing('a2')).toBe(true);
    });
  });

  describe('commitEdit - changed draft', () => {
    it('calls saveItem with the trimmed new text and clears edit state', async () => {
      const { controller, item, saveItem } = makeController('original');
      controller.startEdit(item);
      controller.draftText.value = '  updated text  ';
      await controller.commitEdit(item);

      expect(saveItem).toHaveBeenCalledTimes(1);
      expect(saveItem).toHaveBeenCalledWith(item, 'updated text');
      expect(controller.editingId.value).toBeNull();
      expect(controller.draftText.value).toBe('');
      expect(controller.saving.value).toBe(false);
    });

    it('does not call saveItem when draft is unchanged (after trim)', async () => {
      const { controller, item, saveItem } = makeController('same text');
      controller.startEdit(item);
      controller.draftText.value = 'same text';
      await controller.commitEdit(item);

      expect(saveItem).not.toHaveBeenCalled();
      expect(controller.editingId.value).toBeNull();
    });

    it('does not call saveItem when draft equals original after whitespace trim', async () => {
      const { controller, item, saveItem } = makeController('same text');
      controller.startEdit(item);
      controller.draftText.value = '   same text   ';
      await controller.commitEdit(item);

      expect(saveItem).not.toHaveBeenCalled();
      expect(controller.editingId.value).toBeNull();
    });

    it('leaves edit state intact when saveItem throws (so user can retry)', async () => {
      const { controller, item, saveItem } = makeController('original');
      saveItem.mockRejectedValueOnce(new Error('boom'));
      controller.startEdit(item);
      controller.draftText.value = 'changed';

      await expect(controller.commitEdit(item)).rejects.toThrow('inline-edit save failed');

      expect(saveItem).toHaveBeenCalledTimes(1);
      // State preserved so the user can retry.
      expect(controller.editingId.value).toBe('a1');
      expect(controller.draftText.value).toBe('changed');
      expect(controller.saving.value).toBe(false);
    });
  });

  describe('commitEdit - empty draft (delete path)', () => {
    it('calls deleteItem when the draft is empty after trim', async () => {
      const { controller, item, saveItem, deleteItem } = makeController('original');
      controller.startEdit(item);
      controller.draftText.value = '     ';
      await controller.commitEdit(item);

      expect(deleteItem).toHaveBeenCalledTimes(1);
      expect(deleteItem).toHaveBeenCalledWith(item);
      expect(saveItem).not.toHaveBeenCalled();
      expect(controller.editingId.value).toBeNull();
      expect(controller.draftText.value).toBe('');
    });

    it('clears edit state after a successful delete', async () => {
      const { controller, item, deleteItem } = makeController('original');
      controller.startEdit(item);
      controller.draftText.value = '';
      await controller.commitEdit(item);

      expect(deleteItem).toHaveBeenCalledTimes(1);
      expect(controller.editingId.value).toBeNull();
      expect(controller.saving.value).toBe(false);
    });
  });

  describe('commitEdit - guard conditions', () => {
    it('is a no-op when the committed item is not the one being edited', async () => {
      const { controller, item, saveItem, deleteItem } = makeController();
      const other: ResearchAim = { id: 'other', text: 'other', createdAt: '' };
      controller.startEdit(item);
      // Simulate blur firing on `other` after the user already moved focus.
      await controller.commitEdit(other);
      expect(saveItem).not.toHaveBeenCalled();
      expect(deleteItem).not.toHaveBeenCalled();
      // The actual editing item is still being edited.
      expect(controller.editingId.value).toBe('a1');
    });

    it('is a no-op while a save is already in flight', async () => {
      const { controller, item, saveItem } = makeController();
      let resolveSave: () => void = () => {};
      saveItem.mockImplementationOnce(
        () => new Promise<void>((resolve) => (resolveSave = resolve))
      );
      controller.startEdit(item);
      controller.draftText.value = 'first';
      const inFlight = controller.commitEdit(item);
      // Second call while the first is unresolved should bail out.
      controller.draftText.value = 'second';
      await controller.commitEdit(item);

      // Only the first call landed.
      expect(saveItem).toHaveBeenCalledTimes(1);
      expect(saveItem).toHaveBeenCalledWith(item, 'first');

      resolveSave();
      await inFlight;
      expect(controller.editingId.value).toBeNull();
    });
  });

  describe('cancelEdit', () => {
    it('clears the edit state without calling save or delete', async () => {
      const { controller, item, saveItem, deleteItem } = makeController('original');
      controller.startEdit(item);
      controller.draftText.value = 'changed but not saved';
      controller.cancelEdit();

      expect(saveItem).not.toHaveBeenCalled();
      expect(deleteItem).not.toHaveBeenCalled();
      expect(controller.editingId.value).toBeNull();
      expect(controller.draftText.value).toBe('');
    });

    it('does NOT delete even when the draft is empty', () => {
      // Escape must always cancel, never delete - the user might have cleared
      // the field by accident and pressed Esc to abort.
      const { controller, item, deleteItem } = makeController('original');
      controller.startEdit(item);
      controller.draftText.value = '';
      controller.cancelEdit();

      expect(deleteItem).not.toHaveBeenCalled();
      expect(controller.editingId.value).toBeNull();
    });

    it('is safe to call when nothing is being edited', () => {
      const { controller } = makeController();
      controller.cancelEdit();
      expect(controller.editingId.value).toBeNull();
    });
  });
});
