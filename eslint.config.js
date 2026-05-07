import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import pluginVue from 'eslint-plugin-vue';
import prettierConfig from 'eslint-config-prettier';
import globals from 'globals';

export default tseslint.config(
  { ignores: ['dist/**', 'src-tauri/**', 'node_modules/**'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs['flat/recommended'],
  {
    files: ['*.vue', '**/*.vue'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
      globals: {
        ...globals.browser,
        __APP_VERSION__: 'readonly',
      },
    },
    rules: {
      // Disable TS no-unused-vars for Vue files since it doesn't understand template bindings.
      // The Vue plugin's own rule handles this correctly.
      '@typescript-eslint/no-unused-vars': 'off',
    },
  },
  {
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
      'vue/component-name-in-template-casing': ['error', 'PascalCase'],
      'vue/multi-word-component-names': 'off',
      'no-console': ['warn', { allow: ['warn', 'error'] }],
    },
  },
  // Re-disable no-unused-vars for Vue files — must come AFTER the general
  // rule above so it takes precedence. <script setup> bindings are exposed
  // to the template automatically; the TS rule cannot see template usage.
  {
    files: ['*.vue', '**/*.vue'],
    rules: {
      '@typescript-eslint/no-unused-vars': 'off',
    },
  },
  prettierConfig
);
