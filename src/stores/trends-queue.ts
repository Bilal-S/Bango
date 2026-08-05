import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import {
  type TimeRangeId,
  isValidCustomRange,
  clampToFiveYearWindow,
  buildResearchRange,
  TIME_RANGE_PRESETS,
  MAX_QUEUE_SIZE,
} from '../utils/google-trends';

export const useTrendsQueueStore = defineStore('trendsQueue', () => {
  const keywords = ref<string[]>([]);
  const timeRangeId = ref<TimeRangeId>('5y');
  const customStart = ref<string | null>(null);
  const customEnd = ref<string | null>(null);
  const researchRange = ref<{ start: string; end: string } | null>(null);
  const collapsed = ref<boolean>(false);
  const revision = ref<number>(0);

  /** When true (set after 429), widgets stop rendering until manual retry.
   *  Changing keywords/time-range also clears it. */
  const halted = ref<boolean>(false);

  const hasKeywords = computed(() => keywords.value.length > 0);

  const resolvedRange = computed(() => {
    const defaultPreset = TIME_RANGE_PRESETS.find((p) => p.id === '5y')!;

    if (timeRangeId.value === 'custom') {
      if (customStart.value && customEnd.value) {
        const validation = isValidCustomRange(customStart.value, customEnd.value);
        if (validation.ok) {
          return {
            apiTime: `${customStart.value} ${customEnd.value}`,
            queryDate: `${customStart.value} ${customEnd.value}`,
            label: 'Custom Range',
          };
        }
      }
      return {
        apiTime: defaultPreset.apiTime,
        queryDate: defaultPreset.queryDate,
        label: `${defaultPreset.label} (Fallback)`,
      };
    }

    if (timeRangeId.value === 'research') {
      if (researchRange.value) {
        return {
          apiTime: `${researchRange.value.start} ${researchRange.value.end}`,
          queryDate: `${researchRange.value.start} ${researchRange.value.end}`,
          label: 'Research Range',
        };
      }
      return {
        apiTime: defaultPreset.apiTime,
        queryDate: defaultPreset.queryDate,
        label: `${defaultPreset.label} (Fallback)`,
      };
    }

    const preset = TIME_RANGE_PRESETS.find((p) => p.id === timeRangeId.value);
    if (preset) {
      return {
        apiTime: preset.apiTime,
        queryDate: preset.queryDate,
        label: preset.label,
      };
    }

    return {
      apiTime: defaultPreset.apiTime,
      queryDate: defaultPreset.queryDate,
      label: defaultPreset.label,
    };
  });

  function addKeyword(term: string): boolean {
    if (!term) return false;
    const clean = term.trim();
    if (!clean) return false;

    // Case-insensitive check but keep casing of first added
    const exists = keywords.value.some((k) => k.toLowerCase() === clean.toLowerCase());
    if (exists) return false;

    if (keywords.value.length >= MAX_QUEUE_SIZE) {
      return false;
    }

    keywords.value.push(clean);
    halted.value = false; // adding a keyword is a fresh attempt
    revision.value++;
    return true;
  }

  function removeKeyword(term: string): boolean {
    const idx = keywords.value.findIndex((k) => k.toLowerCase() === term.trim().toLowerCase());
    if (idx >= 0) {
      keywords.value.splice(idx, 1);
      halted.value = false;
      revision.value++;
      return true;
    }
    return false;
  }

  function clearAll() {
    keywords.value = [];
    halted.value = false;
    revision.value++;
  }

  function setTimeRange(id: TimeRangeId) {
    timeRangeId.value = id;
    halted.value = false;
    revision.value++;
  }

  function setCustomRange(start: string, end: string): boolean {
    const clamped = clampToFiveYearWindow(start, end);
    customStart.value = clamped.start;
    customEnd.value = clamped.end;
    timeRangeId.value = 'custom';
    revision.value++;
    return clamped.clamped;
  }

  function setResearchRange(minYear: number, maxYear: number, mostActiveYear: number) {
    const range = buildResearchRange(minYear, maxYear, mostActiveYear);
    researchRange.value = range;
    revision.value++;
  }

  function toggleCollapsed() {
    collapsed.value = !collapsed.value;
  }

  function bumpRevision() {
    revision.value++;
  }

  /** Called by the panel when any widget reports a 429 (preflight or runtime). */
  function haltQueue() {
    halted.value = true;
  }

  /** Called when the user explicitly retries - clears the halt flag. */
  function resumeQueue() {
    halted.value = false;
    revision.value++;
  }

  return {
    keywords,
    timeRangeId,
    customStart,
    customEnd,
    researchRange,
    collapsed,
    revision,
    halted,
    hasKeywords,
    resolvedRange,
    addKeyword,
    removeKeyword,
    clearAll,
    setTimeRange,
    setCustomRange,
    setResearchRange,
    toggleCollapsed,
    bumpRevision,
    haltQueue,
    resumeQueue,
  };
});
