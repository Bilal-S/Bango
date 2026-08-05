<script setup lang="ts">
/** Reusable text/number input with built-in clear ("x") affordance.
 *
 * Wraps native `<input>`. `update:modelValue` on every keystroke; `clear`
 * only on "x" click (parent coerces: e.g. `''`->`null` for number fields).
 * Forwards `enter`, `input`, `focus`, `blur`. Clear hidden when empty/disabled.
 * `maxlength` + `autofocus` cover inline-edit cases (dashboard title). */
import { onMounted, ref } from 'vue';

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
    /** Forwarded to the native input's `maxlength` (character-cap enforcement). */
    maxlength?: number;
    /** When true, focuses + selects the input's text on mount. */
    autofocus?: boolean;
  }>(),
  {
    placeholder: '',
    inputClass: '',
    disabled: false,
    type: 'text',
    min: undefined,
    max: undefined,
    title: '',
    maxlength: undefined,
    autofocus: false,
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

const inputEl = ref<HTMLInputElement | null>(null);

onMounted(() => {
  if (props.autofocus && inputEl.value) {
    inputEl.value.focus();
    inputEl.value.select();
  }
});

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
  /* Emit cleared text via v-model so text parent picks it up free, then emit
     `clear` so parent can coerce (e.g. '' -> null for numbers) and re-submit. */
  emit('update:modelValue', '');
  emit('clear');
}

// `props` is referenced for the `disabled`/`title` bindings in the template.
void props;
</script>

<template>
  <div class="relative">
    <input
      ref="inputEl"
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :min="min"
      :max="max"
      :maxlength="maxlength"
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
      @mousedown.prevent
    >
      <span class="material-symbols-outlined text-base">close</span>
    </button>
  </div>
</template>
