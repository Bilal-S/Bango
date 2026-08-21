import { describe, it, expect, afterEach } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import BulkActionBar from '@/components/bulk-action-bar.vue';

/* Presentational bar: every action is an emit. The More (...) submenu owns
 * its open state locally (network-export-menu pattern) and closes on item
 * pick, anchor re-click, outside click, and Escape. */

let wrapper: VueWrapper | null = null;

function mountBar(props: { selectedCount?: number; llmReady?: boolean } = {}) {
  wrapper = mount(BulkActionBar, {
    props: {
      selectedCount: props.selectedCount ?? 3,
      llmReady: props.llmReady ?? true,
    },
  });
  return wrapper;
}

afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

describe('bulk-action-bar.vue', () => {
  it('hidden_when_nothing_selected', () => {
    const w = mountBar({ selectedCount: 0 });
    expect(w.find('.bulk-bar').exists()).toBe(false);
  });

  it('primary_actions_render_and_export_moved_into_more_menu', () => {
    const w = mountBar();
    const text = w.text();
    for (const label of [
      'Include',
      'Reject',
      'Working',
      'Change Tag',
      'Change Label',
      'Add to Chat',
    ]) {
      expect(text).toContain(label);
    }
    /* Export now lives behind the More submenu - not in the main row. */
    expect(text).not.toContain('Export');
    expect(w.find('button[title="More actions"]').exists()).toBe(true);
  });

  it('primary_buttons_emit_their_actions', async () => {
    const w = mountBar();
    const btn = (label: string) => w.findAll('button').find((b) => b.text().trim() === label)!;
    await btn('Include').trigger('click');
    await btn('Reject').trigger('click');
    await btn('Working').trigger('click');
    await btn('Change Tag').trigger('click');
    await btn('Change Label').trigger('click');
    await btn('Add to Chat').trigger('click');
    expect(w.emitted('bulkInclude')).toEqual([[]]);
    expect(w.emitted('bulkReject')).toEqual([[]]);
    expect(w.emitted('bulkMoveToWorking')).toEqual([[]]);
    expect(w.emitted('bulkAddTag')).toEqual([[]]);
    expect(w.emitted('bulkAddLabel')).toEqual([[]]);
    expect(w.emitted('bulkAddToChat')).toEqual([[]]);
  });

  it('more_menu_opens_on_anchor_click_and_lists_actions', async () => {
    const w = mountBar();
    const menu = () => w.find('[role="menu"]');
    expect(menu().exists()).toBe(false);

    await w.find('button[title="More actions"]').trigger('click');
    expect(menu().exists()).toBe(true);
    expect(w.text()).toContain('Export');
    expect(w.text()).toContain('AI Summary');
    /* Anchor reflects the expanded state. */
    expect(w.find('button[title="More actions"]').attributes('aria-expanded')).toBe('true');
  });

  it('anchor_reclick_toggles_menu_closed', async () => {
    const w = mountBar();
    const anchor = w.find('button[title="More actions"]');
    await anchor.trigger('click');
    await anchor.trigger('click');
    expect(w.find('[role="menu"]').exists()).toBe(false);
  });

  it('export_item_emits_bulk_export_and_closes_menu', async () => {
    const w = mountBar();
    await w.find('button[title="More actions"]').trigger('click');

    await w.findAll('[role="menuitem"]')[0]!.trigger('click');

    expect(w.emitted('bulkExport')).toEqual([[]]);
    expect(w.emitted('bulkAiSummary')).toBeUndefined();
    expect(w.find('[role="menu"]').exists()).toBe(false);
  });

  it('ai_summary_item_emits_bulk_ai_summary_and_closes_menu', async () => {
    const w = mountBar();
    await w.find('button[title="More actions"]').trigger('click');

    await w.findAll('[role="menuitem"]')[1]!.trigger('click');

    expect(w.emitted('bulkAiSummary')).toEqual([[]]);
    expect(w.emitted('bulkExport')).toBeUndefined();
    expect(w.find('[role="menu"]').exists()).toBe(false);
  });

  it('ai_summary_disabled_when_llm_ready_false', async () => {
    const w = mountBar({ llmReady: false });
    await w.find('button[title="More actions"]').trigger('click');

    const items = w.findAll('[role="menuitem"]');
    expect(items[0]!.attributes('disabled')).toBeUndefined();
    expect(items[1]!.attributes('disabled')).toBeDefined();
    expect(items[1]!.attributes('title')).toBe(
      'Configure an LLM provider in Settings to use AI Summary'
    );
  });

  it('outside_click_closes_menu', async () => {
    const w = mountBar();
    await w.find('button[title="More actions"]').trigger('click');
    expect(w.find('[role="menu"]').exists()).toBe(true);

    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await w.vm.$nextTick();
    expect(w.find('[role="menu"]').exists()).toBe(false);
  });

  it('escape_closes_menu', async () => {
    const w = mountBar();
    await w.find('button[title="More actions"]').trigger('click');

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await w.vm.$nextTick();
    expect(w.find('[role="menu"]').exists()).toBe(false);
  });

  it('clear_selection_button_emits_clear_selection', async () => {
    const w = mountBar();
    await w.find('button[title="Clear selection"]').trigger('click');
    expect(w.emitted('clearSelection')).toEqual([[]]);
  });
});
