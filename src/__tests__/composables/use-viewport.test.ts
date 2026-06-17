import { describe, it, expect, beforeEach } from 'vitest';
import { defineComponent, h } from 'vue';
import { mount } from '@vue/test-utils';
import { useViewport } from '@/composables/use-viewport';

// Wrap useViewport in a real component so onMounted/onUnmounted fire.
const ViewportProbe = defineComponent({
  setup() {
    const vp = useViewport();
    return () => h('div', { 'data-width': vp.width.value, 'data-height': vp.height.value });
  },
});

describe('useViewport', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'innerWidth', {
      writable: true,
      configurable: true,
      value: 1280,
    });
    Object.defineProperty(window, 'innerHeight', {
      writable: true,
      configurable: true,
      value: 800,
    });
  });

  it('isSm is true at 640+', () => {
    const wrapper = mount(ViewportProbe);
    window.innerWidth = 640;
    window.dispatchEvent(new Event('resize'));
    const vp = useViewport();
    expect(vp.width.value).toBe(640);
    expect(vp.isSm.value).toBe(true);
    wrapper.unmount();
  });

  it('isSm is false below 640', () => {
    const wrapper = mount(ViewportProbe);
    window.innerWidth = 500;
    window.dispatchEvent(new Event('resize'));
    const vp = useViewport();
    expect(vp.isSm.value).toBe(false);
    wrapper.unmount();
  });

  it('isMd, isLg, isXl breakpoints', () => {
    const wrapper = mount(ViewportProbe);
    const vp = useViewport();

    window.innerWidth = 768;
    window.dispatchEvent(new Event('resize'));
    expect(vp.isMd.value).toBe(true);
    expect(vp.isLg.value).toBe(false);

    window.innerWidth = 1024;
    window.dispatchEvent(new Event('resize'));
    expect(vp.isLg.value).toBe(true);
    expect(vp.isXl.value).toBe(false);

    window.innerWidth = 1280;
    window.dispatchEvent(new Event('resize'));
    expect(vp.isXl.value).toBe(true);
    wrapper.unmount();
  });

  it('isBelowMd and isBelowLg computed correctly', () => {
    const wrapper = mount(ViewportProbe);
    const vp = useViewport();

    window.innerWidth = 500;
    window.dispatchEvent(new Event('resize'));
    expect(vp.isBelowMd.value).toBe(true);
    expect(vp.isBelowLg.value).toBe(true);

    window.innerWidth = 900;
    window.dispatchEvent(new Event('resize'));
    expect(vp.isBelowMd.value).toBe(false);
    expect(vp.isBelowLg.value).toBe(true);
    wrapper.unmount();
  });

  it('updates height on resize', () => {
    const wrapper = mount(ViewportProbe);
    window.innerHeight = 600;
    window.dispatchEvent(new Event('resize'));
    const vp = useViewport();
    expect(vp.height.value).toBe(600);
    wrapper.unmount();
  });
});
