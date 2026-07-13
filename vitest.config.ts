import { defineConfig } from 'vitest/config';
import vue from '@vitejs/plugin-vue';
import { resolve } from 'path';

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  test: {
    environment: 'happy-dom',
    globals: true,
    include: ['src/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'json-summary'],
      include: ['src/**/*.{ts,vue}'],
      exclude: [
        'src/__tests__/**',
        'src/**/*.test.ts',
        'src/**/*.d.ts',
        'src/main.ts',
        'src/vite-env.d.ts',
        'src/workers/**',
        'src/types/**',
      ],
      reportsDirectory: 'coverage',
      thresholds: {
        // Ratcheted upward as coverage improves; target is 70% (see docs/CLAUDE.md).
        statements: 35,
        branches: 21,
        functions: 28,
        lines: 35,
      },
    },
  },
});
