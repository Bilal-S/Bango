import { describe, it, expect } from 'vitest';
import {
  sanitizeKeyword,
  clampToFiveYearWindow,
  isValidCustomRange,
  buildResearchRange,
  buildComparisonItems,
  buildExploreQuery,
  buildExternalExploreUrl,
  buildEmbedMonitorScript,
  buildWidgetSrcdoc,
  buildEmbedUrl,
  nextRequestDelay,
} from '../utils/google-trends';

describe('Google Trends Utilities', () => {
  describe('sanitizeKeyword', () => {
    it('should trim whitespace', () => {
      expect(sanitizeKeyword('  machine learning  ')).toBe('machine learning');
    });

    it('should strip trailing punctuation', () => {
      expect(sanitizeKeyword('deep learning.')).toBe('deep learning');
      expect(sanitizeKeyword('artificial intelligence!')).toBe('artificial intelligence');
      expect(sanitizeKeyword('nlp,')).toBe('nlp');
      expect(sanitizeKeyword('transformers;')).toBe('transformers');
    });

    it('should strip parenthetical text', () => {
      expect(sanitizeKeyword('machine learning (ml)')).toBe('machine learning');
      expect(sanitizeKeyword('natural language processing (NLP) research')).toBe(
        'natural language processing research'
      );
      expect(sanitizeKeyword('image recognition [computer vision]')).toBe('image recognition');
    });
  });

  describe('clampToFiveYearWindow', () => {
    it('should not clamp ranges under 5 years', () => {
      const result = clampToFiveYearWindow('2020-01-01', '2023-01-01');
      expect(result.start).toBe('2020-01-01');
      expect(result.end).toBe('2023-01-01');
      expect(result.clamped).toBe(false);
    });

    it('should clamp ranges over 5 years by sliding start forward', () => {
      const result = clampToFiveYearWindow('2010-01-01', '2020-01-01');
      // end - 5 years (1825 days)
      expect(result.end).toBe('2020-01-01');
      expect(result.start).toBe('2015-01-02'); // 1825 days before Jan 1, 2020 is Jan 2, 2015 in UTC (accounting for leap years)
      expect(result.clamped).toBe(true);
    });
  });

  describe('isValidCustomRange', () => {
    it('should return true for valid custom ranges under 5 years', () => {
      const result = isValidCustomRange('2022-01-01', '2024-01-01');
      expect(result.ok).toBe(true);
    });

    it('should reject empty dates', () => {
      expect(isValidCustomRange('', '2023-01-01').ok).toBe(false);
      expect(isValidCustomRange('2023-01-01', '').ok).toBe(false);
    });

    it('should reject start date after end date', () => {
      const result = isValidCustomRange('2024-01-01', '2022-01-01');
      expect(result.ok).toBe(false);
      expect(result.reason).toContain('Start date must be before');
    });

    it('should reject end dates in the future', () => {
      const result = isValidCustomRange('2023-01-01', '2030-01-01');
      expect(result.ok).toBe(false);
      expect(result.reason).toContain('cannot be in the future');
    });

    it('should reject ranges exceeding 5 years', () => {
      const result = isValidCustomRange('2010-01-01', '2018-01-01');
      expect(result.ok).toBe(false);
      expect(result.reason).toContain('cannot exceed 5 years');
    });
  });

  describe('buildResearchRange', () => {
    const mockToday = new Date('2026-06-14T12:00:00Z');

    it('Rule 1: should use full publication range if minYear > 2001', () => {
      const range = buildResearchRange(2010, 2018, 2014, mockToday);
      expect(range.start).toBe('2010-01-01');
      expect(range.end).toBe('2018-12-31');
    });

    it('Rule 1: should clamp end date to today if maxYear is in the future', () => {
      const range = buildResearchRange(2022, 2030, 2024, mockToday);
      expect(range.start).toBe('2022-01-01');
      expect(range.end).toBe('2026-06-14');
    });

    it('Rule 2: should center 5-year window around peak if minYear <= 2001 and peak > 2001', () => {
      // minYear = 1999, peak = 2008. Window centered: 2006 to 2010 (5 years)
      const range = buildResearchRange(1999, 2015, 2008, mockToday);
      expect(range.start).toBe('2006-01-01');
      expect(range.end).toBe('2010-12-31');
    });

    it('Rule 2: should clamp start to 2002 if centered window slips pre-2002', () => {
      // minYear = 1995, peak = 2002. Center window: 2000 to 2004, but must start >= 2002.
      // So starts at 2002-01-01, ends at 2006-12-31
      const range = buildResearchRange(1995, 2010, 2002, mockToday);
      expect(range.start).toBe('2002-01-01');
      expect(range.end).toBe('2006-12-31');
    });

    it('Rule 2: should clamp end to today if centered window slips into the future', () => {
      // minYear = 1998, peak = 2025. Center window: 2023 to 2027, but end cannot exceed today (2026-06-14)
      const range = buildResearchRange(1998, 2028, 2025, mockToday);
      expect(range.start).toBe('2022-01-01'); // shifts start back to maintain 5 years: 2022-2026 (approx)
      expect(range.end).toBe('2026-06-14');
    });

    it('Rule 3: should fallback to last 5 years if peak and minYear <= 2001', () => {
      // Entire dataset pre-2002: minYear = 1990, peak = 1998
      const range = buildResearchRange(1990, 2000, 1998, mockToday);
      // today is 2026-06-14, 5 years before is 2021-06-14 (roughly)
      expect(range.end).toBe('2026-06-14');
      const startYear = parseInt(range.start.split('-')[0] || '0', 10);
      expect(startYear).toBe(2021);
    });
  });

  describe('buildComparisonItems & buildExploreQuery', () => {
    it('should build correctly formatted comparison items', () => {
      const result = JSON.parse(buildComparisonItems(['A (ai)', 'B!'], 'today 5-y'));
      expect(result).toHaveLength(2);
      expect(result[0].keyword).toBe('A');
      expect(result[0].time).toBe('today 5-y');
      expect(result[1].keyword).toBe('B');
    });

    it('should build correctly url-encoded explore queries', () => {
      // TIMESERIES with presets
      const queryTS = buildExploreQuery('TIMESERIES', ['A (ai)', 'B!'], 'today 5-y');
      expect(queryTS).toBe('date=today%205-y&q=A,B&hl=en-US');

      // TIMESERIES with custom date (should add legacy)
      const queryTSCustom = buildExploreQuery(
        'TIMESERIES',
        ['A (ai)', 'B!'],
        '2026-05-14 2026-06-14'
      );
      expect(queryTSCustom).toBe('date=2026-05-14%202026-06-14&q=A,B&hl=en-US&legacy');

      // GEO_MAP with 'now' preset (single date)
      const queryMapNow = buildExploreQuery('GEO_MAP', ['A (ai)', 'B!'], 'now 1-d');
      expect(queryMapNow).toBe('q=A,B&hl=en&legacy&date=now%201-d');

      // GEO_MAP with non-'now' preset (repeated dates for multiple keywords)
      const queryMapPreset = buildExploreQuery('GEO_MAP', ['A (ai)', 'B!'], 'today 12-m');
      expect(queryMapPreset).toBe('q=A,B&hl=en&legacy&date=today%2012-m,today%2012-m');
    });

    it('should build correctly formatted external explore URLs', () => {
      const urlPreset = buildExternalExploreUrl(['products', 'sweetening'], 'today 5-y');
      expect(urlPreset).toBe(
        'https://trends.google.com/trends/explore?date=today%205-y&q=products,sweetening&hl=en'
      );

      const urlCustom = buildExternalExploreUrl(['unicor', 'walruses'], '2012-05-14 2022-06-14');
      expect(urlCustom).toBe(
        'https://trends.google.com/trends/explore?date=2012-05-14%202022-06-14&q=unicor,walruses&hl=en&legacy'
      );
    });
  });

  describe('buildEmbedMonitorScript', () => {
    it('should post a status message to the parent via postMessage', () => {
      const script = buildEmbedMonitorScript();
      expect(script).toContain('trends_embed_status');
      expect(script).toContain('window.parent.postMessage');
    });

    it('should include all detection mechanisms', () => {
      const script = buildEmbedMonitorScript();
      // 1. Capture-phase error listener for failed script loads
      expect(script).toMatch(/addEventListener\(\s*["']error["']/);
      // 2. fetch patch for HTTP errors
      expect(script).toContain('window.fetch');
      // 3. XMLHttpRequest patch for HTTP errors
      expect(script).toContain('XMLHttpRequest');
      // 4. Watchdog timeout backstop (no more MutationObserver - see docstring)
      expect(script).toMatch(/setTimeout/);
    });

    it('should use a settled flag to prevent multiple reports', () => {
      const script = buildEmbedMonitorScript();
      expect(script).toContain('settled');
      expect(script).toMatch(/if\s*\(settled\)\s*return/);
    });

    it('should distinguish 429 rate-limit from generic http errors', () => {
      const script = buildEmbedMonitorScript();
      expect(script).toContain('429');
      expect(script).toContain('"429"');
      expect(script).toContain('"http"');
    });

    it('should report network errors for failed script loads', () => {
      const script = buildEmbedMonitorScript();
      expect(script).toContain('"network"');
      // Capture phase (third arg = true) so we catch resource load errors
      expect(script).toMatch(/,\s*true\s*\)/);
    });
  });

  describe('buildEmbedUrl', () => {
    it('should build a preflight URL with encoded req and eq params', () => {
      const url = buildEmbedUrl(
        'TIMESERIES',
        ['ai', 'ml'],
        'today 5-y',
        'today 5-y',
        300 // UTC-5:00 (300 minutes)
      );
      expect(url).toContain('https://trends.google.com/trends/embed/explore/TIMESERIES');
      expect(url).toContain('tz=-300');
      expect(url).toContain('req=');
      expect(url).toContain('eq=');
    });

    it('should embed the comparisonItem in the req parameter as JSON', () => {
      const url = buildEmbedUrl('GEO_MAP', ['cats'], 'now 7-d', 'now 7-d');
      const decoded = decodeURIComponent(url);
      expect(decoded).toContain('"keyword":"cats"');
      expect(decoded).toContain('"time":"now 7-d"');
      expect(decoded).toContain('"geo":""');
    });
  });

  describe('nextRequestDelay', () => {
    it('should return a delay between 2000 and 3999ms', () => {
      for (let i = 0; i < 50; i++) {
        const d = nextRequestDelay();
        expect(d).toBeGreaterThanOrEqual(2000);
        expect(d).toBeLessThan(4000);
      }
    });
  });

  describe('buildWidgetSrcdoc', () => {
    it('should embed the monitor script at the top of the document head', () => {
      const doc = buildWidgetSrcdoc('TIMESERIES', ['ai'], 'today 5-y', 'today 5-y');
      // Monitor script appears before the trends loader script
      const monitorIdx = doc.indexOf('trends_embed_status');
      const loaderIdx = doc.indexOf('embed_loader.js');
      expect(monitorIdx).toBeGreaterThan(-1);
      expect(loaderIdx).toBeGreaterThan(-1);
      expect(monitorIdx).toBeLessThan(loaderIdx);
    });

    it('should include both TIMESERIES and GEO_MAP widget types', () => {
      const chartDoc = buildWidgetSrcdoc('TIMESERIES', ['ai'], 'today 5-y', 'today 5-y');
      expect(chartDoc).toContain('TIMESERIES');

      const mapDoc = buildWidgetSrcdoc('GEO_MAP', ['ai'], 'today 5-y', 'today 5-y');
      expect(mapDoc).toContain('GEO_MAP');
    });

    it('should include an open-external fallback button with the correct URL', () => {
      const doc = buildWidgetSrcdoc('TIMESERIES', ['ai'], 'today 5-y', 'today 5-y');
      expect(doc).toContain('open_external_trends');
      expect(doc).toContain('https://trends.google.com');
    });
  });
});
