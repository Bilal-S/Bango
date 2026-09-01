import { describe, it, expect, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import CriteriaEditDialog from '@/components/criteria-edit-dialog.vue';
import SuggestInput from '@/components/suggest-input.vue';
import type { Criterion } from '@/types';

function makeCriterion(
  id: string,
  text: string,
  criterionType: 'inclusion' | 'exclusion'
): Criterion {
  return { id, text, criterionType, priority: 'standard', createdAt: '' };
}

const inclusionCriteria = [
  makeCriterion('c1', 'Must be human study', 'inclusion'),
  makeCriterion('c2', 'Published after 2010', 'inclusion'),
];
const exclusionCriteria = [
  makeCriterion('c3', 'Animal study', 'exclusion'),
  makeCriterion('c4', 'Non-English full text', 'exclusion'),
];

interface MountOptions {
  matchedInclusionIds?: string[];
  matchedExclusionIds?: string[];
}

async function mountDialog(options: MountOptions = {}) {
  // Mount closed, then open through the real prop transition.
  const w = mount(CriteriaEditDialog, {
    props: {
      modelValue: false,
      articleId: 'a1',
      matchedInclusionIds: options.matchedInclusionIds ?? [],
      matchedExclusionIds: options.matchedExclusionIds ?? [],
      inclusionCriteria,
      exclusionCriteria,
    },
    attachTo: document.body,
  });
  await w.setProps({ modelValue: true });
  return w;
}

/** The dialog Teleports to body; query there (repo pattern). */
function bodyText(): string {
  return document.body.textContent ?? '';
}

/** Titles of the pill remove buttons ("Remove {truncated pill text}"). */
function removeTitles(): string[] {
  return Array.from(
    document.body.querySelectorAll<HTMLButtonElement>('button[title^="Remove "]')
  ).map((b) => b.title);
}

function removeButton(title: string): HTMLButtonElement | undefined {
  return Array.from(
    document.body.querySelectorAll<HTMLButtonElement>('button[title^="Remove "]')
  ).find((b) => b.title === title);
}

function findSaveButton(): HTMLButtonElement | undefined {
  return Array.from(document.body.querySelectorAll<HTMLButtonElement>('button')).find(
    (b) => b.textContent?.trim() === 'Save Changes'
  );
}

/** Emit a combobox selection on the nth SuggestInput (0 = inclusion, 1 = exclusion). */
async function pickOption(
  wrapper: Awaited<ReturnType<typeof mountDialog>>,
  index: number,
  id: string,
  label: string
): Promise<void> {
  const inputs = wrapper.findAllComponents(SuggestInput);
  inputs[index]!.vm.$emit('select', label, { id, label });
  await wrapper.vm.$nextTick();
}

/** Option ids offered by the nth SuggestInput. */
function optionIds(wrapper: Awaited<ReturnType<typeof mountDialog>>, index: number): string[] {
  const options = wrapper.findAllComponents(SuggestInput)[index]!.props('options') as {
    id: string;
  }[];
  return options.map((o) => o.id);
}

describe('criteria-edit-dialog.vue', () => {
  let wrapper: Awaited<ReturnType<typeof mountDialog>> | undefined;

  afterEach(() => {
    wrapper?.unmount();
    wrapper = undefined;
    document.body.innerHTML = '';
  });

  it('pre-populates pills: resolved criteria, ghosts, and failed inclusions', async () => {
    wrapper = await mountDialog({
      matchedInclusionIds: ['c1', 'ghost-1'],
      matchedExclusionIds: ['c3', 'c2'],
    });
    // Resolved pills show the criterion text.
    expect(bodyText()).toContain('Must be human study');
    expect(bodyText()).toContain('Animal study');
    // Ghost pill shows the raw value with the unmatched tooltip.
    expect(document.body.querySelector('[title="Unmatched stored entry: ghost-1"]')).toBeTruthy();
    // c2 is an inclusion criterion recorded via the exclusion section: the
    // amber NOT MET pill with the failed-inclusion tooltip.
    expect(
      document.body.querySelector(
        '[title="Failed inclusion criterion (reason for rejection): Published after 2010"]'
      )
    ).toBeTruthy();
    expect(removeTitles()).toContain('Remove Must be human study');
    expect(removeTitles()).toContain('Remove ghost-1');
  });

  it('combobox options exclude criteria already assigned and carry number badges', async () => {
    wrapper = await mountDialog({ matchedInclusionIds: ['c1'], matchedExclusionIds: ['c3'] });
    // Inclusion combobox: only unassigned inclusion criteria.
    expect(optionIds(wrapper, 0)).toEqual(['c2']);
    // Exclusion combobox: violated exclusions first, then NOT MET inclusion picks.
    expect(optionIds(wrapper, 1)).toEqual(['c4', 'c2']);
    const excOptions = wrapper.findAllComponents(SuggestInput)[1]!.props('options') as {
      id: string;
      label: string;
      badge?: string;
    }[];
    const notMet = excOptions.find((o) => o.id === 'c2')!;
    expect(notMet.label).toBe('NOT MET: Published after 2010');
    expect(notMet.badge).toBe('2');
  });

  it('selecting from a section combobox adds the pill to that section only', async () => {
    wrapper = await mountDialog({ matchedInclusionIds: ['c1'] });
    await pickOption(wrapper, 0, 'c2', 'Published after 2010');
    expect(removeTitles()).toContain('Remove Published after 2010');

    findSaveButton()!.click();
    await wrapper.vm.$nextTick();
    const [, inclusionIds, exclusionIds] = wrapper.emitted('save')![0]!;
    expect(inclusionIds).toEqual(['c1', 'c2']);
    expect(exclusionIds).toEqual([]);
  });

  it('picking an inclusion criterion in the exclusion section records it as failed', async () => {
    wrapper = await mountDialog();
    await pickOption(wrapper, 1, 'c2', 'NOT MET: Published after 2010');
    expect(
      document.body.querySelector(
        '[title="Failed inclusion criterion (reason for rejection): Published after 2010"]'
      )
    ).toBeTruthy();

    findSaveButton()!.click();
    await wrapper.vm.$nextTick();
    const [, , exclusionIds] = wrapper.emitted('save')![0]!;
    expect(exclusionIds).toEqual(['c2']);
  });

  it('removing a ghost pill enables Save and drops it while retaining others', async () => {
    wrapper = await mountDialog({
      matchedInclusionIds: ['c1', 'ghost-1', 'ghost-2'],
      matchedExclusionIds: ['c3'],
    });
    // No changes yet -> Save disabled.
    expect(findSaveButton()?.disabled).toBe(true);

    removeButton('Remove ghost-1')!.click();
    await wrapper.vm.$nextTick();
    expect(findSaveButton()?.disabled).toBe(false);

    findSaveButton()!.click();
    await wrapper.vm.$nextTick();
    const [articleId, inclusionIds] = wrapper.emitted('save')![0]!;
    expect(articleId).toBe('a1');
    expect(inclusionIds).toEqual(['c1', 'ghost-2']);
  });

  it('removing a resolved pill drops it from the save payload', async () => {
    wrapper = await mountDialog({ matchedInclusionIds: ['c1'], matchedExclusionIds: ['c3'] });
    removeButton('Remove Animal study')!.click();
    await wrapper.vm.$nextTick();

    findSaveButton()!.click();
    await wrapper.vm.$nextTick();
    const [, inclusionIds, exclusionIds] = wrapper.emitted('save')![0]!;
    expect(inclusionIds).toEqual(['c1']);
    expect(exclusionIds).toEqual([]);
  });
});
