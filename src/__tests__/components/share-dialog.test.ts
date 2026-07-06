import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';

// Mock the Tauri opener plugin so the dialog never tries to launch a browser.
const openUrlMock = vi.fn();
vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
}));

// Mock clipboard for the copy-button path.
const writeTextMock = vi.fn();
Object.defineProperty(globalThis, 'navigator', {
  value: {
    ...(globalThis.navigator ?? {}),
    platform: 'MacIntel',
    clipboard: { writeText: (...args: unknown[]) => writeTextMock(...args) },
  },
  configurable: true,
  writable: true,
});

import ShareDialog from '@/components/share-dialog.vue';

function mountDialog() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(ShareDialog, { global: { plugins: [pinia] } });
}

describe('share-dialog.vue', () => {
  beforeEach(() => {
    openUrlMock.mockReset();
    writeTextMock.mockReset();
    openUrlMock.mockResolvedValue(undefined);
    writeTextMock.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders all seven platform options in the select', () => {
    const wrapper = mountDialog();
    const options = wrapper.findAll('option');
    expect(options).toHaveLength(7);
    const labels = options.map((o) => o.text());
    expect(labels).toEqual([
      'X (Twitter)',
      'WhatsApp',
      'Telegram',
      'Bluesky',
      'Reddit',
      'LinkedIn',
      'Email',
    ]);
  });

  it('defaults to X and shows no info note', () => {
    const wrapper = mountDialog();
    expect(wrapper.find('.dialog__info-box').exists()).toBe(false);
    // Open button mentions X.
    const openBtn = wrapper.findAll('button').find((b) => b.text().includes('Open'));
    expect(openBtn?.text()).toContain('X (Twitter)');
  });

  it('shows the info note for LinkedIn (no full text support)', async () => {
    const wrapper = mountDialog();
    await wrapper.get('#share-platform').setValue('linkedin');
    expect(wrapper.find('.dialog__info-box').exists()).toBe(true);
    expect(wrapper.text()).toContain('LinkedIn does not support pre-filling');
  });

  it('shows the info note for Reddit (title only)', async () => {
    const wrapper = mountDialog();
    await wrapper.get('#share-platform').setValue('reddit');
    expect(wrapper.find('.dialog__info-box').exists()).toBe(true);
  });

  it('hides the info note for WhatsApp (full text support)', async () => {
    const wrapper = mountDialog();
    await wrapper.get('#share-platform').setValue('whatsapp');
    expect(wrapper.find('.dialog__info-box').exists()).toBe(false);
  });

  it('recomputes the message when platform changes (no user edits yet)', async () => {
    const wrapper = mountDialog();
    // Initial message for X includes the URL inline.
    const initial = (wrapper.get('#share-message').element as HTMLTextAreaElement).value;
    expect(initial).toContain('https://github.com/Bilal-S/Bango');

    // Switch to Telegram: body-only (URL passed separately).
    await wrapper.get('#share-platform').setValue('telegram');
    await flushPromises();
    const after = (wrapper.get('#share-message').element as HTMLTextAreaElement).value;
    expect(after).not.toContain('https://github.com/Bilal-S/Bango');
  });

  it('preserves user edits when platform changes', async () => {
    const wrapper = mountDialog();
    const ta = wrapper.get('#share-message');
    await ta.setValue('my custom edited message');
    await wrapper.get('#share-platform').setValue('whatsapp');
    await flushPromises();
    expect((wrapper.get('#share-message').element as HTMLTextAreaElement).value).toBe(
      'my custom edited message'
    );
  });

  it('copies the message to the clipboard', async () => {
    const wrapper = mountDialog();
    const copyBtn = wrapper
      .findAll('button')
      .find((b) => b.attributes('title')?.includes('Copy to clipboard'));
    expect(copyBtn).toBeTruthy();
    await copyBtn!.trigger('click');
    await flushPromises();
    expect(writeTextMock).toHaveBeenCalledTimes(1);
  });

  it('calls openUrl with the constructed share URL on Open click', async () => {
    const wrapper = mountDialog();
    const openBtn = wrapper.findAll('button').find((b) => b.text().includes('Open'));
    expect(openBtn).toBeTruthy();
    await openBtn!.trigger('click');
    await flushPromises();
    expect(openUrlMock).toHaveBeenCalledTimes(1);
    const url = openUrlMock.mock.calls[0]![0] as string;
    expect(url.startsWith('https://twitter.com/intent/tweet?text=')).toBe(true);
  });

  it('emits close after a successful open', async () => {
    const wrapper = mountDialog();
    const openBtn = wrapper.findAll('button').find((b) => b.text().includes('Open'));
    await openBtn!.trigger('click');
    await flushPromises();
    expect(wrapper.emitted('close')).toBeTruthy();
  });

  it('emits close when backdrop is clicked', async () => {
    const wrapper = mountDialog();
    await wrapper.find('.dialog-overlay').trigger('click');
    expect(wrapper.emitted('close')).toBeTruthy();
  });

  it('emits close on Escape key', async () => {
    const wrapper = mountDialog();
    await wrapper.find('.share-dialog').trigger('keydown', { key: 'Escape' });
    expect(wrapper.emitted('close')).toBeTruthy();
  });

  it('emits close when the close (X) button is clicked', async () => {
    const wrapper = mountDialog();
    const closeBtn = wrapper.find('.share-dialog__close');
    expect(closeBtn.exists()).toBe(true);
    await closeBtn.trigger('click');
    expect(wrapper.emitted('close')).toBeTruthy();
  });

  it('surfaces an error when openUrl rejects', async () => {
    openUrlMock.mockRejectedValueOnce(new Error('blocked by OS'));
    const wrapper = mountDialog();
    const openBtn = wrapper.findAll('button').find((b) => b.text().includes('Open'));
    await openBtn!.trigger('click');
    await flushPromises();
    expect(wrapper.find('.share-dialog__error').exists()).toBe(true);
    expect(wrapper.text()).toContain('blocked by OS');
    // Did NOT close.
    expect(wrapper.emitted('close')).toBeFalsy();
  });
});
