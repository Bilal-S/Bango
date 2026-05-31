<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { tauriCommand, isTauri } from '@/composables/use-tauri-command';
import { ask } from '@tauri-apps/plugin-dialog';
import demoProjectJson from '@/assets/demo-project.bango.json?raw';
import { useArticlesStore } from '@/stores/articles';
import { useCriteriaStore } from '@/stores/criteria';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import { useLlmConfigStore } from '@/stores/llm-config';
import { useAuditStore } from '@/stores/audit';
import { useScreeningStore } from '@/stores/screening';

const route = useRoute();
const router = useRouter();
const activeTab = ref<'guide' | 'troubleshoot' | 'local-ai'>('guide');
const demoLoading = ref(false);
const demoError = ref<string | null>(null);

// Deep-link: /help?tab=troubleshoot#error-id
onMounted(() => {
  const tab = route.query.tab as string | undefined;
  if (tab === 'troubleshoot' || tab === 'local-ai' || tab === 'guide') {
    activeTab.value = tab;
  }
  // Scroll to anchor after DOM update
  if (route.hash) {
    requestAnimationFrame(() => {
      const el = document.getElementById(route.hash.slice(1));
      el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    });
  }
});

interface HelpStep {
  step: number;
  title: string;
  icon: string;
  route: string;
  routeLabel: string;
  summary: string;
  details: string[];
}

const steps: HelpStep[] = [
  {
    step: 1,
    title: 'Project Dashboard',
    icon: 'dashboard',
    route: '/',
    routeLabel: 'Dashboard',
    summary:
      'Your project overview - see how many articles you have and track your progress at a glance.',
    details: [
      'The dashboard shows you a summary of where things stand: how many articles are waiting to be reviewed, how many have been included or excluded, and any recent activity.',
      'From here you can jump to any part of the workflow using the sidebar or the quick action buttons.',
      'Think of it as your home base - you can always come back here to see the big picture.',
    ],
  },
  {
    step: 2,
    title: 'Define Your Criteria',
    icon: 'rule',
    route: '/criteria',
    routeLabel: 'Criteria',
    summary:
      'Tell Bango exactly what you are looking for by writing your inclusion and exclusion rules.',
    details: [
      'Before importing any articles, you define the rules for your review. These are the inclusion and exclusion criteria you would normally write in your review protocol.',
      'Each criterion can be given a priority level - from "Critical" (must be met) down to "Optional" (nice to have). This helps the AI make better decisions when rules conflict.',
      'You can also write your research aims here, so the AI understands the broader context of your study.',
    ],
  },
  {
    step: 3,
    title: 'Import Your Search Results',
    icon: 'upload_file',
    route: '/import',
    routeLabel: 'Import',
    summary:
      'Bring in your search results from databases like PubMed, Scopus, Web of Science, or any source that exports RIS files.',
    details: [
      'Most academic databases let you export your search results as an RIS file. Bango reads these files and pulls in the title, abstract, authors, journal, year, and keywords for each article.',
      'You can import multiple files - for example, one from PubMed and one from Scopus - and Bango will combine them into a single project.',
      'After import, Bango automatically checks for duplicate records (the same article found in more than one database) and flags them for your review.',
    ],
  },
  {
    step: 4,
    title: 'Review Duplicates',
    icon: 'science',
    route: '/dedup',
    routeLabel: 'Duplicates',
    summary: 'Review and resolve duplicate records that came from searching multiple databases.',
    details: [
      'When the same article appears in more than one database, Bango detects it and flags it as a potential duplicate.',
      'Exact matches are merged automatically. For close-but-not-identical matches, you can view them side by side and decide whether to keep or remove them.',
      'This step ensures each article only appears once in your review, which is required for accurate reporting.',
    ],
  },
  {
    step: 5,
    title: 'Review Tags & Labels',
    icon: 'sell',
    route: '/tags',
    routeLabel: 'Tags & Labels',
    summary: 'Review the categories the AI will use to organize and classify your articles.',
    details: [
      'Bango suggests content tags (like "clinical-trial" or "qualitative-study") based on the keywords in your articles and the criteria you defined.',
      'It also creates workflow labels (like "priority-read" or "disputed") that help you track the status of individual articles.',
      'You can review, add, edit, or remove any of these before the AI starts screening. This gives you full control over how articles are categorized.',
    ],
  },
  {
    step: 6,
    title: 'AI Screening',
    icon: 'analytics',
    route: '/screening',
    routeLabel: 'Screening',
    summary:
      "Let the AI read each article's abstract and decide whether it meets your inclusion criteria.",
    details: [
      'Once your criteria and tags are set, you start the AI screening. The AI reads the title and abstract of each article and evaluates it against your rules.',
      'For each article, the AI provides a decision (include or exclude), a written explanation, which criteria it matched, suggested tags, and a confidence score.',
      'You can watch the progress in real time. If the AI encounters a problem (like a rate limit from the server), it will retry automatically.',
      'You will need to configure an AI connection in Settings before screening. Bango supports several providers - both cloud-based and locally run models.',
    ],
  },
  {
    step: 7,
    title: 'Review Articles',
    icon: 'description',
    route: '/articles',
    routeLabel: 'Articles',
    summary:
      'Browse all articles, read abstracts, and override any AI decisions you disagree with.',
    details: [
      'After screening, every article has a status: Included, Excluded, or still in the Working list. You can browse, search, and filter to find specific articles.',
      'You have the final say - if you think the AI made a mistake, you can change any decision with one click. All changes are logged in the audit trail.',
      'Use the search bar to find articles by title or keyword. Filter by status, tags, confidence score, or year to focus on what matters most.',
    ],
  },
  {
    step: 8,
    title: 'PRISMA Flow Diagram',
    icon: 'account_tree',
    route: '/prisma',
    routeLabel: 'PRISMA',
    summary:
      'Generate a PRISMA 2020 flow diagram for your review report, with all record counts filled in automatically.',
    details: [
      'The PRISMA flow diagram is a standard requirement for systematic reviews. It shows how many records were identified, screened, included, and excluded at each stage.',
      'Bango generates this diagram for you automatically, with accurate counts drawn from your actual data.',
      'You can choose to show a breakdown of exclusion reasons. The diagram can be exported as an image (SVG or PNG) for inclusion in your manuscript.',
    ],
  },
  {
    step: 9,
    title: 'AI Summary',
    icon: 'summarize',
    route: '/summary',
    routeLabel: 'Summary',
    summary:
      'View a synthesis of your included articles - key themes, research trends, methodological patterns, and gaps in the literature.',
    details: [
      'Once you have a final set of included articles, Bango can produce a structured summary of the body of literature you have identified.',
      'The summary covers recurring themes, common research methods, strengths and weaknesses across studies, and areas where further research is needed.',
      'This can serve as a starting point for writing the narrative synthesis or discussion section of your review.',
    ],
  },
];

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

async function loadDemo(): Promise<void> {
  if (demoLoading.value) return;
  if (!isTauri()) {
    demoError.value = 'Demo requires the desktop app.';
    return;
  }

  // Always confirm via native dialog - this is destructive (replaces all project data)
  const confirmed = await ask(
    'Loading the demo project will replace all your current data ' +
      '(articles, criteria, tags, labels). This cannot be undone.',
    { title: 'Load Demo Project', kind: 'warning', okLabel: 'Load Demo', cancelLabel: 'Cancel' }
  );
  if (!confirmed) return;

  demoLoading.value = true;
  demoError.value = null;
  try {
    await tauriCommand('import_project_backup', {
      request: { jsonContent: demoProjectJson },
    });
    // Invalidate and re-fetch all stores
    const stores = [
      useArticlesStore(),
      useCriteriaStore(),
      useTagsStore(),
      useLabelsStore(),
      useLlmConfigStore(),
      useAuditStore(),
      useScreeningStore(),
    ];
    for (const store of stores) {
      store.invalidate();
    }
    await Promise.all(stores.map((s) => s.fetchIfNeeded()));
    router.push('/');
  } catch (e: unknown) {
    demoError.value = e instanceof Error ? e.message : String(e);
  } finally {
    demoLoading.value = false;
  }
}
</script>

<template>
  <div class="help-guide">
    <!-- Page Header -->
    <section class="help-guide__header">
      <h1 class="page-title">Help & Guides</h1>
      <p class="help-guide__subtitle">
        Everything you need to get the most out of Bango - from step-by-step workflows to
        troubleshooting and local AI setup.
      </p>
    </section>

    <!-- Tab Bar -->
    <nav class="help-tabs" role="tablist">
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'guide' }"
        role="tab"
        :aria-selected="activeTab === 'guide'"
        @click="activeTab = 'guide'"
      >
        <span class="material-symbols-outlined help-tabs__icon">menu_book</span>
        User Guide
      </button>
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'troubleshoot' }"
        role="tab"
        :aria-selected="activeTab === 'troubleshoot'"
        @click="activeTab = 'troubleshoot'"
      >
        <span class="material-symbols-outlined help-tabs__icon">build</span>
        Troubleshooting
      </button>
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'local-ai' }"
        role="tab"
        :aria-selected="activeTab === 'local-ai'"
        @click="activeTab = 'local-ai'"
      >
        <span class="material-symbols-outlined help-tabs__icon">smart_toy</span>
        Local AI
      </button>
    </nav>

    <!-- ===================== TAB: USER GUIDE ===================== -->
    <div v-if="activeTab === 'guide'" role="tabpanel">
      <!-- Workflow Overview -->
      <section class="help-guide__overview">
        <div class="help-guide__overview-card">
          <div class="help-guide__overview-icon material-symbols-outlined">route</div>
          <div class="help-guide__overview-text">
            <h3 class="help-guide__overview-title">The Big Picture</h3>
            <p class="help-guide__overview-desc">
              Bango follows the standard systematic review process: you import search results from
              academic databases, remove duplicates, define what you are looking for, and then let
              AI help you screen each article's title and abstract. You always have the final say on
              every decision. When you are done, Bango generates a PRISMA flow diagram and a summary
              of your included literature.
            </p>
          </div>
        </div>
      </section>

      <!-- Steps -->
      <section class="help-guide__steps">
        <div v-for="step in steps" :key="step.step" class="help-step">
          <div class="help-step__indicator">
            <div class="help-step__number">{{ step.step }}</div>
            <div v-if="step.step < steps.length" class="help-step__line" />
          </div>
          <div class="help-step__card">
            <div class="help-step__card-header">
              <span class="material-symbols-outlined help-step__icon">{{ step.icon }}</span>
              <div class="help-step__card-title-area">
                <h3 class="help-step__title">{{ step.title }}</h3>
                <p class="help-step__summary">{{ step.summary }}</p>
              </div>
            </div>
            <ul class="help-step__details">
              <li v-for="(detail, idx) in step.details" :key="idx" class="help-step__detail">
                {{ detail }}
              </li>
            </ul>
            <button class="help-step__go-btn" @click="navigateTo(step.route)">
              <span class="material-symbols-outlined help-step__go-icon">arrow_forward</span>
              Go to {{ step.routeLabel }}
            </button>
          </div>
        </div>
      </section>

      <!-- Getting Help Footer -->
      <section class="help-guide__footer">
        <div class="help-guide__footer-card">
          <span class="material-symbols-outlined help-guide__footer-icon">settings</span>
          <div>
            <h4 class="help-guide__footer-title">Need to configure AI?</h4>
            <p class="help-guide__footer-desc">
              Before AI screening can run, you need to set up a connection to an AI provider. Go to
              <strong>Settings</strong> in the sidebar to enter your provider details and API key.
            </p>
          </div>
          <button class="help-guide__footer-btn" @click="navigateTo('/settings')">
            Open Settings
          </button>
        </div>
      </section>

      <!-- Demo Tile -->
      <section class="help-guide__demo">
        <div class="help-guide__demo-card">
          <span class="material-symbols-outlined help-guide__demo-icon">science</span>
          <div class="help-guide__demo-body">
            <h4 class="help-guide__demo-title">Try the Demo</h4>
            <p class="help-guide__demo-desc">
              Load a sample project with articles, criteria, and research aims to explore Bango's
              features without setting up your own data. This will replace any existing project
              data.
            </p>
            <p v-if="demoError" class="help-guide__demo-error">{{ demoError }}</p>
          </div>
          <button class="help-guide__demo-btn" :disabled="demoLoading" @click="loadDemo()">
            <span v-if="demoLoading" class="material-symbols-outlined help-guide__demo-spinner">
              progress_activity
            </span>
            <span v-else class="material-symbols-outlined help-guide__demo-btn-icon"
              >play_circle</span
            >
            {{ demoLoading ? 'Loading…' : 'Load Demo Project' }}
          </button>
        </div>
      </section>

      <!-- Developer & License -->
      <section class="help-guide__about">
        <div class="help-guide__about-card">
          <span class="material-symbols-outlined help-guide__about-icon">info</span>
          <div class="help-guide__about-body">
            <h4 class="help-guide__about-title">About Bango</h4>
            <p class="help-guide__about-desc">
              Developed by <strong>BonCode (Bilal Soylu)</strong> with permission from
              <strong>Startup Strategy Advisors LLC</strong>. Released as open source under the
              <strong>Apache License 2.0</strong>.
            </p>
            <ul class="help-guide__about-links">
              <li>
                <span class="material-symbols-outlined help-guide__about-link-icon"
                  >bug_report</span
                >
                <a
                  class="help-guide__about-link"
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

    <!-- ===================== TAB: TROUBLESHOOTING ===================== -->
    <div v-if="activeTab === 'troubleshoot'" role="tabpanel">
      <section class="ts-intro">
        <h2 class="ts-intro__title">Common Errors & Solutions</h2>
        <p class="ts-intro__desc">
          If you are seeing an error while using Bango, check the list below. Each entry shows the
          error message you might encounter, what causes it, and what you can do to fix it.
        </p>
      </section>

      <div class="ts-list">
        <div
          v-for="(item, idx) in troubleshootItems"
          :id="item.anchorId"
          :key="idx"
          class="ts-card"
        >
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

      <section class="help-guide__about">
        <div class="help-guide__about-card">
          <span class="material-symbols-outlined help-guide__about-icon">info</span>
          <div class="help-guide__about-body">
            <h4 class="help-guide__about-title">Still stuck?</h4>
            <p class="help-guide__about-desc">
              If your issue is not listed above, please open an issue on GitHub and the community
              will help you out. Include the error message, your provider, and what you were doing
              when it happened.
            </p>
            <ul class="help-guide__about-links">
              <li>
                <span class="material-symbols-outlined help-guide__about-link-icon"
                  >bug_report</span
                >
                <a
                  class="help-guide__about-link"
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

    <!-- ===================== TAB: LOCAL AI ===================== -->
    <div v-if="activeTab === 'local-ai'" role="tabpanel">
      <section class="ts-intro">
        <h2 class="ts-intro__title">Local AI Setup Guide</h2>
        <p class="ts-intro__desc">
          You can run AI models entirely on your own hardware for full privacy, zero API costs, and
          offline operation. This guide covers the tools, hardware, and configuration you need to
          get started.
        </p>
      </section>

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

      <section class="help-guide__footer">
        <div class="help-guide__footer-card">
          <span class="material-symbols-outlined help-guide__footer-icon">settings</span>
          <div>
            <h4 class="help-guide__footer-title">Ready to configure?</h4>
            <p class="help-guide__footer-desc">
              Go to <strong>Settings</strong> in the sidebar, select your local provider, and enter
              the endpoint URL. No API key is needed for most local servers.
            </p>
          </div>
          <button class="help-guide__footer-btn" @click="navigateTo('/settings')">
            Open Settings
          </button>
        </div>
      </section>

      <section class="help-guide__about">
        <div class="help-guide__about-card">
          <span class="material-symbols-outlined help-guide__about-icon">info</span>
          <div class="help-guide__about-body">
            <h4 class="help-guide__about-title">Further Reading</h4>
            <p class="help-guide__about-desc">
              For a deeper dive into running LLMs locally, check out these community resources and
              documentation sites.
            </p>
            <ul class="help-guide__about-links">
              <li>
                <span class="material-symbols-outlined help-guide__about-link-icon">link</span>
                <a
                  class="help-guide__about-link"
                  href="https://ollama.com"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Ollama - Official Site
                </a>
              </li>
              <li>
                <span class="material-symbols-outlined help-guide__about-link-icon">link</span>
                <a
                  class="help-guide__about-link"
                  href="https://github.com/ggerganov/llama.cpp"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  llama.cpp - GitHub
                </a>
              </li>
              <li>
                <span class="material-symbols-outlined help-guide__about-link-icon">link</span>
                <a
                  class="help-guide__about-link"
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
  </div>
</template>

<style scoped>
.help-guide {
  padding: var(--container-padding);
  max-width: 860px;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .help-guide {
    padding: var(--container-padding-sm);
  }
}

/* Header */
.help-guide__header {
  margin-bottom: var(--space-4);
}

.help-guide__subtitle {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-body);
  margin-top: var(--space-2);
  line-height: var(--line-height-body);
}

/* ===================== TAB BAR ===================== */
.help-tabs {
  display: flex;
  gap: 0;
  border-bottom: 2px solid var(--color-outline-variant, #e0e0e0);
  margin-bottom: var(--space-6);
}

.help-tabs__btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  margin-bottom: -2px;
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface-variant);
  cursor: pointer;
  transition:
    color 0.15s,
    border-color 0.15s,
    background-color 0.15s;
  font-family: inherit;
  white-space: nowrap;
}

.help-tabs__btn:hover {
  color: var(--color-on-surface);
  background-color: rgba(79, 70, 229, 0.04);
}

.help-tabs__btn--active {
  color: #4f46e5;
  border-bottom-color: #4f46e5;
}

.help-tabs__icon {
  font-size: 20px;
}

@media (max-width: 767px) {
  .help-tabs {
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
  }

  .help-tabs__btn {
    padding: var(--space-2) var(--space-3);
    font-size: var(--font-size-caption);
  }

  .help-tabs__icon {
    font-size: 18px;
  }
}

/* ===================== USER GUIDE ===================== */

/* Overview Card */
.help-guide__overview {
  margin-bottom: var(--space-8);
}

.help-guide__overview-card {
  display: flex;
  gap: var(--space-5);
  background-color: #eef2ff;
  border: 1px solid #c7d2fe;
  border-radius: var(--radius-md);
  padding: var(--space-5);
}

.help-guide__overview-icon {
  font-size: 28px;
  color: #4f46e5;
  flex-shrink: 0;
  margin-top: 2px;
}

.help-guide__overview-title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: var(--space-2);
}

.help-guide__overview-desc {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

/* Steps */
.help-guide__steps {
  display: flex;
  flex-direction: column;
}

.help-step {
  display: flex;
  gap: var(--space-5);
}

.help-step + .help-step {
  margin-top: 0;
}

/* Step Indicator (number + line) */
.help-step__indicator {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex-shrink: 0;
  width: 36px;
}

.help-step__number {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background-color: #4f46e5;
  color: #ffffff;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-body);
  flex-shrink: 0;
}

.help-step__line {
  width: 2px;
  flex: 1;
  background-color: #c7d2fe;
  min-height: 16px;
}

/* Step Card */
.help-step__card {
  flex: 1;
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
  box-shadow: var(--shadow-sm);
  margin-bottom: var(--space-4);
}

.help-step__card-header {
  display: flex;
  gap: var(--space-4);
  align-items: flex-start;
  margin-bottom: var(--space-4);
}

.help-step__icon {
  font-size: 22px;
  color: #4f46e5;
  background-color: #eef2ff;
  border-radius: var(--radius-default);
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.help-step__card-title-area {
  flex: 1;
  min-width: 0;
}

.help-step__title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: var(--space-1);
}

.help-step__summary {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

/* Detail List */
.help-step__details {
  list-style: none;
  padding: 0;
  margin: 0 0 var(--space-4) 0;
}

.help-step__detail {
  position: relative;
  padding-left: var(--space-5);
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin-bottom: var(--space-2);
}

.help-step__detail::before {
  content: '';
  position: absolute;
  left: 0;
  top: 9px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background-color: #c7d2fe;
}

.help-step__detail:last-child {
  margin-bottom: 0;
}

/* Go Button */
.help-step__go-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-4);
  background-color: transparent;
  color: #4f46e5;
  border: 1px solid #c7d2fe;
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
  font-family: inherit;
}

.help-step__go-btn:hover {
  background-color: #4f46e5;
  color: #ffffff;
  border-color: #4f46e5;
}

.help-step__go-icon {
  font-size: 16px;
}

/* ===================== TROUBLESHOOTING ===================== */
.ts-intro {
  margin-bottom: var(--space-6);
}

.ts-intro__title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: var(--space-2);
}

.ts-intro__desc {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

.ts-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.ts-card {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-4) var(--space-5);
  box-shadow: var(--shadow-sm);
}

.ts-card__header {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
}

.ts-card__icon {
  font-size: 22px;
  color: #dc2626;
  background-color: #fef2f2;
  border-radius: var(--radius-default);
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.ts-card__header-text {
  flex: 1;
  min-width: 0;
}

.ts-card__error {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: 2px;
}

.ts-card__providers {
  font-size: var(--font-size-caption);
  color: #6b7280;
  font-weight: var(--font-weight-semibold);
  background-color: #f3f4f6;
  padding: 1px 8px;
  border-radius: var(--radius-default);
  display: inline-block;
}

.ts-card__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-left: var(--space-1);
}

.ts-card__field {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
}

.ts-card__field strong {
  color: var(--color-on-surface);
  font-weight: var(--font-weight-semibold);
}

/* ===================== LOCAL AI ===================== */
.lai-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.lai-card {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-4) var(--space-5);
  box-shadow: var(--shadow-sm);
}

.lai-card__header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
}

.lai-card__icon {
  font-size: 22px;
  color: #4f46e5;
  background-color: #eef2ff;
  border-radius: var(--radius-default);
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.lai-card__title {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0;
}

.lai-card__intro {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0 0 var(--space-3) 0;
}

.lai-card__steps {
  list-style: decimal;
  padding-left: var(--space-5);
  margin: 0 0 var(--space-3) 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.lai-card__step {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  padding-left: var(--space-1);
}

.lai-card__step-text {
  display: block;
}

.lai-card__step::marker {
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
}

/* Code block with copy button */
.lai-code {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  background-color: #1e1e2e;
  border-radius: var(--radius-default);
  padding: var(--space-1) var(--space-2);
  margin-top: var(--space-1);
}

.lai-code__text {
  flex: 1;
  font-family: 'Fira Code', 'Cascadia Code', 'JetBrains Mono', ui-monospace, monospace;
  font-size: 12px;
  color: #cdd6f4;
  background: none;
  word-break: break-all;
}

.lai-code__copy {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: none;
  border: none;
  color: #7f849c;
  cursor: pointer;
  padding: 2px;
  border-radius: var(--radius-default);
  flex-shrink: 0;
  transition: color 0.15s;
}

.lai-code__copy:hover {
  color: #cdd6f4;
}

.lai-code__copy .material-symbols-outlined {
  font-size: 16px;
}

/* Tip callout */
.lai-card__tip {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  background-color: #fffbeb;
  border: 1px solid #fde68a;
  border-radius: var(--radius-default);
  padding: var(--space-2) var(--space-3);
  margin: 0;
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
}

.lai-card__tip-icon {
  font-size: 16px;
  color: #d97706;
  flex-shrink: 0;
  margin-top: 1px;
}

/* ===================== SHARED SECTIONS ===================== */

/* Footer */
.help-guide__footer {
  margin-top: var(--space-6);
  margin-bottom: var(--space-8);
}

.help-guide__footer-card {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  background-color: #fffbeb;
  border: 1px solid #fde68a;
  border-radius: var(--radius-md);
  padding: var(--space-4) var(--space-5);
}

.help-guide__footer-icon {
  font-size: 22px;
  color: #d97706;
  flex-shrink: 0;
}

.help-guide__footer-title {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: 2px;
}

.help-guide__footer-desc {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

.help-guide__footer-btn {
  display: inline-flex;
  align-items: center;
  padding: var(--space-2) var(--space-3);
  background-color: #d97706;
  color: #ffffff;
  border: none;
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
  font-family: inherit;
  transition: background-color 0.15s;
}

.help-guide__footer-btn:hover {
  background-color: #b45309;
}

/* Demo Tile */
.help-guide__demo {
  margin-bottom: var(--space-8);
}

.help-guide__demo-card {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  background-color: #eef2ff;
  border: 1px solid #c7d2fe;
  border-radius: var(--radius-md);
  padding: var(--space-4) var(--space-5);
}

.help-guide__demo-icon {
  font-size: 22px;
  color: #4f46e5;
  flex-shrink: 0;
}

.help-guide__demo-body {
  flex: 1;
  min-width: 0;
}

.help-guide__demo-title {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: 2px;
}

.help-guide__demo-desc {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

.help-guide__demo-error {
  font-size: var(--font-size-caption);
  color: #dc2626;
  margin: var(--space-1) 0 0 0;
}

.help-guide__demo-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-3);
  background-color: #4f46e5;
  color: #ffffff;
  border: none;
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
  font-family: inherit;
  transition: background-color 0.15s;
}

.help-guide__demo-btn:hover:not(:disabled) {
  background-color: #4338ca;
}

.help-guide__demo-btn:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}

.help-guide__demo-btn-icon,
.help-guide__demo-spinner {
  font-size: 16px;
}

.help-guide__demo-spinner {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 767px) {
  .help-guide__overview-card {
    flex-direction: column;
  }

  .help-step__card-header {
    flex-direction: column;
    gap: var(--space-3);
  }

  .help-guide__footer-card {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .help-guide__demo-card {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .help-guide__about-card {
    flex-direction: column;
  }
}

/* About / License Section */
.help-guide__about {
  margin-bottom: var(--space-8);
}

.help-guide__about-card {
  display: flex;
  gap: var(--space-4);
  background-color: #f0fdf4;
  border: 1px solid #bbf7d0;
  border-radius: var(--radius-md);
  padding: var(--space-5);
}

.help-guide__about-icon {
  font-size: 22px;
  color: #16a34a;
  flex-shrink: 0;
  margin-top: 2px;
}

.help-guide__about-body {
  flex: 1;
  min-width: 0;
}

.help-guide__about-title {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: var(--space-2);
}

.help-guide__about-desc {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0 0 var(--space-3) 0;
}

.help-guide__about-links {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.help-guide__about-links li {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.help-guide__about-link-icon {
  font-size: 16px;
  color: #16a34a;
  flex-shrink: 0;
}

.help-guide__about-link {
  color: #4f46e5;
  text-decoration: none;
  font-weight: var(--font-weight-semibold);
}

.help-guide__about-link:hover {
  text-decoration: underline;
}

.help-guide__about-text {
  line-height: var(--line-height-body);
}

.help-guide__about-text code {
  background-color: #eef2ff;
  padding: 1px 6px;
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
}
</style>
