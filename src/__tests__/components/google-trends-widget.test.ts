import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => ({ ok: true, statusCode: 200, reason: 'ok' })),
}));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(async () => undefined) }));

import { invoke } from '@tauri-apps/api/core';
import GoogleTrendsWidget from '@/components/google-trends-widget.vue';

type Props = InstanceType<typeof GoogleTrendsWidget>['$props'];

function baseProps(over: Partial<Props> = {}): Props {
  return {
    type: 'TIMESERIES',
    keywords: ['sugar tax'],
    range: { apiTime: 'today 5-y', queryDate: '2026-09-01' },
    revision: 1,
    readyToRender: true,
    rateLimited: false,
    ...over,
  } as Props;
}

/** Flush pending microtasks + due zero-delay timers under fake timers. */
async function flush() {
  await vi.advanceTimersByTimeAsync(0);
  await Promise.resolve();
  await Promise.resolve();
}

/** Number of `check_trends_url` preflight probes issued so far. */
function probes(): number {
  return vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'check_trends_url').length;
}

describe('google-trends-widget.vue', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockClear();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders_iframe_after_ok_preflight', async () => {
    const wrapper = mount(GoogleTrendsWidget, { props: baseProps() });
    await flush();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith(
      'check_trends_url',
      expect.objectContaining({ url: expect.any(String) })
    );
    expect(wrapper.find('iframe').exists()).toBe(true);
  });

  it('prop_changes_are_debounced_into_one_refresh', async () => {
    const wrapper = mount(GoogleTrendsWidget, { props: baseProps() });
    await flush();
    const afterMount = probes();

    /* A prop change alone must not re-probe immediately - the refresh is
     * debounced (250ms) so the parent's chart-then-map serialization and any
     * rapid churn coalesce into a single iframe update. */
    await wrapper.setProps({ keywords: ['obesity policy'] });
    await flush();
    expect(probes()).toBe(afterMount);

    /* A second rapid change within the window coalesces with the first. */
    await wrapper.setProps({ revision: 2 });
    await flush();
    vi.advanceTimersByTime(250);
    await flush();
    expect(probes()).toBe(afterMount + 1);
    expect(wrapper.find('iframe').exists()).toBe(true);
  });

  it('no_spurious_refresh_without_prop_change', async () => {
    mount(GoogleTrendsWidget, { props: baseProps() });
    await flush();
    const before = probes();
    vi.advanceTimersByTime(2_000);
    await flush();
    expect(probes()).toBe(before);
  });

  it('rate_limited_shows_429_overlay_without_new_probe', async () => {
    const wrapper = mount(GoogleTrendsWidget, { props: baseProps() });
    await flush();
    vi.mocked(invoke).mockClear();

    await wrapper.setProps({ rateLimited: true });
    vi.advanceTimersByTime(250);
    await flush();

    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(0);
    expect(wrapper.text()).toContain('Rate limit reached');
    expect(wrapper.find('iframe').exists()).toBe(false);
  });
});
