<script setup lang="ts">
/**
 * CitationResultCard — renders one `CitationMatch` from the Citation Finder
 * (spec §8.7).
 *
 * Layout: metadata line (authors, year, journal, DOI) + Copy/View buttons +
 * the matched passage + the classification badge (Validating green /
 * Opposing amber) + the cosine confidence as `NN%` + the 1-2 sentence AI
 * explanation.
 *
 * **Progressive passage disclosure** (`highlightedSentences`): when the
 * backend supplies grounded justifying sentences, the card collapses to
 * showing only those snippets by default (one tinted block per sentence).
 * A "Show full passage" toggle expands the full `matchedPassage` with the
 * highlighted sentences rendered inline (`<mark>`-style). When
 * `highlightedSentences` is empty (LLM omitted the field or none grounded),
 * the card falls back to the legacy full-passage display with no toggle.
 *
 * The Citation Style is a per-bubble prop (one value for every card in the
 * bubble), frozen at submit time. The card uses it for the Copy button's
 * plain-text output via the pure `formatCitation` helper. IEEE `[N]` is
 * assigned by the caller via `ieeeIndex` (1-based position in the bubble).
 *
 * v1 does NOT render `misrepresentsSource`; it is a latent field reserved
 * for a future "passage misrepresents the source" warning chip.
 */
import { computed, ref } from 'vue';
import { formatCitation } from '@/composables/use-citation-finder';
import type { CitationMatch, CitationStyle } from '@/types/citation-finder';

const props = defineProps<{
  match: CitationMatch;
  /** Citation style captured at submit time + frozen on the bubble. */
  style: CitationStyle;
  /** 1-based position within the bubble for IEEE `[N]` numbering. Default 1. */
  ieeeIndex?: number;
}>();

const emit = defineEmits<{
  /** Parent copies `formatCitation(match, style, ieeeIndex)` to the clipboard. */
  copy: [text: string];
  /** Parent opens the article detail slide-over (existing `openArticleDetail`). */
  view: [articleId: string];
}>();

/** Confidence as a rounded percentage string, e.g. "92%". */
const confidencePct = computed(() => `${Math.round(props.match.confidence * 100)}%`);

/** The matched passage's section label, or `null` when `sectionOrigin` is
 *  `null` (Text-derived chunks) → the template omits the `§…` badge. */
const sectionBadge = computed(() => props.match.sectionOrigin);

/** Truncated DOI for display (keeps the prefix + last 12 chars). */
const doiDisplay = computed(() => {
  const doi = props.match.doi;
  if (!doi) return null;
  return doi.length > 24 ? `${doi.slice(0, 8)}…${doi.slice(-12)}` : doi;
});

/** Plain-text citation for the Copy button. */
const citationText = computed(() => formatCitation(props.match, props.style, props.ieeeIndex));

/** Whether the compact (highlighted-snippets) view is available. When false,
 *  the card renders the full passage with no toggle (legacy behavior). */
const hasHighlights = computed(() => props.match.highlightedSentences.length > 0);

/** Expand/collapse state for the full passage. Defaults to collapsed (false)
 *  when highlights are available; irrelevant when `hasHighlights` is false. */
const expanded = ref(false);

function toggleExpanded() {
  expanded.value = !expanded.value;
}

/**
 * The full passage split into segments with the highlighted sentences marked,
 * so the expanded view renders them inline with `<mark>` styling. Each
 * segment is `{ text, highlight }`. Locates each highlighted sentence via a
 * case-insensitive search on a whitespace-normalized basis (PDF extraction
 * produces irregular spacing); if a sentence can't be located in the raw
 * passage, it is silently skipped (the collapsed view still shows it as its
 * own snippet).
 *
 * Returns `null` when there are no highlights (caller renders the plain
 * passage verbatim instead).
 */
interface PassageSegment {
  text: string;
  highlight: boolean;
}
const passageSegments = computed<PassageSegment[] | null>(() => {
  if (!hasHighlights.value) return null;
  const passage = props.match.matchedPassage;
  const lowerPassage = passage.toLowerCase();
  // Find all (start, end) ranges of highlighted sentences in the passage.
  type Range = { start: number; end: number };
  const ranges: Range[] = [];
  for (const sentence of props.match.highlightedSentences) {
    // Try exact (case-insensitive) first.
    const needle = sentence.trim().toLowerCase();
    if (!needle) continue;
    let idx = lowerPassage.indexOf(needle);
    if (idx === -1) {
      // Fallback: whitespace-normalized search. Collapse whitespace runs in
      // both the passage and the needle to a single space, then map the
      // match back to the original text. This handles PDF-extraction spacing
      // irregularities that would otherwise defeat an exact substring match.
      const normPassage = passage.replace(/\s+/g, ' ');
      const normNeedle = needle.replace(/\s+/g, ' ');
      const normIdx = normPassage.toLowerCase().indexOf(normNeedle);
      if (normIdx === -1) continue; // can't locate → skip the inline mark
      // Map the normalized offset back to the original passage by walking
      // the original and consuming whitespace runs. This is O(n) per
      // sentence; passages are bounded (~chunk size), so this is fine.
      let origIdx = 0;
      let normConsumed = 0;
      while (normConsumed < normIdx && origIdx < passage.length) {
        if (/\s/.test(passage[origIdx]!)) {
          // Skip the entire whitespace run in the original (it collapsed to
          // one space in the normalized form).
          while (origIdx < passage.length && /\s/.test(passage[origIdx]!)) origIdx++;
          normConsumed++; // one normalized space consumed
        } else {
          origIdx++;
          normConsumed++;
        }
      }
      idx = origIdx;
    }
    ranges.push({ start: idx, end: idx + sentence.trim().length });
  }
  if (ranges.length === 0) return null;
  // Sort ranges by start offset so we can walk the passage linearly.
  ranges.sort((a, b) => a.start - b.start);
  const segments: PassageSegment[] = [];
  let cursor = 0;
  for (const r of ranges) {
    if (r.start > cursor) {
      segments.push({ text: passage.slice(cursor, r.start), highlight: false });
    }
    // Clamp end to passage length (defense-in-depth).
    const end = Math.min(r.end, passage.length);
    if (end > r.start) {
      segments.push({ text: passage.slice(r.start, end), highlight: true });
    }
    cursor = end;
  }
  if (cursor < passage.length) {
    segments.push({ text: passage.slice(cursor), highlight: false });
  }
  return segments;
});

function onCopy() {
  emit('copy', citationText.value);
}

function onView() {
  emit('view', props.match.articleId);
}
</script>

<template>
  <div class="citation-card">
    <!-- Metadata header line + actions -->
    <div class="citation-card__header">
      <div class="citation-card__meta">
        <span class="citation-card__authors">{{ match.authors[0] ?? 'Unknown' }}</span>
        <span v-if="match.publicationYear" class="citation-card__year"
          >({{ match.publicationYear }})</span
        >
        <span v-if="match.journal" class="citation-card__journal">{{ match.journal }}</span>
        <span v-if="doiDisplay" class="citation-card__doi">doi:{{ doiDisplay }}</span>
      </div>
      <div class="citation-card__actions">
        <button
          class="citation-card__btn citation-card__btn--copy"
          title="Copy citation"
          @click="onCopy"
        >
          <span class="material-symbols-outlined text-[14px]">content_copy</span>
          <span class="citation-card__btn-label">Copy</span>
        </button>
        <button
          class="citation-card__btn citation-card__btn--view"
          title="Open article details"
          @click="onView"
        >
          <span class="material-symbols-outlined text-[14px]">open_in_new</span>
          <span class="citation-card__btn-label">View</span>
        </button>
      </div>
    </div>

    <!-- Matched passage: progressive disclosure -->
    <div class="citation-card__passage">
      <span v-if="sectionBadge" class="citation-card__section-badge">§{{ sectionBadge }}</span>
      <div class="citation-card__passage-body">
        <!-- Collapsed (default when highlights exist): one snippet per
             highlighted sentence. -->
        <template v-if="hasHighlights && !expanded">
          <p
            v-for="(sentence, i) in match.highlightedSentences"
            :key="i"
            class="citation-card__passage-text citation-card__passage-text--snippet"
          >
            "{{ sentence }}"
          </p>
        </template>
        <!-- Expanded: full passage with inline <mark> highlights. -->
        <p
          v-else-if="hasHighlights && expanded && passageSegments"
          class="citation-card__passage-text"
        >
          <template v-for="(seg, i) in passageSegments" :key="i">
            <mark v-if="seg.highlight" class="citation-card__passage-mark">{{ seg.text }}</mark>
            <template v-else>{{ seg.text }}</template>
          </template>
        </p>
        <!-- Fallback (no highlights): legacy full-passage display. -->
        <p v-else class="citation-card__passage-text">"{{ match.matchedPassage }}"</p>
      </div>
      <button
        v-if="hasHighlights"
        class="citation-card__expand-toggle"
        :title="expanded ? 'Hide full passage' : 'Show full passage'"
        @click="toggleExpanded"
      >
        <span class="material-symbols-outlined citation-card__expand-icon">
          {{ expanded ? 'expand_less' : 'expand_more' }}
        </span>
        <span class="citation-card__expand-label">
          {{ expanded ? 'Less' : 'More' }}
        </span>
      </button>
    </div>

    <!-- Classification + confidence + AI explanation -->
    <div class="citation-card__footer">
      <div class="citation-card__tags">
        <span
          class="citation-card__classification"
          :class="{
            'citation-card__classification--validating': match.classification === 'validating',
            'citation-card__classification--opposing': match.classification === 'opposing',
          }"
        >
          <span v-if="match.classification === 'validating'">✓ Validating</span>
          <span v-else>✗ Opposing</span>
        </span>
        <span class="citation-card__confidence">{{ confidencePct }} match</span>
      </div>
      <p class="citation-card__explanation">{{ match.relevanceExplanation }}</p>
    </div>
  </div>
</template>

<style scoped>
.citation-card {
  border: 1px solid rgb(226 232 240); /* slate-200 */
  border-radius: 0.625rem;
  background: #fff;
  padding: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.citation-card__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.5rem;
}

.citation-card__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.25rem;
  font-size: 0.75rem;
  color: rgb(71 85 105); /* slate-600 */
  min-width: 0;
}

.citation-card__authors {
  font-weight: 600;
  color: rgb(15 23 42); /* slate-900 */
}

.citation-card__year {
  color: rgb(100 116 139); /* slate-500 */
}

.citation-card__journal {
  font-style: italic;
  color: rgb(71 85 105); /* slate-600 */
}

.citation-card__doi {
  color: rgb(148 163 184); /* slate-400 */
  font-size: 0.6875rem;
}

.citation-card__actions {
  display: flex;
  gap: 0.25rem;
  flex-shrink: 0;
}

.citation-card__btn {
  display: inline-flex;
  align-items: center;
  gap: 0.1875rem;
  padding: 0.1875rem 0.4375rem;
  border-radius: 0.375rem;
  border: 1px solid rgb(226 232 240); /* slate-200 */
  background: #fff;
  color: rgb(71 85 105); /* slate-600 */
  font-size: 0.6875rem;
  font-weight: 600;
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s,
    border-color 0.15s;
}

.citation-card__btn:hover {
  background: rgb(248 250 252); /* slate-50 */
  border-color: rgb(203 213 225); /* slate-300 */
}

.citation-card__btn--copy:hover {
  color: rgb(79 70 229); /* indigo-600 */
}

.citation-card__btn--view:hover {
  color: rgb(79 70 229); /* indigo-600 */
}

.citation-card__passage {
  background: rgb(248 250 252); /* slate-50 */
  border-left: 3px solid rgb(199 210 254); /* indigo-200 */
  border-radius: 0.375rem;
  padding: 0.5rem 0.625rem;
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
  align-items: flex-start;
}

.citation-card__section-badge {
  flex-shrink: 0;
  display: inline-block;
  align-self: flex-start;
  padding: 0.0625rem 0.3125rem;
  border-radius: 0.25rem;
  background: rgb(224 231 255); /* indigo-100 */
  color: rgb(67 56 202); /* indigo-800 */
  font-size: 0.625rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  white-space: nowrap;
}

.citation-card__passage-body {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  width: 100%;
}

.citation-card__passage-text {
  font-size: 0.75rem;
  line-height: 1.4;
  color: rgb(51 65 85); /* slate-700 */
  font-style: italic;
  margin: 0;
}

.citation-card__passage-text--snippet {
  /* Collapsed-view snippets: same tinted style, but bolder so the "key"
     sentences stand out from the surrounding card. */
  font-weight: 500;
}

.citation-card__passage-mark {
  background: rgb(254 240 138); /* amber-200 */
  color: rgb(120 53 15); /* amber-900 */
  font-style: italic;
  font-weight: 500;
  padding: 0 0.0625rem;
  border-radius: 0.125rem;
}

.citation-card__expand-toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.125rem;
  background: none;
  border: none;
  color: rgb(100 116 139); /* slate-500 */
  font-size: 0.6875rem;
  font-weight: 600;
  cursor: pointer;
  padding: 0;
  margin-top: 0.125rem;
}

.citation-card__expand-toggle:hover {
  color: rgb(79 70 229); /* indigo-600 */
}

.citation-card__expand-icon {
  font-size: 16px;
}

.citation-card__footer {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.citation-card__tags {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.citation-card__classification {
  display: inline-flex;
  align-items: center;
  padding: 0.0625rem 0.4375rem;
  border-radius: 0.25rem;
  font-size: 0.6875rem;
  font-weight: 700;
}

.citation-card__classification--validating {
  background: rgb(220 252 231); /* green-100 */
  color: rgb(22 101 52); /* green-800 */
}

.citation-card__classification--opposing {
  background: rgb(254 243 199); /* amber-100 */
  color: rgb(120 53 15); /* amber-900 */
}

.citation-card__confidence {
  font-size: 0.6875rem;
  font-weight: 600;
  color: rgb(100 116 139); /* slate-500 */
}

.citation-card__explanation {
  font-size: 0.75rem;
  line-height: 1.4;
  color: rgb(71 85 105); /* slate-600 */
  margin: 0;
}
</style>
