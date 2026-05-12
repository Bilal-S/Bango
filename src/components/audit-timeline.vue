<script setup lang="ts">
import type { AuditEntry, AuditAction } from '@/types';

withDefaults(defineProps<{ entries: AuditEntry[]; showHeader?: boolean }>(), {
  showHeader: true,
});

const emit = defineEmits<{
  navigateToArticle: [id: string];
}>();

const actionLabels: Record<AuditAction, string> = {
  import: 'Article Imported',
  dedup_merge: 'Duplicate Merged',
  dedup_flag: 'Duplicate Flagged',
  status_change: 'Status Changed',
  tag_add: 'Tag Added',
  tag_remove: 'Tag Removed',
  label_add: 'Label Added',
  label_remove: 'Label Removed',
  criteria_match: 'Criteria Matched',
  ai_screen: 'AI Screening Completed',
  manual_override: 'Manual Override',
  ai_summary: 'AI Summary Generated',
};

function formatTimestamp(ts: string): string {
  try {
    const date = new Date(ts);
    return date.toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return ts;
  }
}

function getDotColor(action: AuditAction): string {
  if (action === 'import') return 'bg-slate-300';
  if (action === 'ai_screen') return 'bg-indigo-500';
  if (action === 'manual_override') return 'bg-emerald-500';
  return 'bg-slate-300';
}

/** Match "Auto-detected duplicate of article <uuid>" or "Merged into article <uuid>" */
const DUPLICATE_REF_RE =
  /^(.+ article )([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i;

interface ParsedDetail {
  prefix: string;
  articleId: string | null;
}

function parseDuplicateRef(details: string): ParsedDetail {
  const match = details.match(DUPLICATE_REF_RE);
  if (match && match[1] && match[2]) {
    return { prefix: match[1], articleId: match[2] };
  }
  return { prefix: details, articleId: null };
}
</script>

<template>
  <section>
    <h3
      v-if="showHeader"
      class="text-xs font-label-caps text-slate-500 uppercase mb-6 tracking-wider"
    >
      Audit Trail
    </h3>
    <div v-if="entries.length === 0" class="text-body-sm text-slate-400">
      No audit entries for this article.
    </div>
    <div
      v-else
      class="relative pl-6 space-y-6 before:content-[''] before:absolute before:left-[7px] before:top-1 before:bottom-1 before:w-[2px] before:bg-slate-100"
    >
      <div v-for="entry in entries" :key="entry.id" class="relative">
        <div
          class="absolute -left-[23px] top-1 w-3 h-3 rounded-full border-2 border-white"
          :class="getDotColor(entry.action)"
        />
        <div class="flex flex-col">
          <span class="text-body-sm font-semibold text-on-surface">
            {{ actionLabels[entry.action] || entry.action }}
          </span>
          <span class="text-[11px] text-slate-400">
            {{ formatTimestamp(entry.timestamp) }}
            <span v-if="entry.source === 'ai'"> by AI</span>
            <span v-else-if="entry.source === 'user'"> by User</span>
            <span v-else> via System</span>
          </span>
          <div
            v-if="entry.fromStatus || entry.toStatus"
            class="mt-1 text-[12px] bg-slate-50 p-2 rounded italic text-slate-500 border-l-2 border-slate-200"
          >
            <span v-if="entry.fromStatus">{{ entry.fromStatus }}</span>
            <span v-if="entry.fromStatus && entry.toStatus"> &rarr; </span>
            <span v-if="entry.toStatus">{{ entry.toStatus }}</span>
          </div>
          <p v-if="entry.details" class="mt-1 text-[12px] text-slate-500">
            <template v-if="parseDuplicateRef(entry.details).articleId">
              {{ parseDuplicateRef(entry.details).prefix }}
              <button
                class="text-blue-600 hover:text-blue-800 underline cursor-pointer"
                @click="emit('navigateToArticle', parseDuplicateRef(entry.details).articleId!)"
              >
                {{ parseDuplicateRef(entry.details).articleId }}
              </button>
            </template>
            <template v-else>{{ entry.details }}</template>
          </p>
        </div>
      </div>
    </div>
  </section>
</template>
