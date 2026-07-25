<script setup lang="ts">
/**
 * Reusable text/number input with a built-in clear ("x") affordance pinned to
 * the right edge. Wraps a native `<input>` (so all native attrs flow through)
 * and surfaces the events a parent needs for filter-style inputs:
 *
 * - `update:modelValue` (v-model) on every keystroke.
 * - `clear` ONLY when the "x" is clicked (parent decides what "clear" means:
 *   e.g. coerce `''` -> `null` for number fields, then re-run the query).
 * - `enter`, `input`, `focus`, `blur` forwarded so existing handlers (e.g. the
 *   Author autocomplete dropdown, Enter-to-apply) keep working unchanged.
 *
 * The clear button is hidden while the field is empty OR disabled, matching
 * the established pattern in `citation-controls.vue` / `cocitation-controls.vue`.
 */

const props = withDefaults(
  defineProps<{
    modelValue: string;
    placeholder?: string;
    /** Extra classes applied to the inner `<input>` (focus ring, disabled style, width). */
    inputClass?: string;
    disabled?: boolean;
    type?: 'text' | 'number';
    /** Forwarded to the native input (number min/max for the Year range). */
    min?: number | string;
    max?: number | string;
    /** Forwarded to the native input (e.g. the disabled-tooltip for DOI). */
    title?: string;
  }>(),
  {
    placeholder: '',
    inputClass: '',
    disabled: false,
    type: 'text',
    min: undefined,
    max: undefined,
    title: '',
  }
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
  /** Fires only when the clear ("x") button is clicked. */
  clear: [];
  enter: [];
  input: [value: string];
  focus: [];
  blur: [];
}>();

function onInput(event: Event): void {
  const value = (event.target as HTMLInputElement).value;
  emit('update:modelValue', value);
  emit('input', value);
}

function onKeyupEnter(): void {
  emit('enter');
}

function onFocus(): void {
  emit('focus');
}

function onBlur(): void {
  emit('blur');
}

function clear(): void {
  // Emit the cleared text via v-model so a text parent picks it up for free,
  // then emit `clear` so the parent can coerce (e.g. '' -> null for numbers)
  // and re-submit the query.
  emit('update:modelValue', '');
  emit('clear');
}

// `props` is referenced for the `disabled`/`title` bindings in the template.
void props;
</script>

<template>
  <div class="relative">
    <input
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :min="min"
      :max="max"
      :title="title"
      class="w-full pr-8 bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent disabled:bg-slate-100 disabled:text-slate-400 disabled:cursor-not-allowed"
      :class="inputClass"
      @input="onInput"
      @keyup.enter="onKeyupEnter"
      @focus="onFocus"
      @blur="onBlur"
    />
    <button
      v-if="modelValue && !disabled"
      type="button"
      class="clearable-input__clear absolute right-2 top-1/2 -translate-y-1/2 flex items-center justify-center text-slate-400 hover:text-slate-600 cursor-pointer"
      title="Clear"
      aria-label="Clear"
      @click="clear"
    >
      <span class="material-symbols-outlined text-base">close</span>
    </button>
  </div>
</template>
