<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import type { Article, AuditEntry } from '@/types';
import AuditTimeline from './audit-timeline.vue';

const props = defineProps<{
  article: Article;
  auditTrail: AuditEntry[];
}>();

const emit = defineEmits<{
  close: [];
  moveArticle: [id: string, newStatus: string];
  updateNotes: [id: string, notes: string];
  updateTags: [id: string, tagIds: string[]];
  updateLabels: [id: string, labelIds: string[]];
}>();

// Notes editing
const editingNotes = ref(false);
const noteDraft = ref('');

watch(editingNotes, (val) => {
  if (val) noteDraft.value = props.article.userNotes ?? '';
});

function saveNotes(): void {
  emit('updateNotes', props.article.id, noteDraft.value);
  editingNotes.value = false;
}

function cancelNotes(): void {
  editingNotes.value = false;
}

// Tag/Label add inputs
const newTag = ref('');
const newLabel = ref('');

function removeTag(tag: string): void {
  const updated = props.article.tags.filter((t) => t !== tag);
  emit('updateTags', props.article.id, updated);
}

function addTag(): void {
  const val = newTag.value.trim();
  if (!val || props.article.tags.includes(val)) return;
  emit('updateTags', props.article.id, [...props.article.tags, val]);
  newTag.value = '';
}

function removeLabel(label: string): void {
  const updated = props.article.labels.filter((l) => l !== label);
  emit('updateLabels', props.article.id, updated);
}

function addLabel(): void {
  const val = newLabel.value.trim();
  if (!val || props.article.labels.includes(val)) return;
  emit('updateLabels', props.article.id, [...props.article.labels, val]);
  newLabel.value = '';
}

const confidencePercentage = computed(() =>
  props.article.aiConfidence !== null ? `${Math.round(props.article.aiConfidence * 100)}%` : '---'
);

const aiDecisionLabel = computed(() => {
  if (!props.article.aiDecision) return null;
  return props.article.aiDecision === 'include' ? 'Included' : 'Excluded';
});

const aiDecisionColors = computed(() => {
  if (!props.article.aiDecision) return null;
  if (props.article.aiDecision === 'include') {
    return {
      bg: 'bg-emerald-50',
      border: 'border-emerald-200',
      icon: 'text-emerald-600',
      label: 'text-emerald-900',
      text: 'text-emerald-800',
    };
  }
  return {
    bg: 'bg-rose-50',
    border: 'border-rose-200',
    icon: 'text-rose-600',
    label: 'text-rose-900',
    text: 'text-rose-800',
  };
});

const confidenceBarWidth = computed(() =>
  props.article.aiConfidence !== null ? `${Math.round(props.article.aiConfidence * 100)}%` : '0%'
);
</script>

<template>
  <aside
    class="detail-panel h-full bg-white shadow-[0_4px_24px_rgba(0,0,0,0.15)] border-l border-slate-200 flex flex-col z-50 relative"
  >
    <!-- Header -->
    <div class="p-6 border-b border-slate-100 sticky top-0 bg-white z-10">
      <div class="flex items-center justify-between mb-4">
        <span
          class="text-xs font-label-caps text-primary uppercase bg-primary/5 px-2 py-0.5 rounded"
        >
          Current Selection
        </span>
        <button
          class="material-symbols-outlined text-slate-400 hover:text-slate-900 transition-colors cursor-pointer"
          @click="emit('close')"
        >
          close
        </button>
      </div>
      <h2 class="font-h1 text-h1 text-on-surface leading-tight mb-4">
        {{ article.title }}
      </h2>
      <div class="grid grid-cols-2 gap-4 text-body-sm font-body-sm">
        <div class="flex flex-col gap-1">
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
            >Journal</span
          >
          <span class="text-on-surface truncate">{{ article.journal ?? '---' }}</span>
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
            >Year</span
          >
          <span class="text-on-surface">{{ article.publicationYear ?? '---' }}</span>
        </div>
        <div v-if="article.doi" class="flex flex-col gap-1 col-span-2">
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold">DOI</span>
          <a class="text-primary hover:underline flex items-center gap-1" href="#" @click.prevent>
            {{ article.doi }}
            <span class="material-symbols-outlined text-[14px]">open_in_new</span>
          </a>
        </div>
      </div>
    </div>

    <!-- Scrollable Content -->
    <div class="flex-1 overflow-y-auto p-6 space-y-8">
      <!-- AI Decision Card -->
      <section v-if="aiDecisionLabel && aiDecisionColors">
        <div class="rounded-xl p-4 border" :class="[aiDecisionColors.bg, aiDecisionColors.border]">
          <div class="flex items-center justify-between mb-2">
            <div class="flex items-center gap-2">
              <span class="material-symbols-outlined" :class="aiDecisionColors.icon">
                {{ article.aiDecision === 'include' ? 'verified' : 'cancel' }}
              </span>
              <span class="font-bold" :class="aiDecisionColors.label">
                {{ aiDecisionLabel }}
              </span>
            </div>
            <span
              class="text-[11px] font-bold bg-white px-2 py-0.5 rounded-full shadow-sm"
              :class="aiDecisionColors.label"
            >
              {{ confidencePercentage }} Confidence
            </span>
          </div>
          <!-- Confidence bar -->
          <div class="w-full bg-white/50 h-2 rounded-full overflow-hidden mb-3">
            <div
              class="h-full rounded-full transition-all duration-500"
              :class="article.aiDecision === 'include' ? 'bg-emerald-500' : 'bg-rose-400'"
              :style="{ width: confidenceBarWidth }"
            />
          </div>
          <p
            v-if="article.aiReasoning"
            class="text-body-sm leading-relaxed"
            :class="aiDecisionColors.text"
          >
            <span class="font-semibold">Reasoning:</span> {{ article.aiReasoning }}
          </p>
        </div>
      </section>

      <!-- Matched Criteria -->
      <section
        v-if="
          article.matchedInclusionCriteria.length > 0 || article.matchedExclusionCriteria.length > 0
        "
      >
        <h3 class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">
          Matched Criteria
        </h3>
        <ul class="space-y-2">
          <li
            v-for="criterion in article.matchedInclusionCriteria"
            :key="criterion"
            class="flex items-center gap-3 text-body-sm"
          >
            <span class="material-symbols-outlined text-emerald-500 text-lg">check_circle</span>
            <span>{{ criterion }}</span>
          </li>
          <li
            v-for="criterion in article.matchedExclusionCriteria"
            :key="criterion"
            class="flex items-center gap-3 text-body-sm text-slate-400"
          >
            <span class="material-symbols-outlined text-lg">cancel</span>
            <span class="line-through">{{ criterion }}</span>
          </li>
        </ul>
      </section>

      <!-- Abstract -->
      <section v-if="article.abstractText">
        <h3 class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">
          Abstract
        </h3>
        <p class="text-body-main font-body-main text-on-surface-variant leading-relaxed">
          {{ article.abstractText }}
        </p>
      </section>

      <!-- Tags -->
      <section>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-xs font-label-caps text-slate-500 uppercase tracking-wider">Tags</h3>
        </div>
        <div class="flex flex-wrap gap-2 mb-2">
          <span
            v-for="tag in article.tags"
            :key="'tag-' + tag"
            class="inline-flex items-center gap-1 bg-indigo-50 text-indigo-700 pl-3 pr-1.5 py-1 rounded-lg text-xs font-medium group"
          >
            {{ tag }}
            <button
              class="material-symbols-outlined text-[14px] text-indigo-400 hover:text-indigo-700 cursor-pointer rounded-full hover:bg-indigo-100 leading-none"
              @click="removeTag(tag)"
            >
              close
            </button>
          </span>
          <span
            v-for="label in article.labels"
            :key="'label-' + label"
            class="inline-flex items-center gap-1 border border-slate-200 text-slate-600 pl-3 pr-1.5 py-1 rounded-lg text-xs font-medium group"
          >
            {{ label }}
            <button
              class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer rounded-full hover:bg-slate-100 leading-none"
              @click="removeLabel(label)"
            >
              close
            </button>
          </span>
        </div>
        <div class="flex gap-2">
          <input
            v-model="newTag"
            type="text"
            placeholder="Add tag…"
            class="flex-1 text-xs border border-slate-200 rounded-lg px-2.5 py-1.5 focus:outline-none focus:ring-1 focus:ring-indigo-400"
            @keydown.enter="addTag"
          />
          <button
            class="text-xs text-indigo-600 hover:text-indigo-800 font-semibold cursor-pointer"
            :disabled="!newTag.trim()"
            @click="addTag"
          >
            Add
          </button>
        </div>
        <div class="flex gap-2 mt-2">
          <input
            v-model="newLabel"
            type="text"
            placeholder="Add label…"
            class="flex-1 text-xs border border-slate-200 rounded-lg px-2.5 py-1.5 focus:outline-none focus:ring-1 focus:ring-indigo-400"
            @keydown.enter="addLabel"
          />
          <button
            class="text-xs text-indigo-600 hover:text-indigo-800 font-semibold cursor-pointer"
            :disabled="!newLabel.trim()"
            @click="addLabel"
          >
            Add
          </button>
        </div>
      </section>

      <!-- User Notes -->
      <section>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-xs font-label-caps text-slate-500 uppercase tracking-wider">Notes</h3>
          <button
            v-if="!editingNotes"
            class="material-symbols-outlined text-[16px] text-slate-400 hover:text-indigo-600 cursor-pointer"
            @click="editingNotes = true"
          >
            edit
          </button>
        </div>
        <div v-if="editingNotes" class="space-y-2">
          <textarea
            v-model="noteDraft"
            class="w-full text-sm border border-slate-200 rounded-lg p-3 focus:outline-none focus:ring-1 focus:ring-indigo-400 resize-y min-h-[80px]"
            placeholder="Add notes about this article…"
          />
          <div class="flex gap-2 justify-end">
            <button
              class="text-xs text-slate-500 hover:text-slate-700 font-semibold cursor-pointer px-3 py-1 border border-slate-300 rounded-lg"
              @click="cancelNotes"
            >
              Cancel
            </button>
            <button
              class="text-xs bg-indigo-600 text-white px-3 py-1 rounded-lg font-semibold hover:bg-indigo-700 cursor-pointer"
              @click="saveNotes"
            >
              Save
            </button>
          </div>
        </div>
        <p
          v-else-if="article.userNotes"
          class="text-body-main font-body-main text-on-surface-variant leading-relaxed bg-slate-50 p-3 rounded-lg"
        >
          {{ article.userNotes }}
        </p>
        <p v-else class="text-xs text-slate-400 italic">No notes yet. Click edit to add.</p>
      </section>

      <!-- Audit Trail -->
      <AuditTimeline :entries="auditTrail" />

      <div class="pb-10" />
    </div>

    <!-- Footer Actions -->
    <div class="p-4 border-t border-slate-100 flex gap-3 bg-slate-50/50">
      <button
        v-if="article.status !== 'included'"
        class="flex-1 bg-emerald-600 text-white py-2 rounded-lg font-semibold text-sm hover:bg-emerald-700 active:scale-95 transition-all shadow-sm cursor-pointer"
        @click="emit('moveArticle', article.id, 'included')"
      >
        Include
      </button>
      <button
        v-if="article.status !== 'rejected'"
        class="flex-1 bg-white border border-slate-200 text-rose-700 py-2 rounded-lg font-semibold text-sm hover:bg-rose-50 transition-colors shadow-sm cursor-pointer"
        @click="emit('moveArticle', article.id, 'rejected')"
      >
        Reject
      </button>
      <button
        v-if="article.status !== 'working'"
        class="flex-1 bg-white border border-slate-200 text-slate-700 py-2 rounded-lg font-semibold text-sm hover:bg-slate-50 transition-colors shadow-sm cursor-pointer"
        @click="emit('moveArticle', article.id, 'working')"
      >
        Move to Working
      </button>
    </div>
  </aside>
</template>

<style scoped>
.detail-panel {
  width: var(--detail-panel-width);
  flex-shrink: 0;
  transition: width 0.2s ease;
}

@media (max-width: 1023px) {
  .detail-panel {
    position: fixed;
    top: 0;
    right: 0;
    width: 100%;
    max-width: 100%;
    height: 100vh;
    border-left: none;
    z-index: 60;
    animation: slideInRight 0.25s ease;
  }
}

@keyframes slideInRight {
  from {
    transform: translateX(100%);
  }
  to {
    transform: translateX(0);
  }
}
</style>
