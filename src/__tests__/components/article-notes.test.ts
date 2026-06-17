import { describe, it, expect, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import ArticleNotes from '@/components/article-notes.vue';
import { makeArticle, shimLocalStorage } from '../helpers/fixtures';

describe('article-notes.vue', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
  });

  it('renders Notes header and edit button', () => {
    const wrapper = mount(ArticleNotes, { props: { article: makeArticle() } });
    expect(wrapper.text()).toContain('Notes');
    expect(wrapper.text()).toContain('edit');
  });

  it('shows empty placeholder when no user notes', () => {
    const wrapper = mount(ArticleNotes, { props: { article: makeArticle({ userNotes: null }) } });
    expect(wrapper.text()).toContain('No notes yet');
  });

  it('shows existing user notes when not editing', () => {
    const wrapper = mount(ArticleNotes, {
      props: { article: makeArticle({ userNotes: 'Important finding.' }) },
    });
    expect(wrapper.text()).toContain('Important finding.');
  });

  it('enters edit mode and shows textarea when edit clicked', async () => {
    const wrapper = mount(ArticleNotes, { props: { article: makeArticle() } });
    await wrapper.find('button').trigger('click');
    expect(wrapper.find('textarea').exists()).toBe(true);
    expect(wrapper.text()).toContain('Save');
    expect(wrapper.text()).toContain('Cancel');
  });

  it('emits updateNotes with id and draft on save', async () => {
    const wrapper = mount(ArticleNotes, { props: { article: makeArticle({ id: 'xyz' }) } });
    await wrapper.find('button').trigger('click');
    const textarea = wrapper.find('textarea');
    await textarea.setValue('New note text');
    const buttons = wrapper.findAll('button');
    const saveBtn = buttons.find((b) => b.text() === 'Save')!;
    await saveBtn.trigger('click');
    const events = wrapper.emitted('updateNotes');
    expect(events).toBeTruthy();
    expect(events![0]).toEqual(['xyz', 'New note text']);
  });

  it('cancel exits edit mode without emitting', async () => {
    const wrapper = mount(ArticleNotes, { props: { article: makeArticle() } });
    await wrapper.find('button').trigger('click');
    const cancelBtn = wrapper.findAll('button').find((b) => b.text() === 'Cancel')!;
    await cancelBtn.trigger('click');
    expect(wrapper.find('textarea').exists()).toBe(false);
    expect(wrapper.emitted('updateNotes')).toBeFalsy();
  });

  it('shows imported notes section when present', () => {
    const wrapper = mount(ArticleNotes, {
      props: { article: makeArticle({ notes: 'Author note from RIS' }) },
    });
    expect(wrapper.text()).toContain('Imported Notes');
    expect(wrapper.text()).toContain('Author note from RIS');
  });

  it('hides imported notes section when absent', () => {
    const wrapper = mount(ArticleNotes, {
      props: { article: makeArticle({ notes: null }) },
    });
    expect(wrapper.text()).not.toContain('Imported Notes');
  });

  it('toggles imported notes expanded state', async () => {
    const wrapper = mount(ArticleNotes, {
      props: { article: makeArticle({ notes: 'hidden note' }) },
    });
    const toggle = wrapper.find('button');
    await toggle.trigger('click');
    expect(localStorage.getItem('bango-imported-notes-expanded')).toBe('true');
  });

  it('prepopulates draft with existing notes when editing', async () => {
    const wrapper = mount(ArticleNotes, {
      props: { article: makeArticle({ userNotes: 'existing' }) },
    });
    await wrapper.find('button').trigger('click');
    expect(wrapper.find('textarea').element.value).toBe('existing');
  });
});
