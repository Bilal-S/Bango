<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import SuggestInput from '@/components/suggest-input.vue';
import type { SuggestOption } from '@/types';

/** "Replace with..." merge dialog for Tags & Labels management.
 *
 * Uses `suggest-input.vue` structured-options single-select (same as journal
 * autocomplete). Shows honest-count upper-bound confirmation; real counts
 * surface in success toast (avoids a `preview_merge` IPC). */

type Kind = 'tag' | 'label';

const props = defineProps<{
  kind: Kind;
  from: { id: string; name: string; articleCount: number } | null;
  candidates: { id: string; name: string; articleCount: number }[];
  visible: boolean;
}>();

const emit = defineEmits<{
  'update:visible': [value: boolean];
  merge: [payload: { fromId: string; intoId: string }];
}>();

// Local search-model state for the suggest-input. Reset whenever the dialog
// opens or the survivor selection is cleared.
const search = ref('');
const selectedSurvivorId = ref<string | null>(null);

watch(
  () => props.visible,
  (open) => {
    if (open) {
      search.value = '';
      selectedSurvivorId.value = null;
    }
  }
);

const noun = computed(() => (props.kind === 'tag' ? 'tag' : 'label'));

const selectedCandidate = computed(
  () => props.candidates.find((c) => c.id === selectedSurvivorId.value) ?? null
);

/** Mapped options for `suggest-input`'s structured-options mode. */
const options = computed<SuggestOption[]>(() =>
  props.candidates.map((c) => ({
    id: c.id,
    label: c.name,
    badge: `(${c.articleCount})`,
  }))
);

/** "Looks like a duplicate" hint: normalized names match. Reassures the user
 * they're merging typo/casing variants of the same concept. */
const normalized = (s: string): string =>
  s.toLowerCase().replace(/\s+/g, '-').replace(/-+/g, '-').replace(/^-|-$/g, '');

const isDuplicateHint = computed(() => {
  if (!props.from || !selectedCandidate.value) return false;
  return normalized(props.from.name) === normalized(selectedCandidate.value.name);
});

const hasNoCandidates = computed(() => props.candidates.length === 0);

function onSelect(_name: string, option?: SuggestOption): void {
  selectedSurvivorId.value = option?.id ?? null;
}

function close(): void {
  emit('update:visible', false);
}

function confirm(): void {
  if (!props.from || !selectedSurvivorId.value) return;
  emit('merge', { fromId: props.from.id, intoId: selectedSurvivorId.value });
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="dialog-overlay" @click.self="close" @keyup.escape="close">
      <div class="dialog">
        <!-- Corner close (X) - standard `.dialog__close` from forms.css -->
        <button class="dialog__close" aria-label="Close" @click="close">
          <span class="material-symbols-outlined">close</span>
        </button>

        <h2>Replace {{ noun }}</h2>

        <!-- Subject line: the from-tag/label on its own line, mono-styled to
             match the chips. Inline `margin: 0` because the `.dialog` flex
             container already provides spacing via `gap`. -->
        <p
          v-if="from"
          class="dialog__desc"
          style="font-family: var(--font-mono, monospace); margin: 0"
        >
          '{{ from.name }}'
          <span v-if="from.articleCount > 0">({{ from.articleCount }} articles)</span>
        </p>

        <!-- Body -->
        <template v-if="hasNoCandidates">
          <p class="dialog__desc" style="margin: 0">
            No other {{ noun }}s available to merge into.
          </p>
        </template>
        <template v-else>
          <p
            class="dialog__desc"
            style="text-transform: uppercase; letter-spacing: 0.05em; margin: 0"
          >
            with
          </p>
          <SuggestInput
            v-model="search"
            :options="options"
            :clear-on-select="false"
            placeholder="Search..."
            @select="onSelect"
            @escape="close"
          />
          <p
            v-if="isDuplicateHint"
            class="dialog__desc"
            style="display: flex; align-items: center; gap: 0.25rem"
          >
            <span
              class="material-symbols-outlined"
              style="font-size: 16px; color: var(--color-primary, #3525cd)"
              >check_circle</span
            >
            Looks like a duplicate - good candidate.
          </p>

          <!-- Honest-count confirmation panel -->
          <div v-if="selectedCandidate" class="dialog__danger-box">
            <span class="material-symbols-outlined">warning</span>
            <p>
              Up to <strong>{{ from?.articleCount ?? 0 }}</strong> article(s) will be moved to
              <strong>'{{ selectedCandidate.name }}'</strong>. Articles that already have it will
              just lose the duplicate.<br />
              '{{ from?.name }}' will be deleted permanently.
            </p>
          </div>
        </template>

        <!-- Footer: standard `.dialog__actions` with canonical button classes -->
        <div class="dialog__actions">
          <button class="btn btn--secondary" @click="close">Cancel</button>
          <button class="btn btn--primary" :disabled="!selectedSurvivorId" @click="confirm">
            Confirm replace
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
