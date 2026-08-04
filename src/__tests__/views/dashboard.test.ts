import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';

// Mock vue-router so router.push calls are captured without real navigation.
const mockPush = vi.fn();
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: mockPush }),
}));

// Mock useDashboard so we can drive hasArticles, cta, etc. without the full
// store graph. We only need the surface the template touches.
vi.mock('@/composables/use-dashboard', () => ({
  useDashboard: () => ({
    counts: { value: { total: 5, duplicate: 0, working: 3, included: 1, rejected: 1 } },
    totalNonDuplicate: { value: 5 },
    screenedByAi: { value: 2 },
    screenedByUser: { value: 1 },
    groupedAudit: { value: [] },
    loading: { value: false },
    loadingMoreActivities: { value: false },
    hasMoreActivities: { value: false },
    error: { value: null },
    hasArticles: { value: true },
    screeningPercentage: { value: 60 },
    cta: {
      value: {
        icon: 'play_arrow',
        label: 'Start AI Screening',
        route: '/screening',
        state: 'start_screening',
      },
    },
    refresh: vi.fn(),
    loadMoreActivities: vi.fn(),
  }),
  formatAuditAction: (action: string) => action,
  formatRelativeTimeParts: (_ts: string) => ({ value: '1m', suffix: 'ago' }),
}));

// Mock useDemo + useExport so the component does not pull in the Tauri bridge.
vi.mock('@/composables/use-demo', () => ({
  useDemo: () => ({
    demoLoading: { value: false },
    demoError: { value: null },
    loadDemo: vi.fn(),
  }),
}));

const mockImportProject = vi.fn();
vi.mock('@/composables/use-export', () => ({
  useExport: () => ({
    error: { value: null },
    importProject: mockImportProject,
  }),
}));

import Dashboard from '@/views/dashboard.vue';

describe('dashboard.vue - Start New Project dialog', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    mockPush.mockReset();
  });

  /** Mount the dashboard with hasArticles=true (project loaded). */
  async function mountWithProject() {
    const wrapper = mount(Dashboard, {
      global: {
        plugins: [createPinia()],
      },
    });
    await flushPromises();
    return wrapper;
  }

  it('renders the Start New Project link in the header when a project is loaded', async () => {
    const wrapper = await mountWithProject();
    const link = wrapper.find('.dashboard__start-new-link');
    expect(link.exists()).toBe(true);
    expect(link.text()).toContain('Start New Project');
  });

  it('opens the Start New Project info dialog on click', async () => {
    const wrapper = await mountWithProject();
    // Dialog is hidden initially.
    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);

    await wrapper.find('.dashboard__start-new-link').trigger('click');
    await flushPromises();

    // Dialog now rendered.
    const overlay = wrapper.find('.dialog-overlay');
    expect(overlay.exists()).toBe(true);
    // Title + 3-step workflow present.
    expect(overlay.text()).toContain('Start a New Project');
    expect(overlay.text()).toContain('one project at a time');
    const steps = overlay.findAll('.dashboard__start-new-steps li');
    expect(steps).toHaveLength(3);
    expect(steps[0]?.text()).toContain('Back up');
    expect(steps[1]?.text()).toContain('Delete');
    expect(steps[2]?.text()).toContain('Begin fresh');
  });

  it('renders both navigation buttons in the dialog', async () => {
    const wrapper = await mountWithProject();
    await wrapper.find('.dashboard__start-new-link').trigger('click');
    await flushPromises();

    const buttons = wrapper.findAll('.dialog-overlay button');
    const labels = buttons.map((b) => b.text());
    expect(labels.some((t) => t.includes('Cancel'))).toBe(true);
    expect(labels.some((t) => t.includes('Open Help Guide'))).toBe(true);
    expect(labels.some((t) => t.includes('Go to Project Management'))).toBe(true);
  });

  it('Go to Project Management closes the dialog and navigates to /settings', async () => {
    const wrapper = await mountWithProject();
    await wrapper.find('.dashboard__start-new-link').trigger('click');
    await flushPromises();
    expect(wrapper.find('.dialog-overlay').exists()).toBe(true);

    const goBtn = wrapper
      .findAll('.dialog-overlay button')
      .find((b) => b.text().includes('Go to Project Management'));
    expect(goBtn).toBeTruthy();
    await goBtn!.trigger('click');
    await flushPromises();

    // Dialog closed.
    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);
    // Navigated to settings with the focus query param so the Settings view
    // scrolls to the Project Management card.
    expect(mockPush).toHaveBeenCalledWith('/settings?focus=project-management');
  });

  it('Open Help Guide closes the dialog and navigates to the starting-points anchor', async () => {
    const wrapper = await mountWithProject();
    await wrapper.find('.dashboard__start-new-link').trigger('click');
    await flushPromises();

    const helpBtn = wrapper
      .findAll('.dialog-overlay button')
      .find((b) => b.text().includes('Open Help Guide'));
    expect(helpBtn).toBeTruthy();
    await helpBtn!.trigger('click');
    await flushPromises();

    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);
    expect(mockPush).toHaveBeenCalledWith('/help?tab=guide#starting-points');
  });

  it('Cancel button closes the dialog without navigating', async () => {
    const wrapper = await mountWithProject();
    await wrapper.find('.dashboard__start-new-link').trigger('click');
    await flushPromises();

    const cancelBtn = wrapper
      .findAll('.dialog-overlay button')
      .find((b) => b.text().trim() === 'Cancel');
    expect(cancelBtn).toBeTruthy();
    await cancelBtn!.trigger('click');
    await flushPromises();

    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);
    expect(mockPush).not.toHaveBeenCalled();
  });
});
