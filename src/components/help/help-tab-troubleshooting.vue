<script setup lang="ts">
import '@/styles/help-shared.css';

interface TroubleshootItem {
  anchorId: string;
  icon: string;
  error: string;
  providers: string;
  cause: string;
  solution: string;
}

const troubleshootItems: TroubleshootItem[] = [
  {
    anchorId: 'location-not-supported',
    icon: 'public',
    error: 'User location is not supported for the API use',
    providers: 'Google Gemini',
    cause:
      'Google blocks Gemini API access from the IP address location making the request. Google restricts API availability in certain regions for regulatory reasons. The FAILED_PRECONDITION status confirms your environment fails to meet service requirements. Common causes: the device is in an unsupported country, a VPN routes traffic through a restricted region, or cloud infrastructure (AWS, GCP) operates in a restricted data center zone.',
    solution:
      'Ensure the machine executing the API call uses an IP address from a supported region. Verify the current list of allowed countries in the Google Gemini API documentation. Options: upgrade to a paid Gemini API key (pay-as-you-go), connect through a VPN in a supported region, move cloud workloads to a supported zone, or switch to a different provider such as OpenAI or Anthropic.',
  },
  {
    anchorId: 'rate-limited',
    icon: 'speed',
    error: 'Rate limited (HTTP 429)',
    providers: 'All cloud providers',
    cause:
      'You have exceeded the number of requests allowed per minute by your API plan. Free tiers have very low limits.',
    solution:
      'Reduce the concurrency setting and increase the request delay in Settings. Bango automatically retries with exponential backoff, but lowering throughput helps avoid hitting limits entirely. Alternatly, you can purchase higher limits with your provider. Please note some provider (Z.AI) also issue this message when you are using the wrong endpoint (Base URL). Please ensure you use the correct Base URL for your subscription. Bango prefills the most common one, yours might be different.',
  },
  {
    anchorId: 'api-key-invalid-400',
    icon: 'key_off',
    error: 'API key not valid (HTTP 400 Bad Request)',
    providers: 'Google Gemini',
    cause:
      'The API key is invalid, malformed, or has been revoked. Google Gemini returns HTTP 400 instead of the more common 401 for invalid keys.',
    solution:
      'Go to Settings and verify your API key is correct. For Google Gemini, ensure the key was generated from Google AI Studio and has not expired. If you recently regenerated the key, paste the new one. This error can also occur if the key belongs to a different Google Cloud project or service.',
  },
  {
    anchorId: 'auth-failed',
    icon: 'key',
    error: 'Authentication failed (HTTP 401 / 403)',
    providers: 'All providers',
    cause:
      'The API key is missing, incorrect, revoked, or does not have permission for the requested model.',
    solution:
      'Go to Settings and verify your API key is correct. Also check with your provider whether this key is still active. If you recently regenerated the key, paste the new one. Check that your account has access to the model you selected.',
  },
  {
    anchorId: 'connection-refused',
    icon: 'wifi_off',
    error: 'Connection refused / timeout',
    providers: 'Ollama, llama.cpp, LM Studio',
    cause: 'The local AI server is not running or the endpoint URL is wrong.',
    solution:
      'Make sure the local server is started (e.g., run `ollama serve`). Verify the endpoint URL and port in Settings match your server configuration. Check that no firewall is blocking the connection.',
  },
  {
    anchorId: 'model-not-found',
    icon: 'search_off',
    error: 'Model not found (HTTP 404)',
    providers: 'All providers',
    cause:
      'The model name in your configuration does not match any available model on the provider.',
    solution:
      'Check the model name for typos. Use the model picker in Settings to see available models for your provider. Model names change over time - make sure you are using the current identifier. For local AI check your tools documentation on how to provide the name and where you can look it up.',
  },
  {
    anchorId: 'token-limit',
    icon: 'data_array',
    error: 'Token / context window limit exceeded',
    providers: 'All providers',
    cause:
      "The combined input (criteria + article text) and expected output exceed the model's maximum context window size.",
    solution:
      'Reduce the batch size in Settings, shorten your criteria text, or switch to a model with a larger context window (e.g., 128K or 200K token models).',
  },
  {
    anchorId: 'malformed-json',
    icon: 'broken_image',
    error: 'Malformed JSON / screening errors',
    providers: 'All providers',
    cause:
      'The AI returned an invalid or unexpected response format. This is more common with smaller or locally-run models.',
    solution:
      'Retry the individual article. If the problem persists, try a different model or a larger quantization. Cloud providers (GPT-4, Claude) produce more reliable structured output.',
  },
  {
    anchorId: 'api-key-missing',
    icon: 'vpn_key_off',
    error: 'API key not found / empty key',
    providers: 'All providers',
    cause: 'No API key has been entered in the Settings screen.',
    solution:
      'Go to Settings, select your provider, and enter a valid API key. Click Save to persist the configuration.',
  },
  {
    anchorId: 'ssl-error',
    icon: 'lock',
    error: 'SSL / TLS certificate error',
    providers: 'Self-hosted endpoints',
    cause:
      'The endpoint is using a self-signed or expired SSL certificate, which the app cannot verify.',
    solution:
      'For local providers, use `http://localhost` instead of `https://localhost`. For remote self-hosted servers, ensure a valid certificate is installed.',
  },
  {
    anchorId: 'slow-inference',
    icon: 'hourglass_empty',
    error: 'Slow inference / timeouts',
    providers: 'Local providers',
    cause:
      'Local models run on your hardware and may be slow, especially with large context windows or limited RAM/VRAM.',
    solution:
      'Reduce the context window setting, use a smaller quantized model (Q4_K_M), lower concurrency to 1, and close other applications to free memory.',
  },
  {
    anchorId: 'out-of-memory',
    icon: 'memory',
    error: 'Out of memory (OOM)',
    providers: 'Local providers',
    cause: 'The model requires more RAM or VRAM than is available on your system.',
    solution:
      'Use a smaller model or a more aggressive quantization (Q4_K_M instead of Q8). Close other applications. For GPU-based inference, ensure your GPU has enough dedicated VRAM.',
  },
];
</script>

<template>
  <div class="ht-troubleshoot" role="tabpanel">
    <!-- Intro -->
    <section class="ht-intro">
      <h2 class="ht-intro__title">Common Errors & Solutions</h2>
      <p class="ht-intro__desc">
        If you are seeing an error while using Bango, check the list below. Each entry shows the
        error message you might encounter, what causes it, and what you can do to fix it.
      </p>
    </section>

    <!-- Cards -->
    <div class="ts-list">
      <div v-for="(item, idx) in troubleshootItems" :id="item.anchorId" :key="idx" class="ts-card">
        <div class="ts-card__header">
          <span class="material-symbols-outlined ts-card__icon">{{ item.icon }}</span>
          <div class="ts-card__header-text">
            <h3 class="ts-card__error">{{ item.error }}</h3>
            <span class="ts-card__providers">{{ item.providers }}</span>
          </div>
        </div>
        <div class="ts-card__body">
          <div class="ts-card__field"><strong>Cause:</strong> {{ item.cause }}</div>
          <div class="ts-card__field"><strong>Solution:</strong> {{ item.solution }}</div>
        </div>
      </div>
    </div>

    <!-- Still stuck? -->
    <section class="ht-about">
      <div class="ht-about-card">
        <span class="material-symbols-outlined ht-about-icon">info</span>
        <div class="ht-about-body">
          <h4 class="ht-about-title">Still stuck?</h4>
          <p class="ht-about-desc">
            If your issue is not listed above, please open an issue on GitHub and the community will
            help you out. Include the error message, your provider, and what you were doing when it
            happened.
          </p>
          <ul class="ht-about-links">
            <li>
              <span class="material-symbols-outlined ht-about-link-icon">bug_report</span>
              <a
                class="ht-about-link"
                href="https://github.com/Bilal-S/Bango/issues"
                target="_blank"
                rel="noopener noreferrer"
              >
                Report Issues & Get Support
              </a>
            </li>
          </ul>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.ht-troubleshoot {
  /* Container; uses shared .ht-intro and .ht-about classes */
}

.ts-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.ts-card {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
  box-shadow: var(--shadow-sm);
  scroll-margin-top: var(--space-4);
}

.ts-card__header {
  display: flex;
  gap: var(--space-3);
  align-items: flex-start;
  margin-bottom: var(--space-3);
  padding-bottom: var(--space-3);
  border-bottom: 1px solid var(--color-border);
}

.ts-card__icon {
  font-size: 24px;
  color: #dc2626;
  flex-shrink: 0;
}

.ts-card__header-text {
  flex: 1;
}

.ts-card__error {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-1) 0;
}

.ts-card__providers {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  font-weight: var(--font-weight-semibold);
}

.ts-card__body {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
}

.ts-card__field {
  margin-bottom: var(--space-2);
}

.ts-card__field:last-child {
  margin-bottom: 0;
}

.ts-card__field strong {
  color: var(--color-on-surface);
}
</style>
