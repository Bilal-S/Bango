import { mount, type ComponentMountingOptions } from '@vue/test-utils';
import { createPinia, type Pinia } from 'pinia';
import { createRouter, createMemoryHistory, type Router } from 'vue-router';
import type { Component } from 'vue';

/**
 * Shared mount helper for component tests.
 *
 * Provides global mocks for Tauri commands, Pinia, and a stub router so each
 * component test stays concise. Canvas/chart libraries (sigma, apexcharts)
 * should be stubbed per-test via `stubs`.
 */
export function mountComponent(
  component: Component,
  options: ComponentMountingOptions<Record<string, unknown>> = {}
) {
  const pinia = createPinia();
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div/>' } }],
  });

  return mount(component, {
    ...options,
    global: {
      plugins: [pinia, router],
      stubs: {
        ...(options.global?.stubs ?? {}),
      },
      ...(options.global ?? {}),
    },
  });
}

export function freshPinia(): Pinia {
  return createPinia();
}

export function freshRouter(): Router {
  return createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div/>' } }],
  });
}
