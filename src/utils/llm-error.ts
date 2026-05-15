/**
 * Utility for formatting LLM provider errors with troubleshooting links.
 */

export interface LlmErrorInfo {
  /** True when we matched a known error pattern */
  matched: boolean;
  /** Human-readable prefix explaining this is a provider error */
  prefix: string;
  /** The original raw error */
  details: string;
  /** Suggested troubleshooting link (always present) */
  helpLink: string;
  /** Anchor ID if matched, null otherwise */
  anchorId: string | null;
}

/** Known error patterns → troubleshooting anchor IDs */
const ERROR_PATTERNS: Array<{ pattern: RegExp; anchorId: string }> = [
  { pattern: /not supported for the API use/i, anchorId: 'location-not-supported' },
  { pattern: /\b429\b|rate.?limit/i, anchorId: 'rate-limited' },
  { pattern: /\b40[13]\b|authentication failed|unauthorized|forbidden/i, anchorId: 'auth-failed' },
  {
    pattern: /connection refused|timeout|ECONNREFUSED|fetch failed/i,
    anchorId: 'connection-refused',
  },
  { pattern: /\b404\b|model not found|not found/i, anchorId: 'model-not-found' },
  { pattern: /token limit|context window|context.length|max_tokens/i, anchorId: 'token-limit' },
  { pattern: /malformed|invalid.*json|parse.*response/i, anchorId: 'malformed-json' },
  { pattern: /api key not found|empty key|no api key/i, anchorId: 'api-key-missing' },
  { pattern: /ssl|tls|certificate/i, anchorId: 'ssl-error' },
  { pattern: /out of memory|oom|cuda.*memory/i, anchorId: 'out-of-memory' },
];

const MATCHED_PREFIX =
  'This is not a Bango error or bug, but a response from your chosen LLM provider. ' +
  'Please consult our troubleshooting guide on how to resolve the problem.';

const UNMATCHED_PREFIX = 'This is a response from your LLM provider.';

const TROUBLESHOOT_BASE = '/#/help?tab=troubleshoot';

/**
 * Analyse an LLM error message and return structured info for display.
 */
export function formatLlmError(rawMessage: string): LlmErrorInfo {
  for (const { pattern, anchorId } of ERROR_PATTERNS) {
    if (pattern.test(rawMessage)) {
      return {
        matched: true,
        prefix: MATCHED_PREFIX,
        details: rawMessage,
        helpLink: `${TROUBLESHOOT_BASE}#${anchorId}`,
        anchorId,
      };
    }
  }

  return {
    matched: false,
    prefix: UNMATCHED_PREFIX,
    details: rawMessage,
    helpLink: TROUBLESHOOT_BASE,
    anchorId: null,
  };
}
