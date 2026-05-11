<script setup lang="ts">
import { useRouter } from 'vue-router';

const router = useRouter();

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

function navigateTo(route: string): void {
  router.push(route);
}
</script>

<template>
  <div class="help-guide">
    <!-- Page Header -->
    <section class="help-guide__header">
      <h1 class="page-title">How to Use Bango</h1>
      <p class="help-guide__subtitle">
        A step-by-step walkthrough of the screening workflow. Click any step to navigate directly to
        that screen.
      </p>
    </section>

    <!-- Workflow Overview -->
    <section class="help-guide__overview">
      <div class="help-guide__overview-card">
        <div class="help-guide__overview-icon material-symbols-outlined">route</div>
        <div class="help-guide__overview-text">
          <h3 class="help-guide__overview-title">The Big Picture</h3>
          <p class="help-guide__overview-desc">
            Bango follows the standard systematic review process: you import search results from
            academic databases, remove duplicates, define what you are looking for, and then let AI
            help you screen each article's title and abstract. You always have the final say on
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
  margin-bottom: var(--space-6);
}

.help-guide__subtitle {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-body);
  margin-top: var(--space-2);
  line-height: var(--line-height-body);
}

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
}
</style>
