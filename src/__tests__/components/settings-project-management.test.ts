import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';

// Mock @tauri-apps/api/core so the component's `get_storage_root` call in
// onMounted resolves instead of hitting the real Tauri bridge.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock useExport so the component does not pull in the full store + plugin
// graph (dialog save, opener, loading overlay, every Pinia store). We only
// need the surface the template touches: error ref + the three action fns.
const mockImportProject = vi.fn();
const mockExportProject = vi.fn();
const mockResetProject = vi.fn();
vi.mock('@/composables/use-export', () => ({
  useExport: () => ({
    error: { value: null },
    exportProject: mockExportProject,
    importProject: mockImportProject,
    resetProject: mockResetProject,
  }),
}));

import SettingsProjectManagement from '@/components/settings/settings-project-management.vue';

/** Mount the settings card with a fresh Pinia and the mocked Tauri bridge.
 * The import dialog is opened by clicking the "Import Backup" button so the
 * file-picker markup is rendered. */
/** Build a minimal memory-history router so `useRouter()` resolves in tests
 * (the component now uses the router to deep-link the Help Reference section). */
function buildTestRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div/>' } },
      { path: '/help', component: { template: '<div/>' } },
    ],
  });
}

async function mountWithImportDialog() {
  setActivePinia(createPinia());
  const wrapper = mount(SettingsProjectManagement, {
    global: { plugins: [createPinia(), buildTestRouter()] },
  });
  await flushPromises();
  const openBtn = wrapper.findAll('button').find((b) => b.text().includes('Import Backup'));
  expect(openBtn).toBeTruthy();
  await openBtn!.trigger('click');
  await flushPromises();
  return wrapper;
}

/** Attach a synthetic `files` array to a real `<input>` element, then dispatch
 * a `change` event. jsdom/happy-dom do not allow constructing a FileList, so
 * we define the property directly on the element before triggering.
 * Returns a spy on the input's `value` setter so tests can assert it was
 * reset to '' (the same-file-twice fix). */
function setFilesAndTrigger(
  inputEl: HTMLInputElement,
  file: File | null
): { valueSetterSpy: ReturnType<typeof vi.spyOn> } {
  // Capture writes to `value` so we can assert the handler reset it to ''
  // (re-selection fix). The setter lives on the prototype; spy there.
  const valueSetterSpy = vi.spyOn(inputEl, 'value', 'set');
  Object.defineProperty(inputEl, 'files', {
    value: file ? [file] : [],
    configurable: true,
  });
  inputEl.dispatchEvent(new Event('change'));
  return { valueSetterSpy };
}

describe('settings-project-management.vue', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockImportProject.mockReset();
    mockExportProject.mockReset();
    mockResetProject.mockReset();
    // Default: get_storage_root resolves (bangoDocsDir shown in export dialog).
    mockInvoke.mockResolvedValue({
      effectivePath: '/home/user/Bango',
      isCustom: false,
      defaultPath: '/home/user/Bango',
    });
  });

  it('renders a single input-shaped field with placeholder text when empty', async () => {
    const wrapper = await mountWithImportDialog();
    // The picker is a single <label> styled like .field__input.
    const picker = wrapper.find('.file-picker');
    expect(picker.exists()).toBe(true);
    expect(picker.element.tagName).toBe('LABEL');
    // Placeholder shown when no file selected.
    expect(wrapper.find('.file-picker__placeholder').text()).toContain('Select a .bango.json');
    // No filename span, no clear button when empty.
    expect(wrapper.find('.file-picker__filename').exists()).toBe(false);
    expect(wrapper.find('.file-picker__clear').exists()).toBe(false);
    // The hidden input is present with the right accept + a11y label.
    const input = wrapper.find('input[type="file"]');
    expect(input.exists()).toBe(true);
    expect(input.attributes('accept')).toBe('.bango.json,.json');
    expect(input.attributes('aria-label')).toBe('Project backup file');
    // Import button starts disabled (no file selected).
    const importBtn = wrapper.findAll('button').find((b) => b.text().trim() === 'Import');
    expect(importBtn?.attributes('disabled')).toBeDefined();
  });

  it('selecting a valid .bango.json file shows the filename and enables Import', async () => {
    const wrapper = await mountWithImportDialog();
    const inputEl = wrapper.find('input[type="file"]').element as HTMLInputElement;
    const file = new File(['{}'], 'my-backup.bango.json', { type: 'application/json' });
    setFilesAndTrigger(inputEl, file);
    await flushPromises();

    expect(wrapper.find('.file-picker__filename').text()).toBe('my-backup.bango.json');
    // Placeholder hidden when a file is selected.
    expect(wrapper.find('.file-picker__placeholder').exists()).toBe(false);
    // Clear button visible.
    expect(wrapper.find('.file-picker__clear').exists()).toBe(true);
    const importBtn = wrapper.findAll('button').find((b) => b.text().trim() === 'Import');
    expect(importBtn?.attributes('disabled')).toBeUndefined();
    // No validation error.
    expect(wrapper.find('.file-picker__error').exists()).toBe(false);
  });

  it('selecting a non-backup file (.pdf) shows an inline error and keeps Import disabled', async () => {
    const wrapper = await mountWithImportDialog();
    const inputEl = wrapper.find('input[type="file"]').element as HTMLInputElement;
    const badFile = new File(['%PDF'], 'not-a-backup.pdf', { type: 'application/pdf' });
    setFilesAndTrigger(inputEl, badFile);
    await flushPromises();

    expect(wrapper.find('.file-picker__error').text()).toContain('.bango.json');
    // Placeholder is back (no valid selection), filename hidden.
    expect(wrapper.find('.file-picker__placeholder').exists()).toBe(true);
    expect(wrapper.find('.file-picker__filename').exists()).toBe(false);
    const importBtn = wrapper.findAll('button').find((b) => b.text().trim() === 'Import');
    expect(importBtn?.attributes('disabled')).toBeDefined();
  });

  it('clicking the ✕ clear button after a valid selection resets the picker', async () => {
    const wrapper = await mountWithImportDialog();
    const inputEl = wrapper.find('input[type="file"]').element as HTMLInputElement;
    const file = new File(['{}'], 'good.json', { type: 'application/json' });
    setFilesAndTrigger(inputEl, file);
    await flushPromises();
    // Precondition: filename shown + Import enabled.
    expect(wrapper.find('.file-picker__filename').exists()).toBe(true);

    const clearBtn = wrapper.find('.file-picker__clear');
    expect(clearBtn.exists()).toBe(true);
    await clearBtn.trigger('click');
    await flushPromises();

    // After clear: placeholder back, filename hidden, clear button gone,
    // Import disabled, error hidden.
    expect(wrapper.find('.file-picker__placeholder').exists()).toBe(true);
    expect(wrapper.find('.file-picker__filename').exists()).toBe(false);
    expect(wrapper.find('.file-picker__clear').exists()).toBe(false);
    const importBtn = wrapper.findAll('button').find((b) => b.text().trim() === 'Import');
    expect(importBtn?.attributes('disabled')).toBeDefined();
    expect(wrapper.find('.file-picker__error').exists()).toBe(false);
  });

  it('clicking ✕ does NOT re-open the OS picker (label forwarding is suppressed)', async () => {
    const wrapper = await mountWithImportDialog();
    const inputEl = wrapper.find('input[type="file"]').element as HTMLInputElement;
    const file = new File(['{}'], 'clear-me.json', { type: 'application/json' });
    setFilesAndTrigger(inputEl, file);
    await flushPromises();
    expect(wrapper.find('.file-picker__filename').exists()).toBe(true);

    // Spy on the hidden input's click - if the label forwarded the ✕ click,
    // the input would be .click()'d (which opens the OS picker).
    const inputClickSpy = vi.spyOn(inputEl, 'click');
    const clearBtn = wrapper.find('.file-picker__clear');
    await clearBtn.trigger('click');
    await flushPromises();

    // The picker must be reset WITHOUT the hidden input being .click()'d.
    expect(wrapper.find('.file-picker__filename').exists()).toBe(false);
    expect(inputClickSpy).not.toHaveBeenCalled();
    inputClickSpy.mockRestore();
  });

  it('resets the input value after a selection so re-selecting the same file re-fires change', async () => {
    const wrapper = await mountWithImportDialog();
    const inputEl = wrapper.find('input[type="file"]').element as HTMLInputElement;
    const file = new File(['{}'], 'repeat.json', { type: 'application/json' });
    const { valueSetterSpy } = setFilesAndTrigger(inputEl, file);
    await flushPromises();

    // The handler must reset the input's value to '' so the browser fires a
    // change event even if the user picks the same file again. This is the
    // latent bug fix (mirrors dashboard.vue's onProjectFileSelected).
    expect(valueSetterSpy).toHaveBeenCalledWith('');
    valueSetterSpy.mockRestore();
  });

  it('accepts a plain .json file (not just .bango.json)', async () => {
    const wrapper = await mountWithImportDialog();
    const inputEl = wrapper.find('input[type="file"]').element as HTMLInputElement;
    const file = new File(['{}'], 'export.json', { type: 'application/json' });
    setFilesAndTrigger(inputEl, file);
    await flushPromises();

    expect(wrapper.find('.file-picker__filename').text()).toBe('export.json');
    const importBtn = wrapper.findAll('button').find((b) => b.text().trim() === 'Import');
    expect(importBtn?.attributes('disabled')).toBeUndefined();
  });
});

describe('settings-project-management.vue - Start New Project button + info-box', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockImportProject.mockReset();
    mockExportProject.mockReset();
    mockResetProject.mockReset();
    mockInvoke.mockResolvedValue({
      effectivePath: '/home/user/Bango',
      isCustom: false,
      defaultPath: '/home/user/Bango',
    });
  });

  it('renders the updated card description mentioning start-fresh', async () => {
    setActivePinia(createPinia());
    const wrapper = mount(SettingsProjectManagement, {
      global: { plugins: [createPinia(), buildTestRouter()] },
    });
    await flushPromises();
    const desc = wrapper.find('.settings-card__desc');
    expect(desc.exists()).toBe(true);
    expect(desc.text()).toContain('Start a new project');
  });

  it('renders the info-box explaining the single-project model', async () => {
    setActivePinia(createPinia());
    const wrapper = mount(SettingsProjectManagement, {
      global: { plugins: [createPinia(), buildTestRouter()] },
    });
    await flushPromises();
    const infoBox = wrapper.find('.settings-card__info-box');
    expect(infoBox.exists()).toBe(true);
    expect(infoBox.text()).toContain('one project at a time');
    expect(infoBox.text()).toContain('Delete All Data');
    // The "Learn more" link is present.
    expect(wrapper.find('.settings-card__learn-more').exists()).toBe(true);
  });

  it('renders the Start New Project primary button', async () => {
    setActivePinia(createPinia());
    const wrapper = mount(SettingsProjectManagement, {
      global: { plugins: [createPinia(), buildTestRouter()] },
    });
    await flushPromises();
    const startBtn = wrapper.findAll('button').find((b) => b.text().includes('Start New Project'));
    expect(startBtn).toBeTruthy();
    // It carries the primary style (visually distinct from secondary actions).
    expect(startBtn?.classes()).toContain('btn--primary');
  });

  it('Start New Project button opens the existing Delete dialog (no separate dialog)', async () => {
    setActivePinia(createPinia());
    const wrapper = mount(SettingsProjectManagement, {
      global: { plugins: [createPinia(), buildTestRouter()] },
    });
    await flushPromises();
    // No dialog initially.
    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);

    const startBtn = wrapper.findAll('button').find((b) => b.text().includes('Start New Project'));
    await startBtn!.trigger('click');
    await flushPromises();

    // The existing Delete All Project Data dialog opens.
    const overlay = wrapper.find('.dialog-overlay');
    expect(overlay.exists()).toBe(true);
    expect(overlay.text()).toContain('Delete All Project Data');
    expect(overlay.find('.dialog--danger').exists()).toBe(true);
  });
});
