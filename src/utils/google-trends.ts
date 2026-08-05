export type TimeRangeId = '7d' | '30d' | '12m' | '5y' | 'custom' | 'research';

export interface TimeRangePreset {
  id: TimeRangeId;
  label: string;
  apiTime: string;
  queryDate: string;
}

export const TIME_RANGE_PRESETS: TimeRangePreset[] = [
  { id: '7d', label: 'Past 7 days', apiTime: 'now 7-d', queryDate: 'now 7-d' },
  { id: '30d', label: 'Past 30 days', apiTime: 'now 30-d', queryDate: 'now 30-d' },
  { id: '12m', label: 'Past 12 months', apiTime: 'today 12-m', queryDate: 'today 12-m' },
  { id: '5y', label: 'Past 5 years', apiTime: 'today 5-y', queryDate: 'today 5-y' },
];

export const MAX_QUEUE_SIZE = 5;
const MAX_RANGE_DAYS = 1825; // 5 years

function formatDate(date: Date): string {
  const y = date.getUTCFullYear();
  const m = String(date.getUTCMonth() + 1).padStart(2, '0');
  const d = String(date.getUTCDate()).padStart(2, '0');
  return `${y}-${m}-${d}`;
}

/** Clamp custom date range to max 5 years (1825 days). Slides start forward if needed. */
export function clampToFiveYearWindow(
  start: string,
  end: string
): { start: string; end: string; clamped: boolean } {
  const startDate = new Date(start);
  const endDate = new Date(end);

  if (isNaN(startDate.getTime()) || isNaN(endDate.getTime())) {
    return { start, end, clamped: false };
  }

  const diffTime = endDate.getTime() - startDate.getTime();
  const diffDays = diffTime / (1000 * 60 * 60 * 24);

  if (diffDays > MAX_RANGE_DAYS) {
    const newStart = new Date(endDate.getTime() - MAX_RANGE_DAYS * 24 * 60 * 60 * 1000);
    return {
      start: formatDate(newStart),
      end,
      clamped: true,
    };
  }

  return { start, end, clamped: false };
}

function getTodayUTC(date = new Date()): Date {
  return new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate(), 0, 0, 0, 0));
}

/** Validate custom start/end date strings. */
export function isValidCustomRange(start: string, end: string): { ok: boolean; reason?: string } {
  if (!start || !end) {
    return { ok: false, reason: 'Dates must not be empty.' };
  }

  const startDate = new Date(start);
  const endDate = new Date(end);

  if (isNaN(startDate.getTime()) || isNaN(endDate.getTime())) {
    return { ok: false, reason: 'Invalid date format.' };
  }

  if (startDate > endDate) {
    return { ok: false, reason: 'Start date must be before or equal to end date.' };
  }

  const todayUTC = getTodayUTC();
  const endDateUTC = new Date(
    Date.UTC(endDate.getUTCFullYear(), endDate.getUTCMonth(), endDate.getUTCDate(), 0, 0, 0, 0)
  );
  if (endDateUTC > todayUTC) {
    return { ok: false, reason: 'End date cannot be in the future.' };
  }

  const diffTime = endDate.getTime() - startDate.getTime();
  const diffDays = diffTime / (1000 * 60 * 60 * 24);

  if (diffDays > MAX_RANGE_DAYS) {
    return { ok: false, reason: 'Custom range cannot exceed 5 years.' };
  }

  return { ok: true };
}

/** Derive research range from dataset publication years. Clamps to 5y only as pre-2002 fallback. */
export function buildResearchRange(
  minYear: number,
  maxYear: number,
  mostActiveYear: number,
  today = new Date()
): { start: string; end: string } {
  const todayUTC = getTodayUTC(today);
  const todayYear = todayUTC.getUTCFullYear();

  if (minYear > 2001) {
    const start = `${minYear}-01-01`;
    const clampEnd = Math.min(todayYear, maxYear);
    const end = `${clampEnd}-12-31`;

    const endDate = new Date(end);
    const resolvedEnd = endDate > todayUTC ? formatDate(todayUTC) : end;

    return { start, end: resolvedEnd };
  }

  if (mostActiveYear > 2001) {
    let startYear = mostActiveYear - 2;
    let endYear = mostActiveYear + 2;

    if (startYear < 2002) {
      startYear = 2002;
      endYear = 2006;
    }

    if (endYear > todayYear) {
      endYear = todayYear;
      startYear = Math.max(2002, endYear - 4);
    }

    const start = `${startYear}-01-01`;
    let end = `${endYear}-12-31`;

    const endDate = new Date(end);
    if (endDate > todayUTC) {
      end = formatDate(todayUTC);
    }

    return { start, end };
  }

  // pre-2002 fallback: default to last 5 years
  const start = formatDate(new Date(todayUTC.getTime() - 5 * 365.25 * 24 * 60 * 60 * 1000));
  const end = formatDate(todayUTC);
  return { start, end };
}

/** Clean keyword labels for Google Trends: strip trailing punctuation + parentheticals. */
export function sanitizeKeyword(keyword: string): string {
  if (!keyword) return '';
  let cleaned = keyword
    .trim()
    .replace(/[.,;:!?]+$/, '')
    .trim();
  cleaned = cleaned.replace(/\s*[([][^)]*?[)\]]/g, '').trim();
  return cleaned;
}

/** Build comparison items parameter for renderExploreWidget. */
export function buildComparisonItems(keywords: string[], apiTime: string): string {
  const sanitized = keywords.map(sanitizeKeyword).filter(Boolean);
  const items = sanitized.map((kw) => ({
    keyword: kw,
    geo: '',
    time: apiTime,
  }));
  return JSON.stringify(items);
}

/** Build url-encoded exploreQuery parameter. */
export function buildExploreQuery(
  type: 'TIMESERIES' | 'GEO_MAP',
  keywords: string[],
  queryDate: string
): string {
  const sanitized = keywords.map(sanitizeKeyword).filter(Boolean);
  const q = sanitized.join(',');
  const encodedDate = queryDate.replace(/ /g, '%20');
  const isCustomDate = /\d{4}-\d{2}-\d{2}/.test(queryDate);

  if (type === 'GEO_MAP') {
    let dateParam = encodedDate;
    if (!queryDate.startsWith('now') && sanitized.length > 1) {
      dateParam = Array(sanitized.length).fill(encodedDate).join(',');
    }
    return `q=${q}&hl=en&legacy&date=${dateParam}`;
  } else {
    const legacyPart = isCustomDate ? '&legacy' : '';
    return `date=${encodedDate}&q=${q}&hl=en-US${legacyPart}`;
  }
}

/** Build inline monitor script for the widget iframe <head>.
 * Detects embed failures (429, network errors, script load failures, timeouts)
 * and reports via postMessage. Multiple detection layers: error listener on
 * <script>, fetch/XHR patches, watchdog timeout. Success is inferred from
 * absence of errors + watchdog survival. `settled` flag ensures single report. */
export function buildEmbedMonitorScript(): string {
  return `
(function() {
  var settled = false;
  var WATCHDOG_MS = 10000;

  function report(status, reason, httpStatus) {
    if (settled) return;
    settled = true;
    var payload = { type: "trends_embed_status", status: status };
    if (reason) payload.reason = reason;
    if (httpStatus != null) payload.httpStatus = httpStatus;
    try { window.parent.postMessage(payload, "*"); } catch (e) {}
  }

  // 1. Capture-phase error listener: catches failed resource loads (loader script 429/network)
  window.addEventListener("error", function(event) {
    var target = event.target || event.srcElement;
    if (target && target.tagName === "SCRIPT") {
      report("error", "network");
    }
  }, true);

  // 2. Patch fetch to detect HTTP errors on data requests from this document
  if (window.fetch) {
    var origFetch = window.fetch;
    window.fetch = function() {
      return origFetch.apply(this, arguments).then(function(resp) {
        if (resp && resp.status >= 400) {
          report("error", resp.status === 429 ? "429" : "http", resp.status);
        }
        return resp;
      }, function() {
        report("error", "network");
        return Promise.reject.apply(Promise, arguments);
      });
    };
  }

  // 3. Patch XMLHttpRequest to detect HTTP errors
  if (window.XMLHttpRequest) {
    var origOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function() {
      this.addEventListener("load", function() {
        if (this.status >= 400) {
          report("error", this.status === 429 ? "429" : "http", this.status);
        }
      });
      this.addEventListener("error", function() {
        report("error", "network");
      });
      return origOpen.apply(this, arguments);
    };
  }

  // 4. Watchdog: backstop for silent failures (success is implied by watchdog survival)
  setTimeout(function() { report("timeout"); }, WATCHDOG_MS);
  // Auto-report success slightly before the watchdog fires if no error was seen
  setTimeout(function() { report("success"); }, WATCHDOG_MS - 500);
})();
  `.trim();
}

/** Build renderExploreWidget JS calls. */
function buildTrendsSnippet(
  type: 'TIMESERIES' | 'GEO_MAP',
  keywords: string[],
  timeApi: string,
  queryDate: string
): string {
  const items = buildComparisonItems(keywords, timeApi);
  const exploreQuery = buildExploreQuery(type, keywords, queryDate);
  const guestPath = 'https://trends.google.com:443/trends/embed/';

  return `
<script type="text/javascript" src="https://ssl.gstatic.com/trends_nrtr/4448_RC01/embed_loader.js"></script>
<script type="text/javascript">
  trends.embed.renderExploreWidget(
    "${type}",
    {"comparisonItem":${items},"category":0,"property":""},
    {"exploreQuery":"${exploreQuery}","guestPath":"${guestPath}"}
  );
</script>
  `.trim();
}

/** Wrap script snippet in full isolated html document for srcdoc. */
export function buildWidgetSrcdoc(
  type: 'TIMESERIES' | 'GEO_MAP',
  keywords: string[],
  timeApi: string,
  queryDate: string
): string {
  const snippet = buildTrendsSnippet(type, keywords, timeApi, queryDate);
  const externalUrl = buildExternalExploreUrl(keywords, queryDate);
  const monitorScript = buildEmbedMonitorScript();
  return `
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <script>${monitorScript}</script>
  <style>
    html, body {
      margin: 0;
      padding: 0;
      height: 100%;
      overflow: hidden;
      background-color: #ffffff;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    }
    iframe {
      border: none !important;
      width: 100% !important;
      height: 100% !important;
    }
    .trends-fallback-action {
      position: absolute;
      top: 8px;
      right: 8px;
      z-index: 99999;
    }
    .trends-fallback-action button {
      display: inline-flex;
      align-items: center;
      background: rgba(255, 255, 255, 0.9);
      backdrop-filter: blur(4px);
      border: 1px solid #e2e8f0;
      border-radius: 4px;
      padding: 4px 8px;
      font-family: inherit;
      font-size: 11px;
      font-weight: 600;
      color: #4f46e5;
      cursor: pointer;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
      transition: all 0.2s ease;
    }
    .trends-fallback-action button:hover {
      background: #ffffff;
      border-color: #4f46e5;
      color: #4338ca;
      box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1);
      transform: translateY(-0.5px);
    }
    .trends-fallback-action button:active {
      transform: translateY(0);
    }
  </style>
</head>
<body>
  <div class="trends-fallback-action">
    <button onclick="window.parent.postMessage({ type: 'open_external_trends', url: '${externalUrl}' }, '*')" title="Open this search directly in Google Trends">
      <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" style="margin-right: 4px; display: inline-block; vertical-align: middle;">
        <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
        <polyline points="15 3 21 3 21 9"></polyline>
        <line x1="10" y1="14" x2="21" y2="3"></line>
      </svg>
      Open in Google Trends
    </button>
  </div>
  ${snippet}
</body>
</html>
  `.trim();
}

/** Build URL for opening Google Trends in a standard web browser. */
export function buildExternalExploreUrl(keywords: string[], queryDate: string): string {
  const sanitized = keywords.map(sanitizeKeyword).filter(Boolean);
  const q = sanitized.join(',');
  const encodedDate = encodeURIComponent(queryDate);
  const isCustomDate = /\d{4}-\d{2}-\d{2}/.test(queryDate);
  const legacyPart = isCustomDate ? '&legacy' : '';
  return `https://trends.google.com/trends/explore?date=${encodedDate}&q=${q}&hl=en${legacyPart}`;
}

/** Build embed URL for preflight HTTP probes. Mirrors
 * `trends.embed.renderExploreWidget`'s internal URL format:
 * `/trends/embed/explore/{TYPE}?req={json}&tz={tz}&eq={query}`. */
export function buildEmbedUrl(
  type: 'TIMESERIES' | 'GEO_MAP',
  keywords: string[],
  timeApi: string,
  queryDate: string,
  timezoneMinutes = new Date().getTimezoneOffset()
): string {
  const req = {
    comparisonItem: JSON.parse(buildComparisonItems(keywords, timeApi)),
    category: 0,
    property: '',
  };
  const eq = buildExploreQuery(type, keywords, queryDate);
  const base = 'https://trends.google.com/trends/embed/explore/';
  return `${base}${type}?req=${encodeURIComponent(JSON.stringify(req))}&tz=${-timezoneMinutes}&eq=${encodeURIComponent(eq)}`;
}

/** Random delay 2000-3999ms to serialize Google Trends embed requests under rate limit. */
export function nextRequestDelay(): number {
  return 2000 + Math.floor(Math.random() * 2000);
}
