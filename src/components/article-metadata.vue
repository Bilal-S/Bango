<script setup lang="ts">
import { ref } from 'vue';
import type { Article } from '@/types';
import { useToast } from '@/composables/use-toast';

const props = defineProps<{
  article: Article;
}>();

const toast = useToast();

// Metadata expand/collapse state (persisted)
const metadataExpanded = ref(localStorage.getItem('bango-metadata-expanded') !== 'false');
function toggleMetadata(): void {
  metadataExpanded.value = !metadataExpanded.value;
  localStorage.setItem('bango-metadata-expanded', String(metadataExpanded.value));
}

function copyDoi(): void {
  if (!props.article.doi) return;
  navigator.clipboard.writeText(props.article.doi).then(() => {
    toast.show('DOI copied to clipboard', 'success', 2000);
  });
}
</script>

<template>
  <section>
    <div class="border border-slate-200 rounded overflow-hidden">
      <button
        class="w-full flex items-center justify-between px-3 py-2 text-xs font-label-caps text-slate-500 uppercase tracking-wider hover:bg-slate-50 cursor-pointer transition-colors"
        @click="toggleMetadata"
      >
        <span class="flex items-center gap-1 min-w-0 overflow-hidden">
          <span class="shrink-0">Metadata</span>
          <span
            v-if="!metadataExpanded && article.authors.length > 0"
            class="text-[11px] text-slate-400 font-body-sm normal-case tracking-normal truncate"
          >
            – {{ article.authors.join(', ') }}
          </span>
        </span>
        <span
          class="material-symbols-outlined text-[16px] transition-transform duration-200 shrink-0"
          :class="{ 'rotate-180': metadataExpanded }"
        >
          expand_more
        </span>
      </button>
      <div v-show="metadataExpanded" class="px-3 pb-3 space-y-3">
        <div
          v-if="article.authors.length > 0"
          class="flex flex-col gap-1 text-body-sm font-body-sm"
        >
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
            >Authors</span
          >
          <span class="text-on-surface">{{ article.authors.join(', ') }}</span>
        </div>
        <div v-if="article.affiliation" class="flex flex-col gap-1 text-body-sm font-body-sm">
          <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
            >Affiliation</span
          >
          <span class="text-on-surface">{{ article.affiliation }}</span>
        </div>
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
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >DOI</span
            >
            <div class="flex items-center gap-1">
              <a
                class="text-primary hover:underline"
                :href="'https://doi.org/' + article.doi"
                target="_blank"
                rel="noopener noreferrer"
              >
                {{ article.doi }}
              </a>
              <button
                class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer transition-colors"
                title="Copy DOI"
                @click="copyDoi"
              >
                content_copy
              </button>
            </div>
          </div>
          <div v-if="article.keywords.length > 0" class="flex flex-col gap-1 col-span-2">
            <span class="text-slate-500 text-[11px] uppercase tracking-wider font-semibold"
              >Keywords</span
            >
            <span class="text-on-surface">{{ article.keywords.join(', ') }}</span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
