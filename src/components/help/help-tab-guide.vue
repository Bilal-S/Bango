<script setup lang="ts">
import { onMounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useDemo } from '@/composables/use-demo';
import '@/styles/help-shared.css';

/**
 * User Guide tab.
 *
 * Overview + numbered steps + demo. Accepts an `initialHash` prop (from the
 * parent shell's route hash) so deep-links like `/help?tab=guide#starting-points`
 * scroll to the right section on mount (mirrors the Reference tab).
 */

const props = defineProps<{
  initialHash?: string;
}>();

const router = useRouter();
const { demoLoading, demoError, loadDemo } = useDemo(router);

/** Scroll to the element matching `hashId` (without the leading `#`). Smooth +
 *  respects `scroll-margin-top` so the title clears the sticky help-tabs bar. */
function scrollToHashId(hashId: string): void {
  requestAnimationFrame(() => {
    document.getElementById(hashId)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  });
}

onMounted(() => {
  if (!props.initialHash) return;
  const hashId = props.initialHash.startsWith('#') ? props.initialHash.slice(1) : props.initialHash;
  // Only scroll for anchor hashes (start with a letter), not arbitrary browser
  // state hashes. Keeps this from misfiring on unrelated hash changes.
  if (hashId && /^[a-z]/i.test(hashId)) {
    scrollToHashId(hashId);
  }
});

// Re-apply hash when the parent updates the prop (e.g. navigating while mounted).
watch(
  () => props.initialHash,
  (newHash) => {
    if (!newHash) return;
    const hashId = newHash.startsWith('#') ? newHash.slice(1) : newHash;
    if (hashId && /^[a-z]/i.test(hashId)) {
      scrollToHashId(hashId);
    }
  }
);

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
      'If your project includes non-English articles, enable <strong>Auto Translate</strong> in Settings (off by default). Bango will translate non-English abstracts to English before the AI reads them, showing a "Translating..." progress stage.',
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

function navigateTo(route: string): void {
  router.push(route);
}
</script>

<template>
  <div class="ht-guide" role="tabpanel">
    <!-- Workflow Overview -->
    <section class="ht-guide__overview">
      <div class="ht-guide__overview-card">
        <div class="ht-guide__overview-icon material-symbols-outlined">route</div>
        <div class="ht-guide__overview-text">
          <h3 class="ht-guide__overview-title">The Big Picture</h3>
          <p class="ht-guide__overview-desc">
            Bango follows the standard systematic review process: you bring in search results from
            academic databases or OpenAlex, remove duplicates, define what you are looking for, and
            then let AI help you screen each article's title and abstract. You always have the final
            say on every decision. When you are done, Bango generates a PRISMA flow diagram and a
            summary of your included literature. You can start from whichever entry point fits your
            workflow - see <a href="#starting-points">Starting Points</a> below.
          </p>
        </div>
      </div>
    </section>

    <!-- Starting Points: three valid entry paths into the review workflow.
         Deep-link anchor #starting-points is targeted by the Dashboard's
         "Start New Project" info dialog. -->
    <section id="starting-points" class="ht-guide__starting-points">
      <h3 class="ht-guide__sp-title">
        <span class="material-symbols-outlined ht-guide__sp-icon">alt_route</span>
        Starting Points - Where Should I Begin?
      </h3>
      <p class="ht-guide__sp-intro">
        There is no single "correct" first step in Bango. Choose the entry point that best matches
        where you are in your research. All three paths converge on the same screening, review, and
        reporting pipeline.
      </p>
      <div class="ht-guide__sp-grid">
        <div class="ht-guide__sp-card">
          <div class="ht-guide__sp-card-header">
            <span class="material-symbols-outlined">rule</span>
            <h4>1. Aims & Criteria First</h4>
          </div>
          <p>
            Best for a brand-new review. Visit the
            <strong>Criteria</strong> screen to write your research aims and inclusion/exclusion
            rules, then import or search for articles and run AI screening.
          </p>
          <button class="ht-guide__sp-btn" @click="navigateTo('/criteria')">
            Go to Criteria
            <span class="material-symbols-outlined">arrow_forward</span>
          </button>
        </div>
        <div class="ht-guide__sp-card">
          <div class="ht-guide__sp-card-header">
            <span class="material-symbols-outlined">upload_file</span>
            <h4>2. Articles First</h4>
          </div>
          <p>
            Already have an RIS or BibTeX export from PubMed, Scopus, or another database? Import it
            on the <strong>Import</strong> screen, then define your criteria before screening.
          </p>
          <button class="ht-guide__sp-btn" @click="navigateTo('/import')">
            Go to Import
            <span class="material-symbols-outlined">arrow_forward</span>
          </button>
        </div>
        <div class="ht-guide__sp-card">
          <div class="ht-guide__sp-card-header">
            <span class="material-symbols-outlined">database_search</span>
            <h4>3. Search & Discover</h4>
          </div>
          <p>
            Use the <strong>Search Strategy Builder</strong> (Criteria screen) to generate Boolean
            queries for 8 databases, then search <strong>OpenAlex</strong> (300M+ works) and import
            the articles you discover directly into your working set.
          </p>
          <button class="ht-guide__sp-btn" @click="navigateTo('/criteria')">
            Go to Search Strategy
            <span class="material-symbols-outlined">arrow_forward</span>
          </button>
        </div>
      </div>
    </section>

    <!-- Steps -->
    <section class="ht-guide__steps">
      <div v-for="step in steps" :key="step.step" class="ht-step">
        <div class="ht-step__indicator">
          <div class="ht-step__number">{{ step.step }}</div>
          <div v-if="step.step < steps.length" class="ht-step__line" />
        </div>
        <div class="ht-step__card">
          <div class="ht-step__card-header">
            <span class="material-symbols-outlined ht-step__icon">{{ step.icon }}</span>
            <div class="ht-step__card-title-area">
              <h3 class="ht-step__title">{{ step.title }}</h3>
              <p class="ht-step__summary">{{ step.summary }}</p>
            </div>
          </div>
          <ul class="ht-step__details">
            <li v-for="(detail, idx) in step.details" :key="idx" class="ht-step__detail">
              {{ detail }}
            </li>
          </ul>
          <button class="ht-step__go-btn" @click="navigateTo(step.route)">
            <span class="material-symbols-outlined ht-step__go-icon">arrow_forward</span>
            Go to {{ step.routeLabel }}
          </button>
        </div>
      </div>
    </section>

    <!-- Getting Help Footer -->
    <section class="ht-footer">
      <div class="ht-footer-card">
        <span class="material-symbols-outlined ht-footer-icon">settings</span>
        <div>
          <h4 class="ht-footer-title">Need to configure AI?</h4>
          <p class="ht-footer-desc">
            Before AI screening can run, you need to set up a connection to an AI provider. Go to
            <strong>Settings</strong> in the sidebar to enter your provider details and API key.
          </p>
        </div>
        <button class="ht-footer-btn" @click="navigateTo('/settings')">Open Settings</button>
      </div>
    </section>

    <!-- Managing Your Project: single-project model + start-fresh workflow.
         Mirrors the Dashboard's "Start New Project" info dialog content so the
         user has a durable reference for how to begin a new review. -->
    <section class="ht-manage">
      <div class="ht-manage-card">
        <span class="material-symbols-outlined ht-manage-icon">settings_backup_restore</span>
        <div class="ht-manage-body">
          <h4 class="ht-manage-title">Managing Your Project</h4>
          <p class="ht-manage-desc">
            Bango manages <strong>one project at a time</strong>. To work on a different review,
            back up your current project first, then delete all data and begin fresh.
          </p>
          <ol class="ht-manage-steps">
            <li>
              <strong>Export Backup</strong> - In Settings &rarr; Project Management, click "Export
              Backup" to save a <code>.bango.json</code> file.
            </li>
            <li>
              <strong>Delete All Data</strong> - Use "Delete All Data" to clear the database (this
              also removes the on-disk Wiki and full-text PDFs).
            </li>
            <li>
              <strong>Begin fresh</strong> - Define new criteria, import new articles, or search
              OpenAlex. You can restore the old project anytime via "Import Backup".
            </li>
          </ol>
          <p class="ht-manage-note">
            Need more detail? See the
            <button
              type="button"
              class="ht-manage-note-link"
              @click="navigateTo('/help?tab=reference#ref-backup')"
            >
              Backup & Restore
            </button>
            reference section.
          </p>
        </div>
        <button class="ht-manage-btn" @click="navigateTo('/settings')">
          <span class="material-symbols-outlined">settings</span>
          Open Settings
        </button>
      </div>
    </section>

    <!-- Demo Tile -->
    <section class="ht-demo">
      <div class="ht-demo-card">
        <span class="material-symbols-outlined ht-demo-icon">science</span>
        <div class="ht-demo-body">
          <h4 class="ht-demo-title">Try the Demo</h4>
          <p class="ht-demo-desc">
            Load a sample project with articles, criteria, and research aims to explore Bango's
            features without setting up your own data. This will replace any existing project data.
          </p>
          <p v-if="demoError" class="ht-demo-error">{{ demoError }}</p>
        </div>
        <button class="ht-demo-btn" :disabled="demoLoading" @click="loadDemo()">
          <span v-if="demoLoading" class="material-symbols-outlined ht-demo-spinner">
            progress_activity
          </span>
          <span v-else class="material-symbols-outlined ht-demo-btn-icon">play_circle</span>
          {{ demoLoading ? 'Loading…' : 'Load Demo Project' }}
        </button>
      </div>
    </section>

    <!-- Developer & License -->
    <section class="ht-about">
      <div class="ht-about-card">
        <span class="material-symbols-outlined ht-about-icon">info</span>
        <div class="ht-about-body">
          <h4 class="ht-about-title">About Bango</h4>
          <p class="ht-about-desc">
            Developed by <strong>BonCode (Bilal Soylu)</strong> with permission from
            <strong>Startup Strategy Advisors LLC</strong>. Released as open source under the
            <strong>Apache License 2.0</strong>.
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
.ht-guide {
  /* Container only; uses shared .ht-* classes for footer/demo/about */
}

/* Overview Card */
.ht-guide__overview {
  margin-bottom: var(--space-8);
}

.ht-guide__overview-card {
  display: flex;
  gap: var(--space-5);
  background-color: #eef2ff;
  border: 1px solid #c7d2fe;
  border-radius: var(--radius-md);
  padding: var(--space-5);
}

.ht-guide__overview-icon {
  font-size: 28px;
  color: #4f46e5;
  flex-shrink: 0;
}

.ht-guide__overview-title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-2) 0;
}

.ht-guide__overview-desc {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

@media (max-width: 767px) {
  .ht-guide__overview-card {
    flex-direction: column;
  }

  /* .ht-step__card-header responsive rule lives in help-shared.css. */
}

/* ===================== Starting Points section ===================== */
.ht-guide__starting-points {
  margin-bottom: var(--space-8);
  /* Clear the sticky help-tabs nav bar (~48-56px) when deep-link scrolled
     via `/help?tab=guide#starting-points`. Mirrors `.ref-section`. */
  scroll-margin-top: 80px;
}

.ht-guide__sp-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-2) 0;
}

.ht-guide__sp-icon {
  font-size: 24px;
  color: #4f46e5;
}

.ht-guide__sp-intro {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0 0 var(--space-5) 0;
}

.ht-guide__sp-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--space-4);
}

@media (max-width: 899px) {
  .ht-guide__sp-grid {
    grid-template-columns: 1fr;
  }
}

.ht-guide__sp-card {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  box-shadow: var(--shadow-sm);
}

.ht-guide__sp-card-header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.ht-guide__sp-card-header .material-symbols-outlined {
  font-size: 20px;
  color: #4f46e5;
}

.ht-guide__sp-card-header h4 {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0;
}

.ht-guide__sp-card p {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
  flex: 1;
}

.ht-guide__sp-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  background-color: #eef2ff;
  color: #4f46e5;
  border: 1px solid #c7d2fe;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  font-family: inherit;
  transition: background-color 0.15s;
  align-self: flex-start;
}

.ht-guide__sp-btn:hover {
  background-color: #e0e7ff;
}

.ht-guide__sp-btn .material-symbols-outlined {
  font-size: 16px;
}

/* ===================== Managing Your Project section ===================== */
.ht-manage {
  margin-top: var(--space-8);
  margin-bottom: var(--space-6);
}

.ht-manage-card {
  display: flex;
  align-items: flex-start;
  gap: var(--space-4);
  background-color: #f0fdf4;
  border: 1px solid #bbf7d0;
  border-radius: var(--radius-md);
  padding: var(--space-5);
}

.ht-manage-icon {
  font-size: 28px;
  color: #15803d;
  flex-shrink: 0;
}

.ht-manage-body {
  flex: 1;
  min-width: 0;
}

.ht-manage-title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-2) 0;
}

.ht-manage-desc {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0 0 var(--space-3) 0;
}

.ht-manage-steps {
  margin: 0 0 var(--space-3) 0;
  padding-left: 1.3rem;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.ht-manage-steps li {
  font-size: var(--font-size-caption);
  line-height: var(--line-height-body);
  color: var(--color-on-surface-variant);
}

.ht-manage-steps code {
  font-family: var(--font-family-mono);
  font-size: 12px;
  background-color: rgba(21, 128, 61, 0.1);
  padding: 1px 4px;
  border-radius: 3px;
}

.ht-manage-note {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  margin: 0;
}

.ht-manage-note a {
  color: #15803d;
  font-weight: var(--font-weight-semibold);
}

/* Inline link-styled button inside the "Need more detail?" note. Uses a
   <button> instead of an <a> so vue-router does not intercept it as a
   route (the old `<a href="/help?tab=reference#ref-backup">` produced a
   "No match found for /ref-backup" router warning). */
.ht-manage-note-link {
  display: inline;
  background: none;
  border: none;
  padding: 0;
  margin: 0;
  color: #15803d;
  font-weight: var(--font-weight-semibold);
  font-size: inherit;
  font-family: inherit;
  text-decoration: underline;
  cursor: pointer;
}

.ht-manage-note-link:hover {
  text-decoration: underline;
  color: #166534;
}

.ht-manage-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  background-color: #15803d;
  color: #ffffff;
  border: none;
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  font-family: inherit;
  white-space: nowrap;
  flex-shrink: 0;
  transition: background-color 0.15s;
}

.ht-manage-btn:hover {
  background-color: #166534;
}

.ht-manage-btn .material-symbols-outlined {
  font-size: 16px;
}

@media (max-width: 767px) {
  .ht-manage-card {
    flex-direction: column;
  }
}
</style>
