<script setup lang="ts">
import { onMounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import '@/styles/help-shared.css';

const router = useRouter();

const props = defineProps<{
  initialHash?: string;
}>();

/**
 * Deep-link scroll helper: scrolls the target section (by id, without the leading `#`)
 * into view. Wrapped in `requestAnimationFrame` so the just-mounted tab content has
 * painted before we measure offsets.
 */
function scrollToHash(hash: string): void {
  const id = hash.startsWith('#') ? hash.slice(1) : hash;
  if (!id) return;
  requestAnimationFrame(() => {
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  });
}

// Scroll on mount when arriving with a hash (e.g. /help?tab=local-ai#free-ai).
onMounted(() => {
  if (props.initialHash) {
    scrollToHash(props.initialHash);
  }
});

// Re-scroll when the hash changes while the tab is already mounted
// (e.g. clicking the Settings link a second time with a different provider).
watch(
  () => props.initialHash,
  (newHash) => {
    if (newHash) scrollToHash(newHash);
  }
);

interface LocalAIStep {
  text: string;
  code?: string;
}

interface LocalAIGuide {
  icon: string;
  title: string;
  intro?: string;
  steps?: LocalAIStep[];
  tip?: string;
}

interface FreeAiProvider {
  icon: string;
  name: string;
  /** `ongoing` = always-on free tier; `trial` = one-time / expiring credit. */
  tier: 'ongoing' | 'trial';
  tierLabel: string;
  blurb: string;
  details: string[];
  models: string;
  link: { label: string; url: string };
}

/**
 * Cloud LLM providers that offer a free or trial on-ramp. Each of these maps
 * 1:1 to a built-in provider option in Settings (`settings-provider-card.vue`).
 */
const freeAiProviders: FreeAiProvider[] = [
  {
    icon: 'auto_awesome',
    name: 'Google AI Studio (Gemini)',
    tier: 'ongoing',
    tierLabel: 'Ongoing free tier · Never expires',
    blurb:
      'Rate-limited access to the Gemini family (including Gemini Flash and Pro models) through Google AI Studio. The Gemini models come with very large context windows well-suited to Bango.',
    details: [
      'Go to Google AI Studio and sign in with a Google account.',
      'In the top bar (or left sidebar) click "Get API key" → "Create API key".',
      'Copy the generated key (it starts with AIza...).',
      'Free-tier requests may be used by Google to improve products; switch to a paid billing plan if you need a no-training data guarantee.',
    ],
    models:
      'Recommended for Bango: gemini-3.1-flash or gemini-3.5-flash (fast, cheap, large context). Use pro models for harder reasoning at lower throughput.',
    link: { label: 'aistudio.google.com', url: 'https://aistudio.google.com' },
  },
  {
    icon: 'bolt',
    name: 'Mistral AI (La Plateforme)',
    tier: 'ongoing',
    tierLabel: 'Ongoing free tier (Experiment) · Never expires',
    blurb:
      'The Experiment tier gives rate-limited access to all Mistral models, including the open-weight and "open-mistral-" series. A solid always-on option with no expiry.',
    details: [
      'Create an account at console.mistral.ai (La Plateforme).',
      'Navigate to API Keys and generate a new key.',
      'Copy the key.',
      'The Experiment tier is intended for prototyping; throughput (requests per minute) is capped and may change over time.',
    ],
    models:
      'Recommended for Bango: open-mistral-nemo or mistral-small-latest. Both have a context window large enough for abstract screening.',
    link: { label: 'console.mistral.ai', url: 'https://console.mistral.ai' },
  },
  {
    icon: 'flash_on',
    name: 'Z.AI (GLM)',
    tier: 'ongoing',
    tierLabel: 'Ongoing free tier · Never expires',
    blurb:
      'Z.AI offers free access to specific lightweight GLM models (such as GLM-4.6-Flash) via an OpenAI-compatible API. A good choice for cost-free screening runs.',
    details: [
      'Sign up at z.ai and verify your account.',
      'Open the API / API Keys section of the console and create a key.',
      'Copy the key.',
      'In Bango, choose the built-in "z.ai" provider; only the whitelisted free models are complimentary.',
    ],
    models:
      'Recommended for Bango: glm-4.6-flash (free). Confirm the current free-model list in the Z.AI console before large runs.',
    link: { label: 'z.ai', url: 'https://z.ai' },
  },
  {
    icon: 'redeem',
    name: 'OpenAI',
    tier: 'trial',
    tierLabel: 'One-time sign-up credit · Expires in ~3 months',
    blurb:
      'New OpenAI accounts receive a small one-time credit (about $5) that is valid for roughly three months. Useful for trying Bango end-to-end, but not a long-term free option.',
    details: [
      'Create an account at platform.openai.com and add a phone number for verification.',
      'The sign-up credit appears on the Billing → Usage page.',
      'Generate an API key under API Keys → Create new secret key.',
      'Once the credit is spent or expires, you are billed per token. Set a monthly spending limit if you keep the key active.',
    ],
    models:
      'Recommended for Bango: gpt-5-mini (cheapest). Use gpt-5.6-luna for higher-quality reasoning when budget allows.',
    link: { label: 'platform.openai.com', url: 'https://platform.openai.com' },
  },
  {
    icon: 'redeem',
    name: 'Anthropic',
    tier: 'trial',
    tierLabel: 'One-time sign-up credit · Limited time',
    blurb:
      'New Anthropic console accounts occasionally receive a small promotional credit (about $5) valid for a limited time. Handy for an initial screening run, after which billing applies.',
    details: [
      'Create an account at console.anthropic.com and verify your phone number.',
      'Check the Billing / Credits section for any promotional balance.',
      'Generate an API key under API Keys → Create Key.',
      'Promotional credit availability and expiry vary by region and over time; confirm on the Billing page.',
    ],
    models:
      'Recommended for Bango: claude-haiku-latest (cheapest, fast). Use a claude-sonnet variant for stronger reasoning.',
    link: { label: 'console.anthropic.com', url: 'https://console.anthropic.com' },
  },
];

const localAiGuides: LocalAIGuide[] = [
  {
    icon: 'shield',
    title: 'Why Run LLMs Locally?',
    intro:
      'Running models on your own hardware means your data never leaves your machine - full privacy, no API fees, offline capability, and predictable performance. Tools like Ollama and llama.cpp make it straightforward to deploy models on laptops, workstations, or servers.',
  },
  {
    icon: 'memory',
    title: 'Hardware Requirements',
    intro:
      'Bango sends article abstracts and criteria together, so models supporting 50K+ token context windows are strongly recommended.',
    steps: [
      { text: 'Minimum: 16 GB RAM for small models (e.g., Phi-3 Mini 3.8B at 5-10 tokens/sec).' },
      {
        text: 'Recommended: Apple Silicon Mac (M1/M2+) or a PC with a dedicated GPU (8-16 GB VRAM) for 7B+ parameter models.',
      },
      {
        text: 'For 50K+ token context, you typically need 16 GB+ VRAM or unified memory.',
      },
    ],
  },
  {
    icon: 'rocket_launch',
    title: 'Ollama Setup (Recommended)',
    intro:
      'Ollama is the easiest way to get started. It handles model management automatically and provides a simple interface.',
    steps: [
      {
        text: 'Install Ollama.',
        code: 'curl -fsSL https://ollama.com/install.sh | sh',
      },
      {
        text: 'Pull a model.',
        code: 'ollama pull llama3',
      },
      {
        text: 'Start the server.',
        code: 'ollama serve',
      },
      {
        text: 'In Bango Settings, set the provider to "Ollama" and the endpoint URL to:',
        code: 'http://localhost:11434/v1',
      },
    ],
    tip: 'On Windows, download the installer from ollama.com instead of using curl.',
  },
  {
    icon: 'terminal',
    title: 'LM Studio Setup',
    intro: 'LM Studio provides a graphical interface for downloading and running models.',
    steps: [
      {
        text: 'Download LM Studio from lmstudio.ai and install it.',
      },
      {
        text: 'Browse and download a model (e.g., Llama 3 8B Instruct).',
      },
      {
        text: 'Go to the Local Server tab and click Start Server.',
      },
      {
        text: 'In Bango Settings, set the provider to "LM Studio" and the endpoint URL to:',
        code: 'http://localhost:1234/v1',
      },
    ],
  },
  {
    icon: 'code',
    title: 'llama.cpp Setup (Advanced)',
    intro:
      'llama.cpp offers maximum flexibility and performance with advanced quantization options. It runs on a wide variety of hardware and gives you fine-grained control over inference parameters.',
    steps: [
      {
        text: 'Build from source or download a pre-built binary from github.com/ggerganov/llama.cpp.',
      },
      {
        text: 'Run the server with your model.',
        code: './llama-server -m model.gguf --port 8080',
      },
      {
        text: 'In Bango Settings, set the provider to "llama.cpp" and the endpoint URL to:',
        code: 'http://localhost:8080/v1',
      },
    ],
  },
  {
    icon: 'model_training',
    title: 'Recommended Models',
    intro:
      'Choose models that support 50K+ token context for Bango screening. Good options: Llama 3 (8B or 70B), Mistral (7B), Qwen 2.5 (7B or 14B), and Phi-3 Medium.',
    tip: 'Use quantized versions (Q4_K_M or Q5_K_M) for lower memory usage with minimal quality loss. Quantization reduces model precision (16-bit to 4-bit) to shrink memory requirements - Q4_K_M is roughly 40% of the original size and Q5_K_M about 50%.',
  },
  {
    icon: 'tune',
    title: 'Configuration Tips for Bango',
    steps: [
      { text: 'Temperature: 0.2 (default, good for analytical tasks).' },
      { text: 'Concurrency: 1 (local models handle one request at a time).' },
      { text: 'Request delay: 1000ms+ to give the model time between requests.' },
      {
        text: "Context window: set this to match your model's actual capability - do not set it higher than what the model supports.",
      },
      {
        text: 'If you get truncated responses, lower the context window or switch to a model with a larger window.',
      },
    ],
  },
];

function copyCode(code: string): void {
  navigator.clipboard.writeText(code).catch(() => {
    /* silently fail in restricted contexts */
  });
}

function navigateTo(route: string): void {
  router.push(route);
}
</script>

<template>
  <div class="ht-local-ai" role="tabpanel">
    <!-- Top-level intro -->
    <section class="ht-intro">
      <h2 class="ht-intro__title">Local & Free AI</h2>
      <p class="ht-intro__desc">
        Bango can use either a <strong>cloud LLM provider</strong> or a
        <strong>model running on your own hardware</strong>. This page covers both options: free
        cloud API keys you can get in minutes, and a complete guide to running models locally for
        full privacy and zero per-request cost.
      </p>
    </section>

    <!-- ============================================================ -->
    <!-- SECTION 1: FREE AI (cloud providers with free options)        -->
    <!-- ============================================================ -->
    <section id="free-ai" class="lai-section">
      <header class="lai-section__header">
        <span class="material-symbols-outlined lai-section__icon">cloud</span>
        <div>
          <h3 class="lai-section__title">Free AI</h3>
          <p class="lai-section__subtitle">
            Get started in minutes with a free API key from a cloud provider. All five providers
            below are built into Bango's provider list - no custom endpoints to configure.
          </p>
        </div>
      </header>

      <div class="lai-list">
        <div v-for="p in freeAiProviders" :key="p.name" class="lai-card">
          <div class="lai-card__header">
            <span class="material-symbols-outlined lai-card__icon">{{ p.icon }}</span>
            <h3 class="lai-card__title">{{ p.name }}</h3>
            <span
              class="lai-badge"
              :class="p.tier === 'ongoing' ? 'lai-badge--ongoing' : 'lai-badge--trial'"
            >
              {{ p.tierLabel }}
            </span>
          </div>
          <p class="lai-card__intro">{{ p.blurb }}</p>
          <ul class="lai-card__points">
            <li v-for="(d, i) in p.details" :key="i">{{ d }}</li>
          </ul>
          <p class="lai-card__models">
            <span class="material-symbols-outlined lai-card__models-icon">model_training</span>
            {{ p.models }}
          </p>
          <a class="lai-card__link" :href="p.link.url" target="_blank" rel="noopener noreferrer">
            <span class="material-symbols-outlined">open_in_new</span>
            {{ p.link.label }}
          </a>
        </div>
      </div>

      <!-- Comparison table -->
      <div class="lai-table-wrapper">
        <table class="lai-table">
          <thead>
            <tr>
              <th>Provider</th>
              <th>Ongoing Free Tier</th>
              <th>Included Access</th>
              <th>Expiration</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td><strong>Google</strong></td>
              <td>Yes</td>
              <td>Rate-limited access to Gemini models via AI Studio</td>
              <td>Never</td>
            </tr>
            <tr>
              <td><strong>Mistral AI</strong></td>
              <td>Yes</td>
              <td>Rate-limited access to all models via Experiment tier</td>
              <td>Never</td>
            </tr>
            <tr>
              <td><strong>Z.AI</strong></td>
              <td>Yes</td>
              <td>Access to specific lightweight models (e.g., GLM-4.5-Flash)</td>
              <td>Never</td>
            </tr>
            <tr>
              <td><strong>OpenAI</strong></td>
              <td>No</td>
              <td>One-time $5 sign-up credit</td>
              <td>~3 months</td>
            </tr>
            <tr>
              <td><strong>Anthropic</strong></td>
              <td>No</td>
              <td>One-time $5 sign-up credit</td>
              <td>Limited time</td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- How to use a free key in Bango -->
      <div class="lai-howto">
        <span class="material-symbols-outlined lai-howto__icon">settings</span>
        <div>
          <h4 class="lai-howto__title">Use a free key in Bango</h4>
          <ol class="lai-howto__steps">
            <li>Open <strong>Settings</strong> from the sidebar.</li>
            <li>
              Under <strong>AI Provider</strong>, select the matching provider (e.g. "Google Gemini"
              or "Mistral AI").
            </li>
            <li>Paste your API key and (optionally) pick a model from the list.</li>
            <li>
              Click <strong>Test Connection</strong>. If it succeeds, you are ready to run AI
              screening, summaries, and chat.
            </li>
          </ol>
        </div>
      </div>
    </section>

    <hr class="lai-divider" />

    <!-- ============================================================ -->
    <!-- SECTION 2: LOCAL AI SETUP GUIDE (self-hosted models)          -->
    <!-- ============================================================ -->
    <section id="local-ai-setup" class="lai-section">
      <header class="lai-section__header">
        <span class="material-symbols-outlined lai-section__icon">smart_toy</span>
        <div>
          <h3 class="lai-section__title">Local AI Setup Guide</h3>
          <p class="lai-section__subtitle">
            You can run AI models entirely on your own hardware for full privacy, zero API costs,
            and offline operation. This guide covers the tools, hardware, and configuration you need
            to get started.
          </p>
        </div>
      </header>

      <!-- Guide cards -->
      <div class="lai-list">
        <div v-for="(guide, idx) in localAiGuides" :key="idx" class="lai-card">
          <div class="lai-card__header">
            <span class="material-symbols-outlined lai-card__icon">{{ guide.icon }}</span>
            <h3 class="lai-card__title">{{ guide.title }}</h3>
          </div>
          <p v-if="guide.intro" class="lai-card__intro">{{ guide.intro }}</p>
          <ol v-if="guide.steps && guide.steps.length" class="lai-card__steps">
            <li v-for="(step, sIdx) in guide.steps" :key="sIdx" class="lai-card__step">
              <span class="lai-card__step-text">{{ step.text }}</span>
              <div v-if="step.code" class="lai-code">
                <code class="lai-code__text">{{ step.code }}</code>
                <button
                  class="lai-code__copy"
                  title="Copy to clipboard"
                  @click="copyCode(step.code!)"
                >
                  <span class="material-symbols-outlined">content_copy</span>
                </button>
              </div>
            </li>
          </ol>
          <p v-if="guide.tip" class="lai-card__tip">
            <span class="material-symbols-outlined lai-card__tip-icon">lightbulb</span>
            {{ guide.tip }}
          </p>
        </div>
      </div>

      <!-- Footer -->
      <section class="ht-footer">
        <div class="ht-footer-card">
          <span class="material-symbols-outlined ht-footer-icon">settings</span>
          <div>
            <h4 class="ht-footer-title">Ready to configure?</h4>
            <p class="ht-footer-desc">
              Go to <strong>Settings</strong> in the sidebar, select your local provider, and enter
              the endpoint URL. No API key is needed for most local servers.
            </p>
          </div>
          <button class="ht-footer-btn" @click="navigateTo('/settings')">Open Settings</button>
        </div>
      </section>

      <!-- Further reading -->
      <section class="ht-about">
        <div class="ht-about-card">
          <span class="material-symbols-outlined ht-about-icon">info</span>
          <div class="ht-about-body">
            <h4 class="ht-about-title">Further Reading</h4>
            <p class="ht-about-desc">
              For a deeper dive into running LLMs locally, check out these community resources and
              documentation sites.
            </p>
            <ul class="ht-about-links">
              <li>
                <span class="material-symbols-outlined ht-about-link-icon">link</span>
                <a
                  class="ht-about-link"
                  href="https://ollama.com"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Ollama - Official Site
                </a>
              </li>
              <li>
                <span class="material-symbols-outlined ht-about-link-icon">link</span>
                <a
                  class="ht-about-link"
                  href="https://github.com/ggerganov/llama.cpp"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  llama.cpp - GitHub
                </a>
              </li>
              <li>
                <span class="material-symbols-outlined ht-about-link-icon">link</span>
                <a
                  class="ht-about-link"
                  href="https://lmstudio.ai"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  LM Studio - Official Site
                </a>
              </li>
            </ul>
          </div>
        </div>
      </section>
    </section>
  </div>
</template>

<style scoped>
/* ===================== SECTION SHELL ===================== */
.lai-section {
  margin-bottom: var(--space-6);
}

/* Deep-link targets clear the sticky help tab bar (~60px) */
.lai-section[id] {
  scroll-margin-top: 60px;
}

.lai-section__header {
  display: flex;
  gap: var(--space-3);
  align-items: flex-start;
  border-bottom: 1px solid var(--color-border);
  padding-bottom: var(--space-3);
  margin-bottom: var(--space-4);
}

.lai-section__icon {
  font-size: 28px;
  color: #4f46e5;
  flex-shrink: 0;
  margin-top: 2px;
}

.lai-section__title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-1) 0;
}

.lai-section__subtitle {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

.lai-divider {
  border: none;
  border-top: 1px solid var(--color-border);
  margin: var(--space-6) 0;
}

/* ===================== CARDS (shared) ===================== */
.lai-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.lai-card {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
  box-shadow: var(--shadow-sm);
}

.lai-card__header {
  display: flex;
  gap: var(--space-3);
  align-items: center;
  margin-bottom: var(--space-3);
  flex-wrap: wrap;
}

.lai-card__icon {
  font-size: 24px;
  color: #4f46e5;
  flex-shrink: 0;
}

.lai-card__title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0;
}

.lai-card__intro {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0 0 var(--space-3) 0;
}

/* ===================== FREE-AI BADGES ===================== */
.lai-badge {
  display: inline-flex;
  align-items: center;
  font-size: 11px;
  font-weight: var(--font-weight-semibold);
  padding: 2px var(--space-2);
  border-radius: 999px;
  line-height: var(--line-height-body);
  white-space: nowrap;
}

.lai-badge--ongoing {
  background-color: #dcfce7;
  color: #166534;
  border: 1px solid #86efac;
}

.lai-badge--trial {
  background-color: #fef3c7;
  color: #92400e;
  border: 1px solid #fcd34d;
}

/* ===================== FREE-AI CARD DETAILS ===================== */
.lai-card__points {
  margin: 0 0 var(--space-3) 0;
  padding-left: var(--space-5);
}

.lai-card__points li {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin-bottom: var(--space-2);
}

.lai-card__points li:last-child {
  margin-bottom: 0;
}

.lai-card__models {
  display: flex;
  gap: var(--space-2);
  align-items: flex-start;
  background-color: #eef2ff;
  border: 1px solid #c7d2fe;
  border-radius: var(--radius-default);
  padding: var(--space-3);
  font-size: var(--font-size-caption);
  color: #3730a3;
  line-height: var(--line-height-body);
  margin: 0 0 var(--space-3) 0;
}

.lai-card__models-icon {
  font-size: 18px;
  color: #4f46e5;
  flex-shrink: 0;
}

.lai-card__link {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  color: #4f46e5;
  text-decoration: none;
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-caption);
}

.lai-card__link:hover {
  text-decoration: underline;
}

.lai-card__link .material-symbols-outlined {
  font-size: 16px;
}

/* ===================== FREE-AI COMPARISON TABLE ===================== */
.lai-table-wrapper {
  width: 100%;
  overflow-x: auto;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  margin-top: var(--space-5);
}

.lai-table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
  font-size: var(--font-size-body);
}

.lai-table th,
.lai-table td {
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-border);
  vertical-align: top;
}

.lai-table th {
  background-color: #f1f5f9;
  color: var(--color-on-surface);
  font-weight: var(--font-weight-bold);
  font-size: var(--font-size-caption);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.lai-table tr:last-child td {
  border-bottom: none;
}

.lai-table td strong {
  color: var(--color-on-surface);
}

/* ===================== FREE-AI HOW-TO CALLOUT ===================== */
.lai-howto {
  display: flex;
  gap: var(--space-4);
  align-items: flex-start;
  background-color: #fffbeb;
  border: 1px solid #fde68a;
  border-radius: var(--radius-md);
  padding: var(--space-5);
  margin-top: var(--space-5);
}

.lai-howto__icon {
  font-size: 22px;
  color: #b45309;
  flex-shrink: 0;
  margin-top: 2px;
}

.lai-howto__title {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: var(--space-2);
}

.lai-howto__steps {
  margin: 0;
  padding-left: var(--space-5);
}

.lai-howto__steps li {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin-bottom: var(--space-2);
}

.lai-howto__steps li:last-child {
  margin-bottom: 0;
}

/* ===================== LOCAL-AI GUIDE STEPS (unchanged) ===================== */
.lai-card__steps {
  margin: 0 0 var(--space-3) 0;
  padding-left: var(--space-5);
}

.lai-card__step {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin-bottom: var(--space-3);
}

.lai-card__step:last-child {
  margin-bottom: 0;
}

.lai-card__step-text {
  display: block;
  margin-bottom: var(--space-2);
}

.lai-code {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  background-color: #f8fafc;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  padding: var(--space-2) var(--space-3);
}

.lai-code__text {
  flex: 1;
  font-family: 'Fira Code', 'Cascadia Code', 'JetBrains Mono', ui-monospace, monospace;
  font-size: 12px;
  color: #334155;
  word-break: break-all;
}

.lai-code__copy {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: none;
  border: none;
  cursor: pointer;
  color: var(--color-on-surface-variant);
  padding: var(--space-1);
  border-radius: var(--radius-default);
  transition:
    background-color 0.15s,
    color 0.15s;
}

.lai-code__copy:hover {
  background-color: rgba(79, 70, 229, 0.08);
  color: #4f46e5;
}

.lai-code__copy .material-symbols-outlined {
  font-size: 16px;
}

.lai-card__tip {
  display: flex;
  gap: var(--space-2);
  align-items: flex-start;
  background-color: #fefce8;
  border: 1px solid #fde68a;
  border-radius: var(--radius-default);
  padding: var(--space-3);
  font-size: var(--font-size-caption);
  color: #92400e;
  line-height: var(--line-height-body);
  margin: 0;
}

.lai-card__tip-icon {
  font-size: 18px;
  color: #ca8a04;
  flex-shrink: 0;
}

@media (max-width: 767px) {
  .lai-howto {
    flex-direction: column;
  }
}
</style>
