/** Format LLM provider errors with troubleshooting links. */

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
  /** Inline solution text when matched, null otherwise */
  solution: string | null;
  /** Inline cause text when matched, null otherwise */
  cause: string | null;
}

/** Troubleshooting content for each known error pattern */
interface TroubleshootData {
  anchorId: string;
  cause: string;
  solution: string;
}

/** Known error patterns → troubleshooting data */
const ERROR_PATTERNS: Array<{ pattern: RegExp; data: TroubleshootData }> = [
  {
    pattern: /not supported for the API use/i,
    data: {
      anchorId: 'location-not-supported',
      cause:
        'Your IP is actively blocked by Google. Google blocks Gemini API access from your IP address location making the request. Google restricts API availability in certain regions for regulatory reasons. The FAILED_PRECONDITION status confirms your environment fails to meet service requirements. Common causes: the device is in an unsupported country, operates from a set of blocked IPs, a VPN routes traffic through a restricted region, or cloud infrastructure (AWS, GCP) operates in a restricted data center zone.',
      solution:
        'Ensure the machine executing the API call uses an IP address from a supported region. Verify the current list of allowed countries in the Google Gemini API documentation. Options: upgrade to a paid Gemini API key (pay-as-you-go), connect through a VPN in a supported region, move cloud workloads to a supported zone, or switch to a different provider such as OpenAI or Anthropic.',
    },
  },
  {
    pattern: /\b429\b|rate.?limit/i,
    data: {
      anchorId: 'rate-limited',
      cause:
        'You have exceeded the number of requests allowed per minute by your API plan. Free tiers have very low limits.',
      solution:
        'Reduce the concurrency setting and increase the request delay in Settings. Bango automatically retries with exponential backoff, but lowering throughput helps avoid hitting limits entirely.',
    },
  },
  {
    pattern:
      /\bAPI_KEY_INVALID\b|\bAPI key not valid\b.*\b400\b|\b400\b.*\bAPI_KEY_INVALID\b|\b400\b.*\bAPI key not valid\b/i,
    data: {
      anchorId: 'api-key-invalid-400',
      cause:
        'The API key is invalid, malformed, or has been revoked. Google Gemini returns HTTP 400 instead of the more common 401 for invalid keys.',
      solution:
        'Go to Settings and verify your API key is correct. For Google Gemini, ensure the key was generated from Google AI Studio and has not expired. If you recently regenerated the key, paste the new one. This error can also occur if the key belongs to a different Google Cloud project or service.',
    },
  },
  {
    pattern: /\b40[13]\b|authentication failed|unauthorized|forbidden/i,
    data: {
      anchorId: 'auth-failed',
      cause:
        'The API key is missing, incorrect, revoked, or does not have permission for the requested model.',
      solution:
        'Go to Settings and verify your API key is correct. Also check with your provider whether this key is still active. If you recently regenerated the key, paste the new one.',
    },
  },
  {
    pattern: /connection refused|timeout|ECONNREFUSED|fetch failed/i,
    data: {
      anchorId: 'connection-refused',
      cause: 'The local AI server is not running or the endpoint URL is wrong.',
      solution:
        'Make sure the local server is started (e.g., run `ollama serve`). Verify the endpoint URL and port in Settings match your server configuration.',
    },
  },
  {
    pattern: /\b404\b|model not found|not found/i,
    data: {
      anchorId: 'model-not-found',
      cause:
        'The model name in your configuration does not match any available model on the provider.',
      solution:
        'Check the model name for typos. Use the model picker in Settings to see available models for your provider.',
    },
  },
  {
    pattern: /token limit|context window|context.length|max_tokens/i,
    data: {
      anchorId: 'token-limit',
      cause:
        "The combined input (criteria + article text) and expected output exceed the model's maximum context window size.",
      solution:
        'Reduce the batch size in Settings, shorten your criteria text, or switch to a model with a larger context window (e.g., 128K or 200K token models).',
    },
  },
  {
    pattern: /malformed|invalid.*json|parse.*response/i,
    data: {
      anchorId: 'malformed-json',
      cause:
        'The AI returned an invalid or unexpected response format. This is more common with smaller or locally-run models.',
      solution:
        'Retry the individual article. If the problem persists, try a different model or a larger quantization.',
    },
  },
  {
    pattern: /api key not found|empty key|no api key/i,
    data: {
      anchorId: 'api-key-missing',
      cause: 'No API key has been entered in the Settings screen.',
      solution:
        'Go to Settings, select your provider, and enter a valid API key. Click Save to persist the configuration.',
    },
  },
  {
    pattern: /ssl|tls|certificate/i,
    data: {
      anchorId: 'ssl-error',
      cause:
        'The endpoint is using a self-signed or expired SSL certificate, which the app cannot verify.',
      solution:
        'For local providers, use `http://localhost` instead of `https://localhost`. For remote self-hosted servers, ensure a valid certificate is installed.',
    },
  },
  {
    pattern: /out of memory|oom|cuda.*memory/i,
    data: {
      anchorId: 'out-of-memory',
      cause: 'The model requires more RAM or VRAM than is available on your system.',
      solution:
        'Use a smaller model or a more aggressive quantization (Q4_K_M instead of Q8). Close other applications. For GPU-based inference, ensure your GPU has enough dedicated VRAM.',
    },
  },
];

const MATCHED_PREFIX =
  'This is not a Bango error or bug, but a response from your chosen LLM provider. ' +
  'A suggested resolution is shown below.';

const UNMATCHED_PREFIX = 'This is a response from your LLM provider.';

const TROUBLESHOOT_BASE = '/#/help?tab=troubleshoot';

/** Analyse LLM error message and return structured info for display. */
export function formatLlmError(rawMessage: string): LlmErrorInfo {
  for (const { pattern, data } of ERROR_PATTERNS) {
    if (pattern.test(rawMessage)) {
      return {
        matched: true,
        prefix: MATCHED_PREFIX,
        details: rawMessage,
        helpLink: `${TROUBLESHOOT_BASE}#${data.anchorId}`,
        anchorId: data.anchorId,
        solution: data.solution,
        cause: data.cause,
      };
    }
  }

  return {
    matched: false,
    prefix: UNMATCHED_PREFIX,
    details: rawMessage,
    helpLink: TROUBLESHOOT_BASE,
    anchorId: null,
    solution: null,
    cause: null,
  };
}
