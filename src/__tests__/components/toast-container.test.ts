import { describe, it, expect, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import ToastContainer from '@/components/toast-container.vue';
import { useToast, type Toast } from '@/composables/use-toast';
import { nextTick } from 'vue';

describe('toast-container.vue', () => {
  beforeEach(() => {
    const { toasts, history, clearHistory } = useToast();
    toasts.value = [];
    history.value = [];
    clearHistory();
  });

  function makeToast(overrides: Partial<Toast> = {}): Toast {
    return {
      id: 1,
      message: 'Test message',
      type: 'info',
      duration: 6000,
      timestamp: Date.now(),
      ...overrides,
    };
  }

  it('renders nothing when there are no toasts', () => {
    mount(ToastContainer);
    // The Teleport renders to body. With no toasts, no toast children exist.
    const toastElements = document.body.querySelectorAll('[class*="px-4 py-2 rounded-lg"]');
    expect(toastElements.length).toBe(0);
  });

  it('renders a toast with the correct message', async () => {
    const { toasts } = useToast();
    toasts.value = [makeToast({ id: 1, message: 'Article saved', type: 'success' })];
    await nextTick();

    mount(ToastContainer);
    await nextTick();
    expect(document.body.textContent).toContain('Article saved');
  });

  it('renders multiple toasts', async () => {
    const { toasts } = useToast();
    toasts.value = [
      makeToast({ id: 1, message: 'Toast 1', type: 'info' }),
      makeToast({ id: 2, message: 'Toast 2', type: 'error' }),
    ];
    await nextTick();

    mount(ToastContainer);
    await nextTick();
    expect(document.body.textContent).toContain('Toast 1');
    expect(document.body.textContent).toContain('Toast 2');
  });

  it('applies success background class', async () => {
    const { toasts } = useToast();
    toasts.value = [makeToast({ id: 1, message: 'OK', type: 'success' })];
    await nextTick();

    mount(ToastContainer);
    await nextTick();
    const toastEl = document.body.querySelector('[class*="bg-green-500"]');
    expect(toastEl).not.toBeNull();
  });

  it('applies error background class', async () => {
    const { toasts } = useToast();
    toasts.value = [makeToast({ id: 1, message: 'Fail', type: 'error' })];
    await nextTick();

    mount(ToastContainer);
    await nextTick();
    const toastEl = document.body.querySelector('[class*="bg-red-500"]');
    expect(toastEl).not.toBeNull();
  });

  it('applies info background class', async () => {
    const { toasts } = useToast();
    toasts.value = [makeToast({ id: 1, message: 'Info', type: 'info' })];
    await nextTick();

    mount(ToastContainer);
    await nextTick();
    const toastEl = document.body.querySelector('[class*="bg-blue-500"]');
    expect(toastEl).not.toBeNull();
  });

  it('applies warning background class', async () => {
    const { toasts } = useToast();
    toasts.value = [makeToast({ id: 1, message: 'Warn', type: 'warning' })];
    await nextTick();

    mount(ToastContainer);
    await nextTick();
    const toastEl = document.body.querySelector('[class*="bg-amber-500"]');
    expect(toastEl).not.toBeNull();
  });

  it('dismiss button calls dismiss', async () => {
    const { toasts } = useToast();
    toasts.value = [makeToast({ id: 42, message: 'Dismiss me' })];
    await nextTick();

    mount(ToastContainer);
    await nextTick();
    const dismissBtn = document.body.querySelector('button');
    expect(dismissBtn).not.toBeNull();
    (dismissBtn as HTMLButtonElement).click();
    await nextTick();

    expect(toasts.value.length).toBe(0);
  });
});
