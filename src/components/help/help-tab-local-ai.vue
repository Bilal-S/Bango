<script setup lang="ts">
import { useRouter } from 'vue-router';
import '@/styles/help-shared.css';

const router = useRouter();

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
    <!-- Intro -->
    <section class="ht-intro">
      <h2 class="ht-intro__title">Local AI Setup Guide</h2>
      <p class="ht-intro__desc">
        You can run AI models entirely on your own hardware for full privacy, zero API costs, and
        offline operation. This guide covers the tools, hardware, and configuration you need to get
        started.
      </p>
    </section>

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
  </div>
</template>

<style scoped>
.ht-local-ai {
  /* Container; uses shared .ht-intro, .ht-footer, .ht-about classes */
}

.lai-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
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
</style>
