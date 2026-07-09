<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue';
import '@/styles/help-shared.css';

/**
 * Reference tab.
 *
 * A sidebar + scroll-spy layout with 12 detailed sections. Accepts an `initialHash`
 * prop (from the parent shell's route hash) so deep-links like
 * `/help?tab=reference#ref-references-citations` scroll to the right section on mount.
 */

const props = defineProps<{
  initialHash?: string;
}>();

const emit = defineEmits<{
  (e: 'switch-tab', tab: string): void;
}>();

const activeRefSection = ref<string>('ref-ai-philosophy');

let scrollContainer: HTMLElement | null = null;
let isScrollingManual = false;
let manualScrollTimeout: number | null = null;

function scrollToSection(id: string): void {
  activeRefSection.value = id;
  const el = document.getElementById(id);
  if (el) {
    isScrollingManual = true;
    if (manualScrollTimeout) {
      window.clearTimeout(manualScrollTimeout);
    }
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    manualScrollTimeout = window.setTimeout(() => {
      isScrollingManual = false;
    }, 1000);
  }
}

function selectRefSection(id: string): void {
  scrollToSection(id);
}

function handleScroll(): void {
  if (isScrollingManual) return;

  const sections = document.querySelectorAll('div.ref-content > section.ref-section[id]');
  let currentSectionId = activeRefSection.value;

  const containerRect = scrollContainer?.getBoundingClientRect();
  if (!containerRect) return;

  const triggerPoint = containerRect.top + 120;

  for (const sec of sections) {
    const rect = sec.getBoundingClientRect();
    if (rect.top <= triggerPoint) {
      currentSectionId = sec.id;
    }
  }

  if (currentSectionId && activeRefSection.value !== currentSectionId) {
    activeRefSection.value = currentSectionId;
  }
}

onMounted(() => {
  // Apply deep-link hash from the parent shell if provided.
  if (props.initialHash) {
    const hashId = props.initialHash.startsWith('#')
      ? props.initialHash.slice(1)
      : props.initialHash;
    if (hashId.startsWith('ref-')) {
      if (hashId === 'ref-import-references-popup') {
        activeRefSection.value = 'ref-references-citations';
      } else {
        activeRefSection.value = hashId;
      }
    }
    requestAnimationFrame(() => {
      const el = document.getElementById(hashId);
      el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    });
  }

  scrollContainer = document.querySelector('.app-shell__content');
  if (scrollContainer) {
    scrollContainer.addEventListener('scroll', handleScroll, { passive: true });
  }
});

onUnmounted(() => {
  if (scrollContainer) {
    scrollContainer.removeEventListener('scroll', handleScroll);
  }
  if (manualScrollTimeout) {
    window.clearTimeout(manualScrollTimeout);
  }
});

// Re-apply hash when the parent updates the prop (e.g. navigating while tab is mounted).
watch(
  () => props.initialHash,
  (newHash) => {
    if (!newHash) return;
    const hashId = newHash.startsWith('#') ? newHash.slice(1) : newHash;
    if (hashId.startsWith('ref-')) {
      requestAnimationFrame(() => {
        const el = document.getElementById(hashId);
        el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      });
    }
  }
);
</script>

<template>
  <div class="ht-reference" role="tabpanel">
    <div class="ref-tab-layout">
      <!-- Navigation Sidebar -->
      <aside class="ref-sidebar" aria-label="Reference Navigation">
        <nav class="ref-nav">
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-ai-philosophy' }"
            @click="selectRefSection('ref-ai-philosophy')"
          >
            <span class="material-symbols-outlined ref-nav__icon">psychology</span>
            AI Integration & Philosophy
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-dashboard' }"
            @click="selectRefSection('ref-dashboard')"
          >
            <span class="material-symbols-outlined ref-nav__icon">dashboard</span>
            Project Dashboard
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-criteria' }"
            @click="selectRefSection('ref-criteria')"
          >
            <span class="material-symbols-outlined ref-nav__icon">rule</span>
            Criteria Editor
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-import' }"
            @click="selectRefSection('ref-import')"
          >
            <span class="material-symbols-outlined ref-nav__icon">upload_file</span>
            Import Bibliography
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-dedup' }"
            @click="selectRefSection('ref-dedup')"
          >
            <span class="material-symbols-outlined ref-nav__icon">science</span>
            Duplicate Resolution
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-tags' }"
            @click="selectRefSection('ref-tags')"
          >
            <span class="material-symbols-outlined ref-nav__icon">sell</span>
            Tags & Labels
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-screening' }"
            @click="selectRefSection('ref-screening')"
          >
            <span class="material-symbols-outlined ref-nav__icon">analytics</span>
            AI Screening Setup
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-translation' }"
            @click="selectRefSection('ref-translation')"
          >
            <span class="material-symbols-outlined ref-nav__icon">translate</span>
            Translations
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-review' }"
            @click="selectRefSection('ref-review')"
          >
            <span class="material-symbols-outlined ref-nav__icon">description</span>
            Article Detail Panels
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-references-citations' }"
            @click="selectRefSection('ref-references-citations')"
          >
            <span class="material-symbols-outlined ref-nav__icon">link</span>
            References & Citations
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-bibliometrics' }"
            @click="selectRefSection('ref-bibliometrics')"
          >
            <span class="material-symbols-outlined ref-nav__icon">hub</span>
            Bibliometrics Analysis
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-chat' }"
            @click="selectRefSection('ref-chat')"
          >
            <span class="material-symbols-outlined ref-nav__icon">chat</span>
            Chat Assistant
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-gap-report' }"
            @click="selectRefSection('ref-gap-report')"
          >
            <span class="material-symbols-outlined ref-nav__icon">lightbulb</span>
            Research Gap Report
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-search-strategy' }"
            @click="selectRefSection('ref-search-strategy')"
          >
            <span class="material-symbols-outlined ref-nav__icon">search</span>
            Search Strategy Builder
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-complex-screening' }"
            @click="selectRefSection('ref-complex-screening')"
          >
            <span class="material-symbols-outlined ref-nav__icon">account_tree</span>
            Complex Screening Rules
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-wiki' }"
            @click="selectRefSection('ref-wiki')"
          >
            <span class="material-symbols-outlined ref-nav__icon">local_library</span>
            Wiki
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-settings' }"
            @click="selectRefSection('ref-settings')"
          >
            <span class="material-symbols-outlined ref-nav__icon">settings</span>
            Settings & API Security
          </button>
          <button
            class="ref-nav__link"
            :class="{ 'ref-nav__link--active': activeRefSection === 'ref-backup' }"
            @click="selectRefSection('ref-backup')"
          >
            <span class="material-symbols-outlined ref-nav__icon">backup</span>
            Backup & Restore
          </button>
        </nav>
      </aside>

      <!-- Main Content Area -->
      <div class="ref-content">
        <!-- SECTION: AI INTEGRATION & PHILOSOPHY -->
        <section id="ref-ai-philosophy" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">psychology</span>
            <h2 class="ref-section__title">AI Integration & Philosophy</h2>
          </header>
          <div class="ref-section__body">
            <p>
              Bango is designed with a specific philosophy: AI should serve as a high-accuracy,
              transparent research assistant that helps you organize and analyze literature, not a
              black-box decision-maker that replaces human oversight.
            </p>

            <h3>How Bango Works with AI</h3>
            <p>
              Traditional AI chat interfaces require you to copy-paste or upload entire PDFs into a
              single context window. In contrast, Bango treats papers the way a human researcher
              does: section by section, with the most relevant passages surfaced on demand.
            </p>
            <ul>
              <li>
                <strong>Section-Aware PDF Extraction:</strong> When you attach a PDF, Bango parses
                the text and splits it into structured Method, Results, and Discussion passages.
                This ensures important sections are not lost to context window limits.
              </li>
              <li>
                <strong>Criteria-Targeted Chunking:</strong> Rather than feeding an entire paper to
                the AI, Bango's retrieval engine ranks and pulls only the top-K passages matching
                your criteria.
              </li>
              <li>
                <strong>Grounded Citation Mapping:</strong> Every AI summary, screening advice, and
                chat answer is directly mapped to the source passage (e.g.,
                <code>Smith 2023 (§Methods)</code>). These labels are interactive links that open
                the source text, so you can verify the AI's work.
              </li>
              <li>
                <strong>Deterministic Logic Layer:</strong> Bango evaluates conflict resolution
                rules locally in SQLite based on your criteria priorities, using the LLM solely to
                advise on matches.
              </li>
            </ul>

            <h3>Bango RAG vs. Dumping Documents into Chat Windows</h3>
            <div class="ref-comparison-wrapper">
              <table class="ref-comparison-table">
                <thead>
                  <tr>
                    <th>Dimension</th>
                    <th>Dumping in Chat Windows (ChatGPT/Claude UI)</th>
                    <th>Bango Structured Retrieval (RAG)</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td><strong>Context Truncation</strong></td>
                    <td>High. Long papers exceed limits and get silently cut off.</td>
                    <td>
                      None. Section-by-section parsing and word budgets prevent context overflow.
                    </td>
                  </tr>
                  <tr>
                    <td><strong>Hallucination Risk</strong></td>
                    <td>
                      High. The model generates summaries based on incomplete or recalled memory.
                    </td>
                    <td>
                      Low. The model is constrained to analyze only target, extracted passages.
                    </td>
                  </tr>
                  <tr>
                    <td><strong>Verifiability</strong></td>
                    <td>
                      Low. Answers are plain text; you must search the PDF to locate the source.
                    </td>
                    <td>
                      High. Interactive passage links (e.g., <code>§Methods</code>) open the exact
                      source text.
                    </td>
                  </tr>
                  <tr>
                    <td><strong>Reproducibility</strong></td>
                    <td>
                      None. Repeated questions yield different outputs; there is no saved history.
                    </td>
                    <td>Full. Every decision is logged in an immutable database audit trail.</td>
                  </tr>
                  <tr>
                    <td><strong>Structured Comparison</strong></td>
                    <td>
                      Manual. You must copy/paste data to spreadsheets to compare papers
                      side-by-side.
                    </td>
                    <td>
                      Automatic. Standard variables (sample size, design, effect sizes) are parsed
                      into cards.
                    </td>
                  </tr>
                  <tr>
                    <td><strong>Setup Complexity</strong></td>
                    <td>Instant. Just upload or paste text and type.</td>
                    <td>Structured. Requires importing bibliography files and attaching PDFs.</td>
                  </tr>
                </tbody>
              </table>
            </div>

            <h3>Benefits & Drawbacks of Bango's Approach</h3>
            <p>
              While Bango's structured approach provides the rigor needed for systematic reviews, it
              represents a different set of trade-offs:
            </p>
            <ul>
              <li>
                <strong>Benefits:</strong> Perfect audit trails for PRISMA reporting, automatic
                re-evaluation when criteria change, and lower API token costs by avoiding sending
                unnecessary parts of the paper.
              </li>
              <li>
                <strong>Drawbacks:</strong> Processing requires a multi-phase pipeline (import, text
                extraction, chunking, and database mapping) that takes longer to run than a simple
                chat prompt, and requires attaching original PDF files.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: DASHBOARD -->
        <section id="ref-dashboard" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">dashboard</span>
            <h2 class="ref-section__title">Project Dashboard</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The <strong>Project Dashboard</strong> is the home base of your systematic literature
              review. It provides a real-time status summary of your article library and screening
              progress.
            </p>
            <h3>Key Features</h3>
            <ul>
              <li>
                <strong>Progress KPIs:</strong> Cards displaying the counts of articles in different
                states: <em>Working</em> (unscreened), <em>Included</em>, <em>Rejected</em>, and
                unresolved <em>Duplicates</em>.
              </li>
              <li>
                <strong>Recent Activity Trail:</strong> An active chronological log showing the
                latest system and user actions, such as imports, status overrides, and AI screening
                updates.
              </li>
              <li>
                <strong>Quick Actions:</strong> Navigation buttons to jump directly to key screening
                tasks.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: CRITERIA EDITOR -->
        <section id="ref-criteria" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">rule</span>
            <h2 class="ref-section__title">Criteria Editor</h2>
          </header>
          <div class="ref-section__body">
            <p>
              Before starting your AI screening, you must define the boundaries of your review. The
              <strong>Criteria Editor</strong> allows you to formulate research aims and explicit
              screening rules.
            </p>
            <h3>Key Features</h3>
            <ul>
              <li>
                <strong>Research Aims:</strong> Free-text statements defining the broader research
                context. These are sent as prompts to the LLM to guide screening decisions.
              </li>
              <li>
                <strong>Inclusion / Exclusion Criteria:</strong> Explicit conditions articles must
                meet (Inclusion) or must not meet (Exclusion).
              </li>
              <li>
                <strong>Priority Conflict Resolution:</strong> Each criterion must be assigned a
                priority (<em>Critical</em>, <em>High</em>, <em>Standard</em>, <em>Low</em>, or
                <em>Optional</em>). If both inclusion and exclusion criteria match:
                <ol>
                  <li>The highest-priority matching criterion wins the decision.</li>
                  <li>If priorities are tied, inclusion wins.</li>
                  <li>If no criteria match at all, the article is excluded.</li>
                </ol>
              </li>
            </ul>
            <h3>Screen Controls & Buttons</h3>
            <ul>
              <li>
                <strong>Add Aim / Add Criterion:</strong> Buttons that open input forms to add items
                to the active list.
              </li>
              <li>
                <strong>Priority Selector:</strong> Dropdown menus beside criteria to update
                priority values.
              </li>
              <li>
                <strong>Inline Edit:</strong> Double-click any aim or criterion text to edit it in
                place in a multi-line box. Press <kbd>Enter</kbd> or click outside to save;
                <kbd>Shift</kbd>+<kbd>Enter</kbd> inserts a new line; press <kbd>Esc</kbd> to
                cancel. Saving an empty field deletes the item.
              </li>
              <li>
                <strong>Delete:</strong> The trash icon on each row permanently deletes the item.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: IMPORT BIBLIOGRAPHY -->
        <section id="ref-import" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">upload_file</span>
            <h2 class="ref-section__title">Import Bibliography</h2>
          </header>
          <div class="ref-section__body">
            <p>
              Bango supports importing bibliography results exported from academic search engines.
            </p>
            <h3>Key Features</h3>
            <ul>
              <li>
                <strong>Multi-Format Support:</strong> Reads both standard
                <strong>RIS (.ris)</strong> and <strong>BibTeX (.bib)</strong> files.
              </li>
              <li>
                <strong>Capacity Guard:</strong> The system enforces a project capacity limit of
                <strong>10,000 articles</strong>. Imports that would exceed this threshold are
                blocked.
              </li>
              <li>
                <strong>Exclusion Filters:</strong> A preview list of imported papers allows you to
                review metadata and manually deselect individual rows before writing them to the
                database.
              </li>
              <li>
                <strong>Metadata Validation:</strong> Articles missing essential fields (title,
                abstract text, or authors) will raise warnings and be skipped during import.
              </li>
            </ul>
            <h3>Format Examples</h3>
            <div class="ref-example-grid">
              <div>
                <strong>RIS Format:</strong>
                <pre class="ref-code">
TY  - JOUR
TI  - AI-Assisted Systematic Review Abstraction
AU  - Soylu, Bilal
JO  - Journal of Advanced Agentic Coding
PY  - 2026
AB  - This paper describes a novel system for abstract screening...
KW  - systematic review
KW  - LLM
ER  - </pre
                >
              </div>
              <div>
                <strong>BibTeX Format:</strong>
                <pre class="ref-code">
@article{soylu2026ai,
  title = {AI-Assisted Systematic Review Abstraction},
  author = {Soylu, Bilal},
  journal = {Journal of Advanced Agentic Coding},
  year = {2026},
  abstract = {This paper describes a novel system...},
  keywords = {systematic review, LLM}
}</pre
                >
              </div>
            </div>
          </div>
        </section>

        <!-- SECTION: DUPLICATE RESOLUTION -->
        <section id="ref-dedup" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">science</span>
            <h2 class="ref-section__title">Duplicate Resolution</h2>
          </header>
          <div class="ref-section__body">
            <p>
              When combining bibliography files from multiple databases, duplicate entries are
              common. Bango runs a multi-strategy deduplication pipeline.
            </p>
            <h3>Deduplication Strategies</h3>
            <ol>
              <li>
                <strong>DOI Exact:</strong> Matches DOI strings exactly. Auto-merges duplicates.
              </li>
              <li>
                <strong>Title + Year Exact:</strong> Matches normalized title (similarity >= 95% via
                Levenshtein distance) and exact year. Auto-merges.
              </li>
              <li>
                <strong>Fuzzy Title + Year:</strong> Matches title with 70–94% similarity and exact
                year. Flags for manual resolution.
              </li>
              <li>
                <strong>Author + Title Partial:</strong> Matches first author's last name exactly
                and normalized title similarity >= 80%. Flags for manual resolution.
              </li>
            </ol>
            <h3>Screen Controls & Buttons</h3>
            <ul>
              <li>
                <strong>Merge:</strong> Resolves the conflict by linking the duplicate record to the
                parent article and marking it as read-only.
              </li>
              <li>
                <strong>Keep Both:</strong> Dismisses the duplicate flag, keeping both records in
                the active working set.
              </li>
              <li><strong>Skip:</strong> Defers decision to review later.</li>
            </ul>
          </div>
        </section>

        <!-- SECTION: TAGS & LABELS -->
        <section id="ref-tags" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">sell</span>
            <h2 class="ref-section__title">Tags & Labels</h2>
          </header>
          <div class="ref-section__body">
            <p>Categorize and track your literature using tags and labels.</p>
            <h3>Tags vs. Labels</h3>
            <ul>
              <li>
                <strong>Tags (Content categories):</strong> Used to classify the subject matter of
                an article (e.g., <code>"randomized-control"</code>, <code>"neural-network"</code>).
                Suggested by AI during screening or created manually by users.
              </li>
              <li>
                <strong>Labels (Workflow markers):</strong> Used to track organization or audit
                status (e.g., <code>"disputed"</code>, <code>"priority-read"</code>). Generated by
                AI or managed manually.
              </li>
            </ul>
            <h3>Screen Controls & Buttons</h3>
            <ul>
              <li>
                <strong>Color Swatch:</strong> Click color buttons to assign specific colors to your
                tags and labels.
              </li>
              <li>
                <strong>Create Tag/Label:</strong> Input field at the top of the panels to add a new
                category directly to the project vocabulary.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: AI SCREENING -->
        <section id="ref-screening" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">analytics</span>
            <h2 class="ref-section__title">AI Screening Setup</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The <strong>AI Screening</strong> view manages running your screening queue using
              remote or local LLM models.
            </p>
            <h3>Required Checklist</h3>
            <p>Before the screening worker can start, the system checks for:</p>
            <ul>
              <li>At least one Research Aim.</li>
              <li>At least one Inclusion Criterion and one Exclusion Criterion.</li>
              <li>A valid LLM Provider configuration and API key in Settings.</li>
            </ul>
            <h3>Behaviors & Exception Handling</h3>
            <ul>
              <li>
                <strong>Token Limit Safeguard:</strong> The system estimates input sizes for each
                abstract batch and prompts warnings if the estimated payload exceeds 80% of the
                configured context window (requires at least 50,000 tokens context).
              </li>
              <li>
                <strong>Rate Limiting (HTTP 429):</strong> If the remote provider throttles the
                connection, Bango pauses and executes automatic retries using exponential backoff.
                You can mitigate this by lowering concurrency or increasing request delays in
                Settings.
              </li>
              <li>
                <strong>Authentication Failure (HTTP 400/401):</strong> Occurs if API keys are
                invalid or revoked. Check the Diagnostics log and verify configurations in Settings.
              </li>
              <li>
                <strong>Screening Errors:</strong> API timeouts or malformed JSON responses leave
                the article in <em>Working</em> status with the "Screening Error" flag enabled. You
                can retry these individual records from the detail panel.
              </li>
            </ul>
            <h3>Screen Controls & Buttons</h3>
            <ul>
              <li><strong>Start Screening:</strong> Launches the background async queue worker.</li>
              <li>
                <strong>Pause / Resume:</strong> Safely halts the queue execution or resumes it from
                where it stopped.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: TRANSLATION PIPELINE -->
        <section id="ref-translation" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">translate</span>
            <h2 class="ref-section__title">Translations</h2>
          </header>
          <div class="ref-section__body">
            <p>
              Bango can translate non-English articles to English before AI workflows process them.
              This is a <strong>permanent rewrite</strong>: after translation, the working article
              fields hold English text, and the originals are preserved in archive tables but
              currently not visible. Translation is <strong>experimental</strong> and
              <strong>opt-in</strong>. To enable it use the <strong>Auto Translate</strong> setting
              in the Settings page.
            </p>

            <h3>How It Works</h3>
            <p>
              When <strong>Auto Translate</strong> is enabled in Settings, Bango detects the
              article's language from the RIS/BibTeX <code>language</code> field. English articles
              and articles with blank language are skipped. Non-English articles are translated
              using your configured LLM provider.
            </p>
            <p>
              The scope of translation is chosen automatically from whether the article has full
              text attached:
            </p>
            <ul>
              <li>
                <strong>MetadataOnly</strong>: Translates the title and abstract. Used during import
                when no full text is attached, and during the screening pre-translation step.
              </li>
              <li>
                <strong>FullText</strong>: Translates the title, abstract, all article chunks, then
                re-chunks the stitched English result for screening. Used when full text is attached
                and during batch import.
              </li>
            </ul>

            <h3>Implicit Translations</h3>
            <p>
              Please note: Even if you do not have <strong>Auto Translate</strong> enabled you may
              be causing implicit translations via the LLM that you use. When you submit non-english
              articles for AI screening or AI Summary the LLM you choose decides whether to
              translate these and or respond in english or the original language. Thus explicit
              translations via enabling the <strong>Auto Translate</strong>
              setting may be preferable as they will be more reproducable.
            </p>

            <h3>Translation State Machine</h3>
            <p>
              Each article tracks its translation lifecycle through five states which you can also
              review in the audit logs for articles:
            </p>
            <ul>
              <li><strong>none</strong> : Never translated. Initial state.</li>
              <li><strong>queued</strong> : Job sent to the worker. Waiting to be processed.</li>
              <li>
                <strong>running</strong> : Bango is actively translating (LLM call in flight).
              </li>
              <li><strong>succeeded</strong> : Translation complete - with a time stamp.</li>
              <li>
                <strong>failed</strong> : Translation errored. This holds the error message. You can
                retry via the translate button.
              </li>
            </ul>

            <h3>Screening-Time Translation</h3>
            <p>
              When <strong>Auto Translate</strong> is on, the screening engine explicitly runs a
              pre-translation step before the AI reads any abstracts. It queries unscreened working
              articles with non-English language, enqueues Metadata (abstract and keywords)
              translation jobs, and waits for all to complete. The screening progress bar shows
              "Translating 3/12 articles..." during this stage. The readiness check also reports
              <code>pending</code> so you know how many articles need translation before screening
              can begin.
            </p>

            <h3>Manual Translation</h3>
            <p>
              A translate button appears on the article detail header for any non-English article.
              Clicking it opens a confirmation dialog (warning about the permanent rewrite and token
              cost), then enqueues the job. Popup messages provide feedback at each stage. A red
              <strong>TRANSLATED</strong> badge appears on the header once translation succeeds.
            </p>

            <h3>Original Content Archive</h3>
            <p>
              Before the working <code>articles</code> row is rewritten, the original-language text
              is saved to two dedicated tables. These are currently not visible in UI but can be
              queried from database with other tools:
            </p>
            <ul>
              <li>
                <strong>article_original_content</strong> : Stores original title, abstract, full
                text, and the source language at translation time.
              </li>
              <li>
                <strong>article_original_chunks</strong> : Stores the pre-translation chunk
                coordinate space (section, content, word count). After translation, English chunks
                live in <code>article_chunks</code> with their own independent indices.
              </li>
            </ul>

            <h3>Batch Import Integration</h3>
            <p>
              Translations are also integrated into batch imports and imported full text pdf will
              also automatically translated if the setting is enabled.
            </p>

            <h3>Multilingual Section Classification</h3>
            <p>
              To correctly chunk non-English full text, Bango's section classifier supports 10
              languages beyond English: French, Spanish, Japanese, Chinese, German, Russian,
              Portuguese, Italian, Arabic, and Turkish. Each language maps academic section keywords
              (Abstract, Introduction, Methods, Results, Discussion, Conclusion, References) to its
              native terms. A Unicode-aware numbered-heading regex detects headings in non-Latin
              scripts (Cyrillic, CJK, Arabic).
            </p>
          </div>
        </section>

        <!-- SECTION: ARTICLE DETAIL PANELS -->
        <section id="ref-review" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">description</span>
            <h2 class="ref-section__title">Article Detail Panels</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The article viewer uses a dense 3-pane layout allowing you to read abstracts, inspect
              metadata, examine AI reasoning, and download attachments.
            </p>
            <h3>Key Features</h3>
            <ul>
              <li>
                <strong>Left Pane (Filters & Search):</strong> Query by title, year, tags, AI
                confidence, or status.
              </li>
              <li>
                <strong>Center Pane (Article Table):</strong> Scroll through matched articles and
                status indicators.
              </li>
              <li>
                <strong>Right Pane (Sliding Details Panel):</strong>
                <ul>
                  <li>
                    <strong>AI Decision Card:</strong> Shows the suggested action, matching criteria
                    list, and confidence.
                  </li>
                  <li>
                    <strong>Full-Text Attachments:</strong> Attach PDFs or TXT files. Raw texts are
                    extracted and cached locally in the full-text storage directory.
                  </li>
                  <li>
                    <strong>Inline PDF Reader:</strong> Render and read attached documents
                    side-by-side with the metadata.
                  </li>
                  <li>
                    <strong>Audit Trail Timeline:</strong> View immutable histories of actions taken
                    on the article (imports, status changes, manual overrides, error codes).
                  </li>
                </ul>
              </li>
            </ul>
            <h3>Screen Controls & Buttons</h3>
            <ul>
              <li>
                <strong>Include / Reject:</strong> Action buttons at the top of the detail panel
                that override AI-assigned decisions. These are logged in the audit trail.
              </li>
              <li>
                <strong>Attach File:</strong> File selection trigger that imports a PDF or text file
                for full-text caching.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: REFERENCES & CITATIONS -->
        <section id="ref-references-citations" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">link</span>
            <h2 class="ref-section__title">References & Citations</h2>
          </header>
          <div class="ref-section__body">
            <p>
              Bango tracks both backward references (articles cited by the paper) and forward
              citations (articles that cite the paper) for included records.
            </p>

            <div id="ref-import-references-popup" class="ref-callout">
              <h4>Import References Pop Up</h4>
              <p>
                To load specific reference datasets for a given article, click the
                <strong>Import</strong> button on the References tab in the article details panel.
                This opens the <strong>Import References Dialog</strong> where you can:
              </p>
              <ol>
                <li>
                  Select the citation direction: <strong>Backward (cited refs)</strong> or
                  <strong>Forward (cited by)</strong>.
                </li>
                <li>
                  Click <strong>Choose File</strong> to upload an RIS or BibTeX file matching that
                  citation dataset.
                </li>
                <li>
                  Review the parsed preview list, and click <strong>Add</strong> to import them.
                </li>
                <li>
                  Click the <strong>Help</strong> link in the popup header to close the dialog and
                  jump back to this reference chapter.
                </li>
              </ol>
            </div>

            <h3>Data Sources</h3>
            <p>
              Citation count fields (<code>num_cited</code> and <code>num_references</code>) are
              automatically parsed from the <code>N1</code> field of bibliography files during
              initial imports. Detailed citation lists can be obtained from:
            </p>
            <ul>
              <li>
                <strong>Web of Science:</strong> Export records selecting "Full Record and Cited
                References" in RIS format.
              </li>
              <li>
                <strong>Lens.org:</strong> Create a free account on
                <a href="https://www.lens.org" target="_blank" rel="noopener noreferrer">lens.org</a
                >, compile a collection, and export the citation/reference lists in RIS or BibTeX
                format.
              </li>
            </ul>

            <h3>Match Status States</h3>
            <p>Once references are loaded, each reference paper has one of the following states:</p>
            <ul>
              <li>
                <code>unmatched</code>: The paper is listed in the reference dataset but has not
                been found or promoted in the library.
              </li>
              <li>
                <code>matched</code>: The paper matches an existing article in the main library
                (matched by DOI or title/author/year). Clicking the link icon will take you to that
                library record.
              </li>
              <li>
                <code>imported</code>: The paper has been promoted to a full article and is now
                available in the working list.
              </li>
              <li><code>not_in_library</code>: No match exists in the current library.</li>
            </ul>
          </div>
        </section>

        <!-- SECTION: BIBLIOMETRICS -->
        <section id="ref-bibliometrics" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">hub</span>
            <h2 class="ref-section__title">Bibliometrics Analysis</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The Bibliometrics tab analyzes structural collaborations, citation density, and
              keywords across six modules: Co-Authorship, Citation Network, Keyword Co-Occurrence,
              Publication Timeline, Author Productivity, and Co-Citation Analysis. See the
              <button type="button" class="ref-link" @click="emit('switch-tab', 'biblio')">
                Understanding Bibliometrics
              </button>
              help tab for detailed explanations and use cases for each module.
            </p>
            <h3>Modularity & Layout Algorithms</h3>
            <p>
              Bango uses a local modularity engine designed to map direct working relationships:
            </p>
            <div class="ref-comparison-wrapper">
              <table class="ref-comparison-table">
                <thead>
                  <tr>
                    <th>Feature</th>
                    <th>Bango Engine</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td><strong>Normalization</strong></td>
                    <td>
                      Uses <strong>Absolute Link Weights</strong> (actual co-authored papers).
                    </td>
                  </tr>
                  <tr>
                    <td><strong>Clustering</strong></td>
                    <td>
                      Standard Louvain modularity optimization. Groups nodes into highly cohesive
                      communities.
                    </td>
                  </tr>
                  <tr>
                    <td><strong>Visual Layout</strong></td>
                    <td>
                      ForceAtlas2 force-directed model. Highlights central hubs and direct
                      departmental working groups.
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <h3>Analytical Features</h3>
            <ul>
              <li>
                <strong>Normalize:</strong> Populates the bibliometric databases by parsing active
                metadata within a single transaction to ensure maximum speed.
              </li>
              <li>
                <strong>Louvain Clustering:</strong> Groups co-authors into color-coded
                collaborative teams. Adjust the modularity slider to group or split cohorts.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: CHAT ASSISTANT -->
        <section id="ref-chat" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">chat</span>
            <h2 class="ref-section__title">Chat Assistant</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The <strong>Chat Assistant</strong> provides a conversational interface to query your
              systematic review database.
            </p>
            <h3>Key Features</h3>
            <ul>
              <li>
                <strong>RAG (Retrieval-Augmented Generation):</strong> The assistant uses your
                project's inclusion/exclusion criteria, research aims, and article abstracts as
                context to answer questions.
              </li>
              <li>
                <strong>Source Citations:</strong> Answers include direct citation badges linking to
                referenced papers. Clicking a badge opens the corresponding article details panel.
              </li>
              <li>
                <strong>Two retrieval modes:</strong> the default <strong>Article Chat</strong>
                (dumps selected article summaries into the prompt) and the token-optimized
                <strong>Wiki Chat</strong> (BM25 retrieval over the wiki index - see the
                <a href="#" @click.prevent="selectRefSection('ref-wiki')">Wiki section</a> for why
                this scales to hundreds of pages).
              </li>
            </ul>

            <h3>Bango Chat vs. Copy-Pasting into a Plain LLM / Obsidian</h3>
            <p>
              The defining cost of an LLM query is the <strong>input token count</strong> - the
              context you send with every question. Bango's two chat modes are engineered to
              minimize that cost, and the difference compounds with every follow-up question:
            </p>
            <div class="ref-comparison-wrapper">
              <table class="ref-comparison-table">
                <thead>
                  <tr>
                    <th>Approach</th>
                    <th>Tokens per question</th>
                    <th>Scales to</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td>
                      <strong>Bango Article Chat</strong><br />
                      <span class="text-xs text-slate-500">(<code>send_chat_message</code>)</span>
                    </td>
                    <td>
                      <strong>~7,500 tokens</strong> for 10 articles. Each article's
                      <code>summary_text</code> (~3,000 chars ÷ 4 chars/token) is re-sent on
                      <em>every</em> question. Fine for small, targeted selections.
                    </td>
                    <td>~20 articles before hitting typical 16K-token context limits.</td>
                  </tr>
                  <tr>
                    <td>
                      <strong>Bango Wiki Chat</strong><br />
                      <span class="text-xs text-slate-500">(<code>wiki_chat</code>)</span>
                    </td>
                    <td>
                      <strong>~3,000 tokens, flat.</strong> The question is BM25-matched against the
                      SQLite FTS5 index; only the top 3–5 matching pages are retrieved, capped at a
                      <code>12,000-char</code> (~3,000-token) budget. Overflow pages are listed as
                      titles only ("see also").
                    </td>
                    <td>
                      Hundreds of pages. Retrieval picks the relevant subset regardless of corpus
                      size - a 5-page wiki and a 500-page wiki cost roughly the same.
                    </td>
                  </tr>
                  <tr>
                    <td><strong>Manual Obsidian + LLM</strong></td>
                    <td>
                      You copy-paste notes by hand. No retrieval, no budgeting - easy to overflow
                      the context window or pay for tokens you never use.
                    </td>
                    <td>Unmanageable past ~10 notes per question.</td>
                  </tr>
                </tbody>
              </table>
            </div>

            <div class="ref-callout">
              <h4>Why the token budget matters</h4>
              <p>
                At typical LLM pricing (~$3 / 1M input tokens for mid-tier models), 100 follow-up
                questions on a 10-article selection costs about <strong>$2.25</strong> in Article
                Chat vs. <strong>$0.90</strong> in Wiki Chat - and only the latter keeps working as
                your corpus grows. The FTS5 index is built once (during wiki ingest) and queried
                offline in milliseconds; there is no per-question embedding API call.
              </p>
            </div>

            <h3>Screen Controls & Buttons</h3>
            <ul>
              <li>
                <strong>Article selection <code>(+)</code>:</strong> choose which articles to
                include as context for Article Chat.
              </li>
              <li>
                <strong>Wiki toggle:</strong> the
                <span class="material-symbols-outlined ref-inline-icon">local_library</span> icon
                (right of the <code>(+)</code> button) switches to Wiki Chat mode. Visible only when
                the wiki is initialized and has pages.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: RESEARCH GAP REPORT -->
        <section id="ref-gap-report" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">lightbulb</span>
            <h2 class="ref-section__title">Research Gap Report</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The <strong>Research Gap Report</strong> is an AI-generated analysis that identifies
              unexplored or under-explored research areas in your included article corpus. It is
              available from the <strong>Summary</strong> screen.
            </p>
            <h3>What It Does</h3>
            <ul>
              <li>
                Reads all included article summaries and key insights to build a comprehensive
                picture of the current research landscape.
              </li>
              <li>
                Identifies topics, methods, populations, or angles that are missing or
                underrepresented across your corpus.
              </li>
              <li>
                Produces a structured report with gap themes, supporting evidence from existing
                studies, and suggested research directions.
              </li>
            </ul>
            <h3>Use Cases</h3>
            <ul>
              <li>
                Framing <strong>&ldquo;future work&rdquo;</strong> sections in your own
                publications.
              </li>
              <li>Writing grant proposals that target genuine knowledge gaps.</li>
              <li>Validating that your systematic review scope is comprehensive.</li>
              <li>Identifying niche areas where your research team has a competitive advantage.</li>
            </ul>
            <h3>How to Use</h3>
            <ol>
              <li>Navigate to the <strong>Summary</strong> screen in the sidebar.</li>
              <li>Click the <strong>Research Gap Report</strong> button in the toolbar.</li>
              <li>
                The report is saved in the database and remains available until you regenerate it.
              </li>
            </ol>
            <p>
              <em>Note:</em> The gap report uses token-optimized retrieval, so even large corpora
              stay within your LLM's context window budget.
            </p>
          </div>
        </section>

        <!-- SECTION: SEARCH STRATEGY BUILDER -->
        <section id="ref-search-strategy" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">search</span>
            <h2 class="ref-section__title">Search Strategy Builder</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The <strong>Search Strategy Builder</strong> generates database-ready Boolean search
              strings from your research aims. It is available on the
              <strong>Criteria</strong> screen and uses AI to translate plain-language aims into
              structured query syntax.
            </p>
            <h3>How It Works</h3>
            <ul>
              <li>
                The AI reads all research aims from the Criteria screen to understand your review
                scope.
              </li>
              <li>
                It produces structured queries with <strong>MeSH terms</strong>,
                <strong>free-text keywords</strong>,
                <strong>Boolean operators</strong> (<code>AND</code>, <code>OR</code>,
                <code>NOT</code>), and <strong>field tags</strong> (e.g.
                <code>[Title/Abstract]</code> for PubMed).
              </li>
              <li>
                The result is displayed as a formatted card you can copy directly into PubMed,
                Scopus, Web of Science, or any other academic database.
              </li>
            </ul>
            <h3>How to Use</h3>
            <ol>
              <li>Navigate to the <strong>Criteria</strong> screen in the sidebar.</li>
              <li>Ensure you have at least one research aim and a configured LLM provider.</li>
              <li>
                Click the <strong>Generate Search Strategy</strong> button (identified by the
                sparkle icon).
              </li>
              <li>
                The result appears as a card below the button. Copy any query string with one click.
              </li>
            </ol>
            <p>
              <em>Tip:</em> Regenerate the strategy whenever you update your aims to keep your
              database queries aligned with your review scope.
            </p>
          </div>
        </section>

        <!-- SECTION: COMPLEX SCREENING RULES -->
        <section id="ref-complex-screening" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">account_tree</span>
            <h2 class="ref-section__title">Complex Screening Rules</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The <strong>Custom Screening Instructions</strong> panel (Section 4 of the
              <strong>Criteria</strong> screen) lets you define advanced logic beyond the standard
              priority-based matching: AND/OR gates, hard exclusions, and conditional inclusion
              rules that give your AI fine-grained control over screening decisions.
            </p>
            <h3>How to Write Rules</h3>
            <ul>
              <li>
                Reference criteria by their <strong>numbered position</strong> shown on the Criteria
                screen. Inclusion criteria are numbered <code>1..N</code> and exclusion criteria
                continue <code>N+1..N+M</code>, so every criterion has a unique global number.
              </li>
              <li>
                <strong>AND gates:</strong> &ldquo;Inclusion criteria 2, 3, and 4 are mandatory AND
                gates - all three must match for inclusion.&rdquo;
              </li>
              <li>
                <strong>Conditional inclusion:</strong> &ldquo;Only if criteria 2&ndash;4 are all
                satisfied, consider inclusion criterion 5 OR 6 as the final signal.&rdquo;
              </li>
              <li>
                <strong>Hard exclusions:</strong> &ldquo;Exclusion criterion 9 is a hard gate; if it
                matches, ignore inclusion criteria 11&ndash;14.&rdquo;
              </li>
              <li>
                <strong>Combined guarding:</strong> &ldquo;If inclusion 3 and 7 both match,
                exclusion 5 OR 6 must NOT match for inclusion.&rdquo;
              </li>
            </ul>
            <p>
              Click the <strong>help icon</strong> (<code>?</code>) next to the heading for a live
              syntax guide with more examples.
            </p>
            <h3>Auto-Save &amp; Check Rules</h3>
            <ul>
              <li>
                <strong>Auto-save:</strong> Your instructions save automatically when you leave the
                text area (click outside) or navigate to another screen. No manual Save button
                needed.
              </li>
              <li>
                <strong>Check Rules:</strong> The sparkle button in the section header runs an AI
                consistency review over your entire ruleset (aims, criteria, <em>and</em> custom
                instructions), flagging contradictions, ambiguity, and missing edge cases.
              </li>
              <li>
                Saved instructions are injected into every screening prompt as a
                <code>## Custom Screening Instructions</code> section.
              </li>
            </ul>
            <p>
              <em>Tip:</em> Leave the field blank for the default priority-only behavior. Rules are
              only applied when you start a new screening run.
            </p>
          </div>
        </section>

        <!-- SECTION: WIKI -->
        <section id="ref-wiki" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">local_library</span>
            <h2 class="ref-section__title">Wiki</h2>
          </header>
          <div class="ref-section__body">
            <p>
              The <strong>Wiki</strong> is a local-first, Obsidian-style knowledge base that Bango's
              LLM builds from your <em>included</em> articles. Instead of a flat list of papers, you
              get a linked, navigable synthesis of concepts, authors, methods, and themes, with
              every claim traced back to its source article.
            </p>

            <div class="ref-callout">
              <h4>Why a Wiki?</h4>
              <p>
                A systematic review often ends as a static table. The Wiki turns that table into a
                living knowledge graph: the LLM extracts entities (sugar tax, SDIL, key authors,
                methods), cross-links them with <code>[[wikilinks]]</code>, and cites each fact with
                a source reference like <code>[^art-123]</code> that jumps back to the original
                article. It is the fastest way to understand the landscape of your corpus.
              </p>
            </div>

            <h3>Why Bango Instead of Pure Obsidian?</h3>
            <p>
              You can open the generated <code>wiki-root/</code> folder in
              <a href="https://obsidian.md" target="_blank" rel="noopener noreferrer">Obsidian</a>
              as a read-only companion view (the Markdown is fully portable). But Bango and a plain
              Obsidian vault are not interchangeable - Bango's wiki is the <em>output</em> of a
              review pipeline that Obsidian has no equivalent of:
            </p>

            <h4>1. The corpus is already reviewed and normalized</h4>
            <p>
              Every page in the Bango wiki is built from articles with
              <code>status = 'included'</code> - meaning each source has already passed:
            </p>
            <ul>
              <li>
                <strong>Deduplication</strong> (4 strategies: DOI exact, title+year ≥ 95%, fuzzy
                70–94%, author+title ≥ 80%).
              </li>
              <li>
                <strong>AI screening</strong> against your aims/criteria with priority-based
                conflict resolution.
              </li>
              <li>
                <strong>Your manual review</strong> (Include/Reject overrides, full-text attachment,
                notes).
              </li>
              <li>
                <strong>Bibliometric normalization</strong> - the 8-step
                <code>biblio_normalize</code> pipeline populates canonical author, keyword, and
                institution tables that the wiki pre-seeds from (see next point).
              </li>
            </ul>
            <p>
              In Obsidian, you would manually curate which sources are worth synthesizing - there is
              no deduplicated, screened, normalized corpus to build on.
            </p>

            <h4>2. Deterministic 4-layer pre-seed matrix (before the LLM runs)</h4>
            <p>
              This is the core advantage. Before the LLM generates a single page, Bango writes a
              connected graph backbone from the normalized metadata - so the wiki is never missing
              key structural pages, regardless of which LLM model you use:
            </p>
            <div class="ref-comparison-wrapper">
              <table class="ref-comparison-table">
                <thead>
                  <tr>
                    <th>Layer</th>
                    <th>What Bango writes deterministically (no LLM)</th>
                    <th>Obsidian equivalent</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td><strong>Author pages</strong></td>
                    <td>
                      One per corpus author, with h-index, total citations, first-author count,
                      papers/year, frequent collaborators as <code>[[links]]</code> - all derived
                      from the <code>biblio_authors</code> table, not LLM-hallucinated.
                    </td>
                    <td>You write each by hand, or nothing.</td>
                  </tr>
                  <tr>
                    <td><strong>Synthesis pages</strong></td>
                    <td>
                      One per included article, built from the structured
                      <code>full_text_ai_summary</code> JSON (summary + key insights). Slug =
                      article UUID so <code>[^art-uuid]</code> citations resolve.
                    </td>
                    <td>One note per paper, written by hand.</td>
                  </tr>
                  <tr>
                    <td><strong>Concept hubs</strong></td>
                    <td>
                      Top-25 keyword pages, statistically derived from <code>biblio_terms</code>
                      co-occurrence - not guessed by the LLM.
                    </td>
                    <td>You decide what concepts matter, manually.</td>
                  </tr>
                  <tr>
                    <td><strong>Source pages</strong></td>
                    <td>
                      One per uploaded external document (PDF / web / TXT), so external sources get
                      a first-class wiki node and their citations resolve.
                    </td>
                    <td>Each external doc is an orphan note unless you link it.</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <p>
              The result: a connected graph (author ↔ synthesis ↔ concept ↔ source) exists
              <strong>before</strong> the LLM runs. The LLM then enriches it with concept
              relationships, method groupings, and cross-cutting synthesis - but the backbone is
              always there.
            </p>

            <h4>3. Ingest is batched, parallel, and self-consolidating</h4>
            <p>Bango's ingest engine is designed to handle large corpora efficiently:</p>
            <ul>
              <li>
                <strong>Token-budgeted batching:</strong> raw sources are split into batches sized
                to <code>40%</code> of your configured context window (the remainder is reserved for
                the model's output). Example: a 50K-token context → ~80K chars per batch; 20
                articles × 2,000 chars = 40,000 chars fits in <strong>one</strong> batch.
              </li>
              <li>
                <strong>Parallel dispatch:</strong> all batches run concurrently via a Bango
                Orchestrator, bounded by your available LLM Context Size. Each batch carries a
                compact full-source index so the model can link across batches.
              </li>
              <li>
                <strong>Deterministic consolidation:</strong> when multiple batches run, a no-LLM
                merge pass deduplicates near-identical pages (slug Jaccard similarity ≥ 0.5, or ≥ 2
                shared source articles), rewrites inbound links to canonical slugs, and unions the
                frontmatter. This prevents the <code>childhood-obesity</code> vs
                <code>obesity-childhood</code> fragmentation that independent LLM calls would
                otherwise produce.
              </li>
              <li>
                <strong>Cost example:</strong> a 50-article corpus on a 128K-token model typically
                ingests in 2–3 parallel batches (~$0.05–0.10 total at mid-tier pricing). The same
                corpus processed one-note-at-a-time through an Obsidian plugin would cost 10–50×
                more in API calls and produce inconsistent cross-linking.
              </li>
            </ul>

            <div class="ref-callout">
              <h4>Obsidian's role</h4>
              <p>
                Obsidian shines as a <strong>reading and annotation surface</strong> for the wiki
                Bango generates - its graph view, backlinks panel, and mobile sync are excellent for
                exploration. But it cannot <em>produce</em> the wiki: it has no dedup pipeline, no
                criteria-based screening, no bibliometric normalization, and no batched
                cross-consolidating LLM ingest. Use Bango to generate and maintain the wiki; use
                Obsidian (optionally) to read it.
              </p>
            </div>

            <h3>Where Your Documents Live</h3>
            <p>
              The Wiki is stored as plain Markdown on your disk, as a sibling of the full-text
              directory:
            </p>
            <pre class="ref-code">
~/Documents/Bango/
  fulltext/          # article PDFs and text extracts
  wiki-root/         # the LLM Wiki (plain Markdown)
    AGENTS.md        # the LLM's workflow contract (read on every ingest)
    raw/             # sources: article exports and your dropped files
    wiki/            # generated pages (concepts/ authors/ methods/ synthesis/)
      log.md         # append-only audit trail of ingest and lint runs
    templates/       # page skeletons the LLM follows</pre
            >
            <p>
              If you set a custom <strong>Storage</strong> directory in Settings, the wiki-root is
              placed under it. Every file is plain <code>.md</code> - you own it and can edit it in
              any text editor.
            </p>

            <h3>Getting Started (General Workflow)</h3>
            <p>Three prerequisites gate the Wiki, shown as readiness indicators in the toolbar:</p>
            <ol>
              <li>
                <strong>Configure an LLM provider</strong> in Settings (the Wiki uses the LLM to
                synthesize pages).
              </li>
              <li>
                <strong>Include at least one article</strong> (the Wiki is built from the
                <code>status = 'included'</code> corpus; rejected/working articles are ignored).
              </li>
              <li>
                Click <strong>Initialize Wiki</strong> (first time) or
                <strong>Rebuild Wiki</strong> in the Wiki toolbar. The one-click pipeline runs:
                <ul>
                  <li>Scaffolds the <code>wiki-root/</code> directory tree</li>
                  <li>Exports included articles as raw Markdown sources into <code>raw/</code></li>
                  <li>Processes any user-added documents into companion <code>.md</code> files</li>
                  <li>
                    Synthesizes wiki pages via the LLM (concepts, authors, methods, synthesis)
                  </li>
                  <li>Builds the FTS5 full-text search index</li>
                </ul>
              </li>
            </ol>
            <p>
              After the corpus changes (new imports, status flips to included, full-text attached),
              the toolbar shows a <strong>stale</strong> badge. Click <strong>Rebuild Wiki</strong>
              to regenerate.
            </p>

            <h3>Adding Documents</h3>
            <p>
              The <strong>Add Documents</strong> button has two on-ramps:
              <strong>From Web</strong> (paste one or more URLs; Bango fetches and extracts the
              text) and <strong>From Local Drive</strong> (pick one or more files). Added documents
              are processed into companion <code>.md</code> files and a fresh ingest runs
              automatically. Supported file types:
            </p>
            <div class="ref-comparison-wrapper">
              <table class="ref-comparison-table">
                <thead>
                  <tr>
                    <th>Format</th>
                    <th>How it is handled</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td><code>.pdf</code></td>
                    <td>Text extracted with the built-in PDF engine (same as full-text attach).</td>
                  </tr>
                  <tr>
                    <td><code>.txt</code> <code>.text</code> <code>.log</code></td>
                    <td>Read verbatim as plain text.</td>
                  </tr>
                  <tr>
                    <td><code>.html</code> <code>.htm</code></td>
                    <td>Tags stripped, entities decoded, whitespace collapsed.</td>
                  </tr>
                  <tr>
                    <td><code>.rtf</code></td>
                    <td>RTF control words stripped to clean text.</td>
                  </tr>
                  <tr>
                    <td><code>.csv</code></td>
                    <td>Parsed and rendered as a Markdown table.</td>
                  </tr>
                  <tr>
                    <td><code>.md</code></td>
                    <td>Passed through verbatim.</td>
                  </tr>
                  <tr>
                    <td>
                      <code>.json</code> <code>.xml</code> source code (<code>.rs</code>
                      <code>.py</code> <code>.js</code> <code>.ts</code> ...)
                    </td>
                    <td>Wrapped in a fenced code block (verbatim, no reformatting).</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <p>
              <em>Note:</em> Office formats (<code>.docx</code>, <code>.xlsx</code>, etc.) are not
              supported - export them to PDF or TXT first. Originals you add are always kept as the
              source of truth; the companion <code>.md</code> is regenerated idempotently.
            </p>

            <h3>Browsing & Editing Pages</h3>
            <ul>
              <li>
                <strong>Sidebar:</strong> lists all pages grouped by type (Concepts, Authors,
                Methods, Synthesis). Use the search box to filter by title or summary.
              </li>
              <li>
                <strong>Reading:</strong> <code>[[wikilinks]]</code> are clickable and navigate
                between pages. Source references (e.g. <code>[^art-123]</code>) open the article
                detail panel as a slide-over.
              </li>
              <li>
                <strong>Editing:</strong> Click <strong>Edit</strong> on any page to modify its
                title, summary, and body. The split-pane editor shows a live Markdown preview. Pages
                you mark <code>status: reviewed</code> are protected from being overwritten by the
                next LLM ingest.
              </li>
              <li>
                <strong>Graph View:</strong> An interactive network graph of all pages and their
                links (ForceAtlas2 layout, color-coded by type). Click a node to open that page.
              </li>
            </ul>

            <h3>Health Check (Lint)</h3>
            <p>
              <strong>Health Check</strong> runs a deterministic check (no LLM required) for broken
              links, orphan pages (zero inbound links), duplicate slugs, and missing frontmatter.
              Rebuilding the Wiki regenerates all pages and fixes most link/orphan issues.
            </p>

            <h3>Chat with Wiki (Token-Optimized RAG)</h3>
            <p>
              The article Chat Assistant dumps all selected article abstracts into the prompt, which
              does not scale to a wiki of hundreds of pages. The
              <strong>Wiki chat mode</strong> (toggle the Wiki icon in the Chat view, right of the
              <code>(+)</code> button) uses a token-efficient retrieval design:
            </p>
            <ul>
              <li>
                <strong>FTS5 BM25 retrieval:</strong> your question is matched against the wiki's
                SQLite full-text index (offline, no new dependencies). The top matching pages are
                retrieved, ranked by relevance.
              </li>
              <li>
                <strong>Token budget:</strong> each page's cost is estimated. If the total would
                exceed 50% of your configured context window, pages are downgraded from their full
                <code>body</code> to just their <code>summary</code> field - keeping the prompt lean
                and within limits.
              </li>
              <li>
                <strong>Cited answers:</strong> the assistant responds with citations rendered as
                clickable links that open a Wiki reader slide-over (with a back-stack for chained
                navigation).
              </li>
            </ul>

            <h3>Using with Obsidian (or any Markdown tool)</h3>
            <p>
              Because the Wiki is plain Markdown with <code>[[wikilinks]]</code>, you can open the
              <code>wiki-root/</code> folder directly in
              <a href="https://obsidian.md" target="_blank" rel="noopener noreferrer">Obsidian</a>
              (free) or any Markdown editor as a read-only companion view. Bango remains the source
              of truth: edits you make inside Bango rebuild the FTS index, so chat and search stay
              in sync. Obsidian is <em>optional</em> - everything works inside Bango without it.
            </p>

            <h3>Export Wiki as Website</h3>
            <p>
              The <strong>Export Wiki Website</strong> option (Wiki toolbar or the global Export
              menu in the article list) packages your entire wiki as a self-contained static
              <code>.zip</code> website:
            </p>
            <ul>
              <li>
                Every wiki page becomes a standalone <code>.html</code> file with full navigation.
              </li>
              <li>
                Article references resolve to metadata-only stub pages (no full text - copyright
                safe).
              </li>
              <li>
                Includes a built-in search engine, styling, and a clickable graph view with
                ForceAtlas2 layout.
              </li>
              <li>
                Extract the zip and open <code>index.html</code> in any browser - no server
                required.
              </li>
            </ul>
            <p>
              Use this to share your research synthesis with colleagues, submit as supplemental
              material, or archive your review for posterity.
            </p>

            <h3>Hosting Your Wiki Online</h3>
            <p>
              The exported wiki is a folder of static HTML files. You can host it for free on any
              static site hosting service - no backend, no database, no server configuration needed:
            </p>
            <h4>GitHub Pages</h4>
            <ul>
              <li>
                Create a GitHub repository, upload the extracted wiki folder, and enable
                <strong>GitHub Pages</strong> in the repository settings (Settings &rarr; Pages
                &rarr; deploy from main branch).
              </li>
              <li>
                Your wiki is live at
                <code>https://your-username.github.io/repo-name/</code>.
              </li>
              <li>Free, fast, and updates automatically when you push new exports.</li>
            </ul>
            <h4>Netlify</h4>
            <ul>
              <li>
                Drag and drop the extracted wiki folder onto
                <a href="https://app.netlify.com/drop" target="_blank" rel="noopener noreferrer"
                  >Netlify Drop</a
                >.
              </li>
              <li>
                Your site is live instantly with a shareable URL. Connect a custom domain for free.
              </li>
            </ul>
            <h4>Vercel</h4>
            <ul>
              <li>
                Upload the folder to a Git repository or use the
                <a href="https://vercel.com/new" target="_blank" rel="noopener noreferrer"
                  >Vercel CLI</a
                >
                (<code>vercel</code> command in the extracted folder).
              </li>
              <li>Deploys instantly with automatic HTTPS and a shareable URL.</li>
            </ul>
            <p>
              <em>Tip:</em> If you prefer keeping things offline, you can also share the extracted
              folder via cloud storage (OneDrive, Google Drive). Recipients download and open
              <code>index.html</code> in their browser - no hosting required.
            </p>

            <h3>Deleting & Resetting</h3>
            <ul>
              <li>
                <strong>Delete Wiki</strong> (Wiki toolbar) removes generated pages but keeps raw
                sources and templates. Rebuild at any time.
              </li>
              <li>
                <strong>Delete All Data</strong> (Settings) wipes the database AND the entire
                on-disk <code>wiki-root/</code> directory.
              </li>
              <li>
                <strong>Backups:</strong> the <code>.bango.json</code> backup does
                <strong>not</strong> include the Wiki directory - it lives on disk and must be
                copied manually if you want to preserve it.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: SETTINGS & SECURITY -->
        <section id="ref-settings" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">settings</span>
            <h2 class="ref-section__title">Settings & API Security</h2>
          </header>
          <div class="ref-section__body">
            <p>
              Configure AI connections, custom directories, reprocessing tasks, and project backups.
              Bango's settings are arranged into modular cards to help you manage your workspace.
            </p>

            <h3>API Key Encryption</h3>
            <p>
              To protect your credentials, LLM API keys are encrypted locally using **AES-256-GCM**.
              The decryption key is derived cryptographically from your local machine's hostname,
              username, and a secure app salt. API keys are never included in project backups.
            </p>

            <h3>Configurable Options & Cards</h3>
            <ul>
              <li>
                <strong>LLM Provider Settings:</strong> Select your AI provider (Google Gemini,
                Anthropic Claude, OpenAI, Ollama, LM Studio, or custom endpoints) and enter your
                credentials. Use the model picker to select active models.
              </li>
              <li>
                <strong>AI Summary Settings:</strong> Toggle the "Include Section Summaries" option.
                When enabled, the AI reads your PDFs section-by-section (Methods, Results,
                Discussion) to build a structured breakdown of study design, sample size,
                population, effect sizes, and limitations.
              </li>
              <li>
                <strong>AI Screening Preferences:</strong> Configure the active screening mode:
                <ul>
                  <li>
                    <em>Abstract Mode:</em> Evaluates articles using title and abstract text alone
                    (default).
                  </li>
                  <li>
                    <em>Enhanced Mode:</em> Evaluates abstract plus the top criteria-matched
                    passages from full text.
                  </li>
                  <li>
                    <em>Two-Stage Mode:</em> Screens abstracts first, then runs a full-text pass
                    only for borderline papers (confidence in configurable range, e.g.,
                    <code>[0.4, 0.7)</code>).
                  </li>
                </ul>
                Allows setting the chunk budget per article (default 2400 words) and active sections
                (default Methods, Results).
              </li>
              <li>
                <strong>File Storage:</strong> Defines the Bango documents root folder. All cached
                files, PDF attachments, scrapers, and the local Wiki files reside in subdirectories
                here (<code>fulltext/</code>, <code>ris/</code>, <code>wiki-root/</code>).
              </li>
              <li>
                <strong>Maintenance & Imports (Reprocessing):</strong>
                <ul>
                  <li>
                    <em>Rebuild Text Chunks:</em> Forces Bango to re-parse and split attached
                    full-text files into vector chunks.
                  </li>
                  <li>
                    <em>Batch Import:</em> A three-phase automated pipeline that scans your Storage
                    directory. It links PDFs to articles via DOI (Phase 1), imports Citation
                    Chaser/RIS metadata (Phase 2), and pre-generates AI summaries (Phase 3).
                  </li>
                </ul>
              </li>
              <li>
                <strong>Project Management:</strong> Contains core project options. Export backup as
                a <code>.bango.json</code> file, import a backup to restore data, reset the current
                project database, or delete all data.
              </li>
              <li>
                <strong>Diagnostics & Notification History:</strong> View previous toast messages,
                system logs, and error trails.
              </li>
            </ul>
          </div>
        </section>

        <!-- SECTION: BACKUP & RESTORE -->
        <section id="ref-backup" class="ref-section">
          <header class="ref-section__header">
            <span class="material-symbols-outlined ref-section__icon">backup</span>
            <h2 class="ref-section__title">Backup & Restore</h2>
          </header>
          <div class="ref-section__body">
            <p>
              Protect your systematic reviews by managing backups. Manage multiple projects one at a
              time by exporting / importing the current project.
            </p>
            <h3>Backup Operations</h3>
            <ul>
              <li>
                <strong>Export Backup:</strong> Exports all project variables (aims, criteria,
                articles, tags, labels, and audit logs) into a single <code>.bango.json</code> file.
                Note that system settings, LLM API keys, and the <em>Journal Index</em> reference
                tables are excluded from the backup to preserve security and app settings across
                installations.
              </li>
              <li>
                <strong>Import Backup:</strong> Restores a project from a
                <code>.bango.json</code> backup file. Importing will completely overwrite your
                current project database. A warnings modal requires explicit confirmation before
                initiating the overwrite.
              </li>
            </ul>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ht-reference {
  /* Container */
}

.ref-tab-layout {
  display: grid;
  grid-template-columns: 200px minmax(0, 1fr);
  gap: var(--space-6);
  align-items: start;
  margin-top: var(--space-4);
}

.ref-sidebar {
  position: sticky;
  top: var(--space-4);
  max-height: calc(100vh - 120px);
  overflow-y: auto;
}

.ref-nav {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  box-shadow: var(--shadow-sm);
}

.ref-nav__link {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  background: none;
  border: none;
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  font-family: inherit;
  text-align: left;
  text-decoration: none;
  border-radius: var(--radius-default);
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
  width: 100%;
}

.ref-nav__link:hover {
  background-color: rgba(79, 70, 229, 0.04);
  color: #4f46e5;
}

.ref-nav__link--active {
  background-color: #eef2ff;
  color: #4f46e5;
}

.ref-nav__icon {
  font-size: 18px;
  flex-shrink: 0;
}

.ref-content {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
  max-width: 100%;
  padding-bottom: var(--space-8);
}

.ref-section {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
  box-shadow: var(--shadow-sm);
  scroll-margin-top: var(--space-4);
}

.ref-section__header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  border-bottom: 1px solid var(--color-border);
  padding-bottom: var(--space-3);
  margin-bottom: var(--space-4);
}

.ref-section__icon {
  color: #4f46e5;
  font-size: 24px;
}

/* Inline Material Symbols icon within body prose (e.g. the `local_library` wiki
   toggle reference in the Chat section). Sized to sit on the text baseline. */
.ref-inline-icon {
  font-size: 16px;
  vertical-align: middle;
  position: relative;
  top: -1px;
  color: #4f46e5;
  user-select: none;
}

.ref-section__title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0;
}

.ref-section__body {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
}

.ref-section__body h3 {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-top: var(--space-4);
  margin-bottom: var(--space-2);
}

.ref-section__body h4 {
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-top: var(--space-3);
  margin-bottom: var(--space-1);
}

.ref-section__body ul,
.ref-section__body ol {
  margin: 0 0 var(--space-3) 0;
  padding-left: var(--space-5);
}

.ref-section__body li {
  margin-bottom: calc(var(--space-1) * 1.5);
}

.ref-section__body li:last-child {
  margin-bottom: 0;
}

.ref-example-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: var(--space-4);
  margin-top: var(--space-3);
}

@media (min-width: 992px) {
  .ref-example-grid {
    grid-template-columns: 1fr 1fr;
  }
}

.ref-code {
  background-color: #f8fafc;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  padding: var(--space-3);
  font-family: 'Fira Code', 'Cascadia Code', 'JetBrains Mono', ui-monospace, monospace;
  font-size: 11px;
  color: #334155;
  overflow-x: auto;
  margin-top: var(--space-2);
}

.ref-callout {
  background-color: #f0fdf4;
  border: 1px solid #bbf7d0;
  border-radius: var(--radius-md);
  padding: var(--space-4);
  margin: var(--space-4) 0;
}

.ref-callout h4 {
  color: #16a34a;
  margin-top: 0 !important;
  margin-bottom: var(--space-2) !important;
}

.ref-comparison-wrapper {
  overflow-x: auto;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  margin-top: var(--space-3);
}

.ref-comparison-table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--font-size-caption);
  text-align: left;
}

.ref-comparison-table th,
.ref-comparison-table td {
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  vertical-align: top;
}

.ref-comparison-table th {
  background-color: #f8fafc;
  color: var(--color-on-surface);
  font-weight: var(--font-weight-semibold);
}

.ref-comparison-table tr:last-child td {
  border-bottom: none;
}

@media (max-width: 767px) {
  .ref-tab-layout {
    grid-template-columns: 1fr;
  }
  .ref-sidebar {
    position: static;
    max-height: none;
  }
}

.ref-link {
  display: inline;
  padding: 0;
  margin: 0;
  background: none;
  border: none;
  color: #4f46e5;
  text-decoration: none;
  font-weight: var(--font-weight-semibold);
  font-family: inherit;
  font-size: inherit;
  cursor: pointer;
}

.ref-link:hover {
  text-decoration: underline;
}
</style>
