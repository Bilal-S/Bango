/// <reference types="vite/client" />

declare const __APP_VERSION__: string;

declare module '*.css?raw' {
  const css: string;
  export default css;
}

declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent<object, object, unknown>;
  export default component;
}
