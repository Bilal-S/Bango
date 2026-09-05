<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import '@/styles/help-shared.css';

const router = useRouter();

/**
 * The Zotero file-renaming template. Produces filenames like `10.1001_art1.pdf`
 * that match Bango's Batch Import `clean_doi_filename` convention, so Phase 1
 * can auto-attach PDFs to articles by DOI.
 *
 * Source: https://www.zotero.org/support/file_renaming
 */
const ZOTERO_TEMPLATE =
  '{{ if DOI }}{{ DOI replaceFrom="[/\\?%*:|\\"<>]" replaceTo="_" regexOpts="g" }}{{ else }}{{ title truncate="40" replaceFrom="[/\\?%*:|\\"<>]" replaceTo="_" regexOpts="g" }}{{ endif }}';

interface ZoteroStep {
  step: number;
  title: string;
  icon: string;
  summary: string;
  details: string[];
  /** Renders the Zotero file-renaming template block + Copy button on this step. */
  hasTemplate?: boolean;
  /** Optional "Go to" button: route path + label. Omit for external-tool steps. */
  goTo?: { route: string; label: string };
}

/**
 * One of the two Zotero data-moving paths. Rendered in order with a big
 * "- OR -" separator between them: users pick either the automatic API route
 * or the manual import/export route, never both.
 */
interface ZoteroPath {
  key: 'api' | 'manual';
  title: string;
  steps: ZoteroStep[];
}

/** Path A: the automatic route. One prerequisite step powers import + export. */
const apiSteps: ZoteroStep[] = [
  {
    step: 1,
    title: 'Enable the Zotero local API (recommended)',
    icon: 'settings',
    summary:
      'One setting unlocks the direct path: Import from Zotero pulls a whole collection (metadata, tags, notes, and full-text PDFs) into Bango, and the Zotero export option syncs results (including your notes) back - no file juggling needed.',
    details: [
      'Open Zotero and go to Edit > Settings (Windows/Linux) or Zotero > Settings (macOS).',
      'Select the Advanced tab.',
      'Tick "Allow other applications on this computer to communicate with Zotero".',
      'In Bango, go to the Import tab and click "Import from Zotero". Bango connects, you pick a collection, review the preview, and confirm.',
      'Zotero must be running while you import. Exporting back to Zotero (from the Bango Export dialog) needs Zotero 10 or newer and uses the same setting.',
    ],
    goTo: { route: '/import', label: 'Import' },
  },
];

/** Path B: the manual route via RIS export/import + full-text file copy. */
const manualSteps: ZoteroStep[] = [
  {
    step: 1,
    title: 'Collect articles in Zotero',
    icon: 'download',
    summary:
      'Zotero is a free, open-source reference manager. Use it to collect articles and their full-text PDFs from academic databases, publisher websites, or anywhere on the web.',
    details: [
      'Download Zotero from zotero.org and install the Zotero Connector browser extension.',
      'When browsing PubMed, Scopus, Google Scholar, or publisher pages, click the Zotero Connector icon in your browser toolbar to save the article.',
      'Zotero automatically captures the title, authors, abstract, journal, year, and DOI.',
      'If a PDF is available, Zotero can download and attach it automatically (this is enabled by default in Settings).',
      'Organize your articles into a collection (folder) for your review. You will export this collection in step 3.',
    ],
  },
  {
    step: 2,
    title: 'Set up automatic file renaming',
    icon: 'drive_file_rename_outline',
    hasTemplate: true,
    summary:
      'Configure Zotero to rename PDF files using the article DOI. This produces filenames that Bango Batch Import can match automatically.',
    details: [
      'Open Zotero and go to Edit > Settings (Windows/Linux) or Zotero > Settings (macOS).',
      'Select the General tab.',
      'Under "File Renaming", click the "Configure File Renaming..." button.',
      'Paste the template below into the text box, then click OK to save.',
      'From now on, any new PDF you save will be renamed automatically. To rename PDFs you already have, right-click them and choose "Rename File from Parent Metadata".',
    ],
  },
  {
    step: 3,
    title: 'Export articles as RIS',
    icon: 'file_export',
    summary: 'Export your Zotero collection as an RIS file that Bango can import.',
    details: [
      'In Zotero, right-click the collection you organized in step 1.',
      'Choose "Export Collection..." from the menu.',
      'In the export dialog, select "RIS" as the format.',
      'Click OK and choose where to save the `.ris` file.',
      'This RIS file contains all the metadata (title, authors, abstract, DOI, journal, year) for every article in your collection.',
    ],
    goTo: { route: '/import', label: 'Import' },
  },
  {
    step: 4,
    title: 'Copy PDF files to Bango full-text folder',
    icon: 'content_copy',
    summary: 'Copy your renamed PDF files from Zotero storage into the Bango full-text directory.',
    details: [
      'In Zotero, go to Edit > Settings > Advanced > Files and Folders, then click "Show Data Directory".',
      'Your file manager opens the Zotero data folder. Open the `storage` subfolder inside it.',
      'In the `storage` folder, search for all PDF files. On Windows, type `*.pdf` in the search box. On macOS, use the Finder search bar.',
      'Select all the PDF files and copy them (Ctrl+C on Windows/Linux, Cmd+C on macOS).',
      'In Bango, go to Settings and find the "Storage" card. Note the storage root path. Open that folder in your file manager, then open the `fulltext` subfolder inside it.',
      'Paste the PDF files into the `fulltext` folder. They are now in place for Bango to find in step 6.',
    ],
  },
  {
    step: 5,
    title: 'Import the RIS file into Bango',
    icon: 'upload_file',
    summary: 'Import the RIS file you exported from Zotero into Bango.',
    details: [
      'In Bango, go to the Import tab from the sidebar.',
      'Click the file browser button and select the `.ris` file you saved in step 3.',
      'Bango shows a preview of the articles it found in the file. Review them and deselect any you do not want.',
      'Click "Import" to add the articles to your project.',
      'Bango automatically checks for duplicate records and links each article to its journal in the reference database.',
    ],
    goTo: { route: '/import', label: 'Import' },
  },
  {
    step: 6,
    title: 'Run Batch Import to attach full text',
    icon: 'playlist_play',
    summary:
      'Run Bango Batch Import to match and attach your full-text PDFs to the articles by DOI.',
    details: [
      'Go to Settings > Reprocessing and find the "Batch Import" section.',
      'Click the "Import full text files" button.',
      'A dialog explains the process. Click "Start" to begin.',
      'Phase 1 scans your `fulltext` folder for PDFs and matches them to articles by DOI. Each matched PDF is extracted and its text is stored with the article.',
      'You can watch the progress bar as it works. You can cancel at any time.',
      'When complete, your articles have full-text content attached. They are ready for enhanced AI screening and the Wiki knowledge base.',
    ],
    goTo: { route: '/settings', label: 'Settings' },
  },
];

/** Render order: the API path, the "- OR -" separator, then the manual path. */
const paths: ZoteroPath[] = [
  { key: 'api', title: 'Automatically Moving Data via API', steps: apiSteps },
  { key: 'manual', title: 'Manually Moving Data via Import/Export', steps: manualSteps },
];

/**
 * Navigate to a Bango route (used by the "Go to" buttons on the API step and
 * on manual steps 3, 5, and 6).
 */
function navigateTo(route: string): void {
  router.push(route);
}

const copied = ref(false);

/**
 * Copy the Zotero file-renaming template to the clipboard so the user can paste
 * it straight into the Zotero settings dialog without retyping the escape-heavy
 * template by hand.
 */
async function copyTemplate(): Promise<void> {
  try {
    await navigator.clipboard.writeText(ZOTERO_TEMPLATE);
    copied.value = true;
    setTimeout(() => {
      copied.value = false;
    }, 2000);
  } catch {
    // Clipboard API can fail in some webview contexts; the user can still
    // select-and-copy the <pre> block manually.
    copied.value = false;
  }
}
</script>

<template>
  <div class="ht-zotero" role="tabpanel">
    <!-- Overview -->
    <section class="ht-zotero__overview">
      <div class="ht-zotero__overview-card">
        <div class="ht-zotero__overview-icon material-symbols-outlined">route</div>
        <div class="ht-zotero__overview-text">
          <h3 class="ht-zotero__overview-title">Zotero and Bango, together</h3>
          <p class="ht-zotero__overview-desc">
            Zotero is excellent for collecting articles and their PDFs. Bango is excellent for
            AI-assisted screening and analysis. Below, choose one of two paths to move your Zotero
            data into Bango: let the apps talk directly via the Zotero local API, or move files
            yourself via RIS export/import and the full-text folder.
          </p>
        </div>
      </div>
    </section>

    <!-- Two paths: automatic (API) or manual (import/export), split by a big "- OR -" -->
    <template v-for="(path, pathIdx) in paths" :key="path.key">
      <section class="ht-zotero__path">
        <h2 class="ht-zotero__path-title">{{ path.title }}</h2>
        <div class="ht-guide__steps">
          <div v-for="(step, idx) in path.steps" :key="`${path.key}-${step.step}`" class="ht-step">
            <div class="ht-step__indicator">
              <div class="ht-step__number">{{ step.step }}</div>
              <div v-if="idx < path.steps.length - 1" class="ht-step__line" />
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
                <li v-for="(detail, dIdx) in step.details" :key="dIdx" class="ht-step__detail">
                  {{ detail }}
                </li>
              </ul>

              <!-- Zotero file-renaming template block with Copy button (rename step only) -->
              <div v-if="step.hasTemplate" class="ht-zotero__template">
                <div class="ht-zotero__template-header">
                  <span class="ht-zotero__template-label">Zotero file-renaming template</span>
                  <button
                    class="ht-zotero__copy-btn"
                    :class="{ 'ht-zotero__copy-btn--done': copied }"
                    type="button"
                    @click="copyTemplate"
                  >
                    <span class="material-symbols-outlined ht-zotero__copy-icon">{{
                      copied ? 'check' : 'content_copy'
                    }}</span>
                    {{ copied ? 'Copied' : 'Copy' }}
                  </button>
                </div>
                <pre class="ht-zotero__pre"><code>{{ ZOTERO_TEMPLATE }}</code></pre>
              </div>

              <!-- Optional "Go to" button -->
              <button
                v-if="step.goTo"
                class="ht-step__go-btn"
                type="button"
                @click="navigateTo(step.goTo.route)"
              >
                <span class="material-symbols-outlined ht-step__go-icon">arrow_forward</span>
                Go to {{ step.goTo.label }}
              </button>
            </div>
          </div>
        </div>

        <!-- No-DOI warning callout: Batch Import DOI matching is a manual-path concern -->
        <section v-if="path.key === 'manual'" class="ht-zotero__callout" role="note">
          <span class="material-symbols-outlined ht-zotero__callout-icon">warning</span>
          <div class="ht-zotero__callout-body">
            <h4 class="ht-zotero__callout-title">Important: only articles with a DOI auto-match</h4>
            <p class="ht-zotero__callout-text">
              Bango Batch Import matches PDFs to articles using the DOI in the filename. Articles
              that have no DOI cannot be auto-matched, even if Zotero renames their PDF using the
              title. For no-DOI articles you have two options: add the DOI in the article metadata
              first and run Batch Import again, or attach the PDF manually from the article detail
              panel using the "Attach Full Text" button.
            </p>
          </div>
        </section>
      </section>

      <!-- Big "- OR -" separator between the two paths -->
      <div v-if="pathIdx < paths.length - 1" class="ht-zotero__or" role="separator" aria-label="or">
        <span class="ht-zotero__or-line" aria-hidden="true"></span>
        <span class="ht-zotero__or-label">- OR -</span>
        <span class="ht-zotero__or-line" aria-hidden="true"></span>
      </div>
    </template>
  </div>
</template>

<style scoped>
/* Overview card - same look as help-tab-guide.vue */
.ht-zotero__overview {
  margin-bottom: var(--space-6);
}

.ht-zotero__overview-card {
  display: flex;
  gap: var(--space-5);
  background-color: #eef2ff;
  border: 1px solid #c7d2fe;
  border-radius: var(--radius-md);
  padding: var(--space-5);
}

.ht-zotero__overview-icon {
  font-size: 28px;
  color: #4f46e5;
  flex-shrink: 0;
}

.ht-zotero__overview-title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-2) 0;
}

.ht-zotero__overview-desc {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

/* Two-path layout: per-path section title + the big "- OR -" separator */
.ht-zotero__path-title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-4) 0;
}

.ht-zotero__or {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.ht-zotero__or-line {
  flex: 1;
  height: 2px;
  background-color: #c7d2fe;
}

.ht-zotero__or-label {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-bold);
  color: #4f46e5;
  letter-spacing: 0.08em;
  white-space: nowrap;
}

/* No-DOI warning callout */
.ht-zotero__callout {
  display: flex;
  gap: var(--space-3);
  align-items: flex-start;
  background-color: #fffbeb;
  border: 1px solid #fcd34d;
  border-radius: var(--radius-md);
  padding: var(--space-4);
  margin-bottom: var(--space-6);
}

.ht-zotero__callout-icon {
  font-size: 22px;
  color: #b45309;
  flex-shrink: 0;
  margin-top: 2px;
}

.ht-zotero__callout-title {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: #92400e;
  margin: 0 0 var(--space-1) 0;
}

.ht-zotero__callout-text {
  font-size: var(--font-size-body);
  color: #78350f;
  line-height: var(--line-height-body);
  margin: 0;
}

/* Zotero template block + copy button */
.ht-zotero__template {
  margin: 0 0 var(--space-3) 0;
}

.ht-zotero__template-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  margin-bottom: var(--space-2);
}

.ht-zotero__template-label {
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface-variant);
}

.ht-zotero__copy-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-3);
  background-color: #ffffff;
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

.ht-zotero__copy-btn:hover {
  background-color: #eef2ff;
}

.ht-zotero__copy-btn--done {
  color: #047857;
  border-color: #6ee7b7;
  background-color: #ecfdf5;
}

.ht-zotero__copy-icon {
  font-size: 16px;
}

.ht-zotero__pre {
  margin: 0;
  padding: var(--space-3);
  background-color: #1f2937;
  color: #f9fafb;
  border-radius: var(--radius-md);
  /* Wrap long template lines instead of horizontal-scrolling, so the dark box
     never overflows its card on narrow viewports. `pre-wrap` preserves internal
     whitespace while allowing soft wraps at whitespace; `overflow-wrap: anywhere`
     is defense-in-depth so a long unbreakable token (e.g. the regex char class)
     also breaks at the box edge if it lands with no surrounding space. */
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-family:
    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New',
    monospace;
  font-size: var(--font-size-caption);
  line-height: 1.5;
}

.ht-zotero__pre code {
  font-family: inherit;
  background: none;
  color: inherit;
  padding: 0;
}

@media (max-width: 767px) {
  .ht-zotero__overview-card {
    flex-direction: column;
  }

  .ht-step__card-header {
    flex-direction: column;
  }
}
</style>
