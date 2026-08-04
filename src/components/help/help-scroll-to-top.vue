<script setup lang="ts">
/**
 * Reusable "Scroll to top" icon.
 *
 * A bare Material Symbols `vertical_align_top` glyph with a mouse-over
 * tooltip (via the native `title` attribute). Emits `click` - the parent
 * decides what "top" means (e.g. which scroll container to reset and
 * whether to also reset active-section state).
 *
 * Used at the bottom of each Reference section card in the Help screen so
 * users can jump back to the opening state of the tab without scrolling.
 *
 * The `<button>` element is kept (rather than a bare `<span>`) for
 * accessibility: it is keyboard-focusable, announces itself to screen
 * readers via `aria-label`, and handles click / Enter / Space natively.
 * All box styling (border, background, dimensions) is stripped so only the
 * glyph is visible.
 */

const props = withDefaults(
  defineProps<{
    /** Mouse-over tooltip text (also the accessible label). */
    label?: string;
  }>(),
  {
    label: 'Scroll to top',
  }
);

const emit = defineEmits<{
  click: [];
}>();

function onClick(): void {
  emit('click');
}

// `props` is referenced for the `label` binding in the template.
void props;
</script>

<template>
  <button
    type="button"
    class="help-scroll-to-top"
    :title="label"
    :aria-label="label"
    @click="onClick"
  >
    <span class="material-symbols-outlined help-scroll-to-top__icon">vertical_align_top</span>
  </button>
</template>

<style scoped>
.help-scroll-to-top {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: none;
  padding: 0;
  margin: 0;
  color: var(--color-on-surface-variant, #475569);
  cursor: pointer;
  transition: color 0.15s;
  font-family: inherit;
  line-height: 0;
}

.help-scroll-to-top:hover {
  color: #4f46e5;
}

.help-scroll-to-top__icon {
  font-size: 20px;
}
</style>
