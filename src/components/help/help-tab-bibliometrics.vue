<script setup lang="ts">
import '@/styles/help-shared.css';

/**
 * Understanding Bibliometrics tab.
 *
 * Documents all six completed bibliometric analysis modules with concept,
 * implementation details, key controls, and use cases. Content is sourced from
 * the v4 spec and the actual UI controls in the biblio-* views / *-controls.vue.
 */
</script>

<template>
  <div class="ht-biblio" role="tabpanel">
    <!-- Intro -->
    <section class="ht-intro">
      <h2 class="ht-intro__title">Understanding Bibliometrics</h2>
      <p class="ht-intro__desc">
        Bibliometrics is the statistical analysis of books, articles, and other scholarly
        publications. It helps researchers map scientific fields, trace citation pathways, quantify
        author impact, and visualize collaborations across institutions and research groups.
      </p>
      <p class="ht-intro__desc" style="margin-top: 12px">
        Bango includes six interconnected bibliometric modules. They all operate on your project's
        <strong>included articles</strong> and the citation/reference data you have imported. Before
        any module can display results, you must run <strong>Refresh</strong> from the Bibliometrics
        dashboard once to build the analytical data layer.
      </p>
    </section>

    <!-- Prerequisite callout -->
    <section class="ht-biblio__callout">
      <div class="ht-biblio__callout-card">
        <span class="material-symbols-outlined ht-biblio__callout-icon">play_circle</span>
        <div>
          <h4 class="ht-biblio__callout-title">Getting Started</h4>
          <p class="ht-biblio__callout-desc">
            Open <strong>Bibliometrics</strong> in the sidebar and click the
            <strong>Normalize</strong>
            button. This single transaction parses authors, institutions, terms, and citation links
            from your active metadata. After it completes, all six analysis modules become available
            from the dashboard tiles.
          </p>
        </div>
      </div>
    </section>

    <!-- =========================================================== -->
    <!-- MODULE 1: CO-AUTHORSHIP NETWORK -->
    <!-- =========================================================== -->
    <section class="bm-section">
      <header class="bm-section__header">
        <span class="material-symbols-outlined bm-section__icon" style="color: #6366f1">group</span>
        <div>
          <h3 class="bm-section__title">1. Co-Authorship Network</h3>
          <p class="bm-section__subtitle">
            Map collaborative relationships between researchers based on joint publications.
          </p>
        </div>
      </header>

      <div class="bm-section__body">
        <h4 class="bm-body__heading">What it measures</h4>
        <p>
          A co-authorship network represents researchers as nodes and their joint publications as
          edges. Two authors who have published together get a link, and the more papers they
          co-author, the stronger that link becomes. The resulting map reveals research teams,
          institutional clusters, and the central figures who bridge different groups.
        </p>

        <h4 class="bm-body__heading">How Bango builds it</h4>
        <ul>
          <li>
            <strong>Absolute edge weights:</strong> The link between two authors is the raw count of
            papers they co-authored. This preserves the real-world intensity of collaboration.
          </li>
          <li>
            <strong>Full & fractional modes:</strong> Full counting credits every co-author equally.
            <em>Fractional counting</em> divides each co-authorship by the number of authors on the
            paper, reducing the dominance of large consortium papers.
          </li>
          <li>
            <strong>Louvain community detection:</strong> Groups authors into color-coded research
            clusters by optimizing network modularity.
          </li>
          <li>
            <strong>ForceAtlas2 layout:</strong> A force-directed simulation pulls tightly-connected
            authors together and pushes separate groups apart, making collaborative cohorts visually
            obvious.
          </li>
        </ul>

        <h4 class="bm-body__heading">Key controls</h4>
        <ul>
          <li>
            <strong>Modularity resolution:</strong> Raise it to split large clusters; lower it to
            merge them.
          </li>
          <li>
            <strong>Min. articles filter:</strong> Hide authors with fewer than N publications to
            reduce clutter.
          </li>
          <li>
            <strong>Color mode:</strong> Color nodes by cluster, or temporally by average
            publication year.
          </li>
          <li>
            <strong>Layout:</strong> Fixed (preserves positions across filters) or Dynamic
            (re-simulate).
          </li>
          <li>
            <strong>Search & locate:</strong> Find a specific author and highlight their
            neighborhood.
          </li>
        </ul>

        <h4 class="bm-body__heading">Use cases</h4>
        <ul>
          <li>Identify the core research group around a prolific principal investigator.</li>
          <li>
            Find isolated sub-fields that rarely collaborate with the mainstream of your corpus.
          </li>
          <li>Spot potential collaborators or reviewers by tracing who bridges two communities.</li>
          <li>Support a "social network analysis" section in a bibliometric review paper.</li>
        </ul>
      </div>
    </section>

    <!-- =========================================================== -->
    <!-- MODULE 2: CITATION NETWORK -->
    <!-- =========================================================== -->
    <section class="bm-section">
      <header class="bm-section__header">
        <span class="material-symbols-outlined bm-section__icon" style="color: #8b5cf6"
          >account_tree</span
        >
        <div>
          <h3 class="bm-section__title">2. Citation Network</h3>
          <p class="bm-section__subtitle">
            A directed graph showing which articles cite which others, with main-path analysis.
          </p>
        </div>
      </header>

      <div class="bm-section__body">
        <h4 class="bm-body__heading">What it measures</h4>
        <p>
          Unlike co-authorship, a citation network captures the flow of ideas. Each article is a
          node, and a directed edge from A to B means "A cites B." This reveals foundational
          (heavily-cited) works, recent derivatives, and the chronological lineage of a research
          thread. Main-path analysis then extracts the backbone - the chain of papers that forms the
          intellectual spine of the field.
        </p>

        <h4 class="bm-body__heading">How Bango builds it</h4>
        <ul>
          <li>
            <strong>Directed edges:</strong> Drawn from the citing article to each article it
            references (backward links). Forward citations (cited-by) extend the picture when
            imported.
          </li>
          <li>
            <strong>Unmatched leaf nodes:</strong> Reference papers that are not themselves included
            articles appear as small dashed grey leaves, so you can see the full intellectual
            context without polluting the main graph.
          </li>
          <li>
            <strong>Main Path (SPC):</strong> Search Path Count highlights the sequence of citations
            carrying the highest traversal count, dimming everything off the backbone.
          </li>
          <li>
            <strong>Ancestry / Progeny isolation:</strong> Select a node to trace its full citation
            ancestry (what it builds on) or progeny (what builds on it).
          </li>
        </ul>

        <h4 class="bm-body__heading">Key controls</h4>
        <ul>
          <li><strong>Min. citations received:</strong> Hide weakly-connected nodes.</li>
          <li><strong>Show isolated papers:</strong> Toggle nodes with zero citation links.</li>
          <li>
            <strong>Show unmatched references:</strong> Include/exclude the grey dashed leaf nodes.
          </li>
          <li>
            <strong>Main Paths (SPC):</strong> Toggle the backbone highlight; off-path nodes dim.
          </li>
          <li>
            <strong>Time-slice:</strong> Restrict the graph to a year range via a dual-thumb slider.
          </li>
          <li><strong>Color mode:</strong> Cluster or temporal (publication year gradient).</li>
        </ul>

        <h4 class="bm-body__heading">Use cases</h4>
        <ul>
          <li>Identify the seminal foundational papers that everyone in the field cites.</li>
          <li>Trace how a method or concept propagated from its origin to current work.</li>
          <li>Find recent breakthrough articles sitting at the tips of citation chains.</li>
          <li>Justify the selection of "key papers" in a narrative literature review.</li>
        </ul>
      </div>
    </section>

    <!-- =========================================================== -->
    <!-- MODULE 3: KEYWORD CO-OCCURRENCE -->
    <!-- =========================================================== -->
    <section class="bm-section">
      <header class="bm-section__header">
        <span class="material-symbols-outlined bm-section__icon" style="color: #ec4899">cloud</span>
        <div>
          <h3 class="bm-section__title">3. Keyword Co-Occurrence</h3>
          <p class="bm-section__subtitle">
            Discover clusters of related research topics by mapping how often terms appear together.
          </p>
        </div>
      </header>

      <div class="bm-section__body">
        <h4 class="bm-body__heading">What it measures</h4>
        <p>
          A keyword co-occurrence network treats terms as nodes. Two terms are linked when they
          appear in the same article, and the more articles they share, the stronger the link. This
          reveals the conceptual structure of a field: which topics are studied together, where the
          field is fragmenting into sub-specialties, and which terms are central hubs bridging
          different research themes.
        </p>

        <h4 class="bm-body__heading">How Bango builds it</h4>
        <p>
          Bango is unusual in that it can draw keywords from
          <strong>five different sources</strong>, which you can combine or filter independently:
        </p>
        <ul>
          <li>
            <strong>Metadata</strong> - author-supplied keywords from the original database records.
          </li>
          <li>
            <strong>AI Noun Phrases</strong> - noun phrases extracted from abstracts by an LLM
            during screening, capturing concepts the authors never tagged explicitly.
          </li>
          <li><strong>Tags</strong> - the user-defined content tags assigned during screening.</li>
          <li>
            <strong>Labels</strong> - screening workflow labels (e.g. "disputed", "priority-read").
          </li>
          <li><strong>User Added</strong> - custom keywords you add manually.</li>
        </ul>
        <p>
          Co-occurrence strength equals the number of articles in which two terms appear together.
          Louvain clustering then groups terms into thematic communities, and ForceAtlas2 lays them
          out.
        </p>

        <h4 class="bm-body__heading">Key controls</h4>
        <ul>
          <li>
            <strong>Keyword sources:</strong> Toggle any combination of the five sources (at least
            one required).
          </li>
          <li>
            <strong>Min. document frequency:</strong> Drop terms appearing in fewer than N articles
            (removes noise).
          </li>
          <li><strong>Min. co-occurrence strength:</strong> Hide weak links below a threshold.</li>
          <li>
            <strong>Color mode:</strong> Cluster or temporal (average year of articles using the
            term).
          </li>
          <li>
            <strong>Cluster legend:</strong> Click a cluster to isolate its thematic neighborhood.
          </li>
        </ul>

        <h4 class="bm-body__heading">Use cases</h4>
        <ul>
          <li>
            Map the thematic landscape of a research area for an introduction or scoping review.
          </li>
          <li>Detect emerging terminology clusters that signal a new sub-field.</li>
          <li>
            Compare author keywords vs. AI-extracted concepts to find blind spots in indexing.
          </li>
          <li>Identify "bridge terms" that connect otherwise separate research communities.</li>
        </ul>
      </div>
    </section>

    <!-- =========================================================== -->
    <!-- MODULE 4: PUBLICATION TIMELINE -->
    <!-- =========================================================== -->
    <section class="bm-section">
      <header class="bm-section__header">
        <span class="material-symbols-outlined bm-section__icon" style="color: #f59e0b"
          >timeline</span
        >
        <div>
          <h3 class="bm-section__title">4. Publication Timeline</h3>
          <p class="bm-section__subtitle">
            Track publishing trends over time - your articles, their references, and their citations
            by year.
          </p>
        </div>
      </header>

      <div class="bm-section__body">
        <h4 class="bm-body__heading">What it measures</h4>
        <p>
          The timeline view plots three year-by-year distributions as stacked bar charts:
          <strong>publications</strong> (the years your included articles were published),
          <strong>references</strong> (the years of the papers they cite), and
          <strong>citations received</strong> (the years of papers that cite them, when forward
          citations are imported). A growth-rate sparkline shows year-over-year acceleration of your
          own corpus.
        </p>

        <h4 class="bm-body__heading">How Bango builds it</h4>
        <ul>
          <li>
            <strong>Publication years</strong> come from each included article's
            <code>publication_year</code>.
          </li>
          <li>
            <strong>Reference years</strong> come from the imported backward-reference papers
            (typically older, showing the intellectual foundation).
          </li>
          <li>
            <strong>Citation years</strong> come from forward-citation imports (typically newer,
            showing current impact).
          </li>
          <li>
            A secondary <strong>Top Journals</strong> chart auto-hides below 700px viewport height
            to keep the primary chart readable.
          </li>
        </ul>

        <h4 class="bm-body__heading">Key controls & export</h4>
        <ul>
          <li><strong>Year-range filter:</strong> Drill into a specific period.</li>
          <li>
            <strong>CSV export:</strong> Download the raw year/count data for your own analysis or
            plotting.
          </li>
          <li>
            <strong>SVG export:</strong> Save the chart as a vector image for manuscript figures.
          </li>
        </ul>

        <h4 class="bm-body__heading">Use cases</h4>
        <ul>
          <li>Visualize the growth (or decline) of a research field over decades.</li>
          <li>
            Show the gap between the age of your corpus and the age of its references - evidence of
            a field building on classic vs. contemporary work.
          </li>
          <li>Identify "breakout years" that correlate with landmark publications.</li>
          <li>Produce a publication-trend figure for a bibliometric results section.</li>
        </ul>
      </div>
    </section>

    <!-- =========================================================== -->
    <!-- MODULE 5: AUTHOR PRODUCTIVITY -->
    <!-- =========================================================== -->
    <section class="bm-section">
      <header class="bm-section__header">
        <span class="material-symbols-outlined bm-section__icon" style="color: #10b981"
          >bar_chart</span
        >
        <div>
          <h3 class="bm-section__title">5. Author Productivity Ranking</h3>
          <p class="bm-section__subtitle">
            Rank authors by output and citation impact, with h-index, i10, and g-index metrics.
          </p>
        </div>
      </header>

      <div class="bm-section__body">
        <h4 class="bm-body__heading">What it measures</h4>
        <p>
          This view moves beyond network structure to quantify individual author impact. Every
          author in your normalized corpus is ranked on a sortable table. Bango computes three
          complementary bibliometric indices, all scoped to the included articles you have screened
          in:
        </p>
        <ul>
          <li>
            <strong>h-index</strong> - an author has index h if h of their papers have at least h
            citations each. Balances productivity and impact.
          </li>
          <li>
            <strong>i10-index</strong> - the number of papers with at least 10 citations. A Google
            Scholar-style productivity measure.
          </li>
          <li>
            <strong>g-index</strong> - gives more weight to highly-cited papers than h-index; the
            top g papers together receive g² citations.
          </li>
          <li>
            Plus <strong>first-author</strong>, <strong>last-author</strong>, and
            <strong>solo</strong> publication counts to distinguish leaders from contributors.
          </li>
        </ul>

        <h4 class="bm-body__heading">How Bango builds it</h4>
        <p>
          All metrics are computed locally in Rust from the normalized author tables and are scoped
          to your included articles (not the global citation count, which may be incomplete).
          Citation counts draw on the <code>num_cited</code> field parsed from the N1 notes during
          import.
        </p>

        <h4 class="bm-body__heading">Key controls</h4>
        <ul>
          <li>
            <strong>Sortable columns:</strong> Click any header (papers, h-index, citations, year)
            to re-rank.
          </li>
          <li><strong>Detail slide-over:</strong> Click an author for a per-article breakdown.</li>
          <li>
            <strong>Google Scholar lookup:</strong> External-link icons launch a Scholar search for
            the author's global profile (useful for cross-checking against worldwide metrics).
          </li>
        </ul>

        <h4 class="bm-body__heading">Use cases</h4>
        <ul>
          <li>
            Identify the most influential authors in your corpus for targeted expert outreach.
          </li>
          <li>Compare h-index vs. first-author counts to find emerging vs. established leaders.</li>
          <li>Build an "author prominence" table for a bibliometric results section.</li>
          <li>Find potential collaborators with complementary productivity profiles.</li>
        </ul>
      </div>
    </section>

    <!-- =========================================================== -->
    <!-- MODULE 6: CO-CITATION ANALYSIS -->
    <!-- =========================================================== -->
    <section class="bm-section">
      <header class="bm-section__header">
        <span class="material-symbols-outlined bm-section__icon" style="color: #3b82f6">hub</span>
        <div>
          <h3 class="bm-section__title">6. Co-Citation Analysis</h3>
          <p class="bm-section__subtitle">
            Find works frequently cited together, with four normalization modes and a heatmap.
          </p>
        </div>
      </header>

      <div class="bm-section__body">
        <h4 class="bm-body__heading">What it measures</h4>
        <p>
          Two papers are <em>co-cited</em> when a third paper cites both of them. The more often
          they are co-cited, the more the academic community treats them as related. Co-citation
          analysis therefore reveals intellectual structure <strong>from the outside</strong> -
          based on how others group works together - rather than from the authors' own keywords or
          collaborations. It is especially powerful for mapping the foundational literature of a
          field.
        </p>

        <h4 class="bm-body__heading">How Bango builds it</h4>
        <p>
          Unlike the other networks which are precomputed during normalization, co-citation is
          calculated <strong>on demand</strong> because the result depends on the normalization mode
          and scope you choose. Bango scans the reference lists of your selected articles, counts
          shared reference papers, and builds a weighted similarity matrix between citing articles.
          Four normalization modes transform the raw co-citation counts:
        </p>
        <ul>
          <li>
            <strong>Raw</strong> - the absolute number of shared references. Preserves true
            intensity but favors prolific citing articles.
          </li>
          <li>
            <strong>Cosine</strong> - raw count divided by the geometric mean of each pair's total
            references. The default; balances intensity and size.
          </li>
          <li>
            <strong>Jaccard</strong> - shared references divided by the union of all references.
            Penalizes pairs with many non-overlapping references.
          </li>
          <li>
            <strong>Pearson</strong> - correlation coefficient across the reference vectors.
            Emphasizes structural similarity regardless of raw frequency.
          </li>
        </ul>

        <h4 class="bm-body__heading">Key controls</h4>
        <ul>
          <li>
            <strong>Scope:</strong> Compute over <em>included</em> articles only, or <em>all</em>
            articles in the project.
          </li>
          <li>
            <strong>Normalization:</strong> Toggle Raw / Cosine / Jaccard / Pearson (re-computes
            instantly).
          </li>
          <li><strong>Min. citation count:</strong> Drop articles cited fewer than N times.</li>
          <li><strong>Min. co-citation:</strong> Hide edges below a similarity threshold.</li>
          <li>
            <strong>Dual visualization:</strong> An interactive network graph <em>and</em> a
            sortable co-citation heatmap of the strongest pairs.
          </li>
          <li><strong>Color mode:</strong> Cluster or temporal (publication year).</li>
        </ul>

        <h4 class="bm-body__heading">Use cases</h4>
        <ul>
          <li>
            Identify the "invisible colleges" - groups of papers the field consistently reads
            together even without formal collaboration.
          </li>
          <li>
            Compare normalization modes: Cosine for a balanced view, Pearson for pure structural
            similarity, Raw to see dominant citing hubs.
          </li>
          <li>Discover foundational reference pairs that anchor an entire research stream.</li>
          <li>
            Produce a co-citation map figure for a bibliometric review, then cite the top pairs as
            evidence of thematic clustering.
          </li>
        </ul>
      </div>
    </section>

    <!-- =========================================================== -->
    <!-- BANGO vs. VOSVIEWER COMPARISON -->
    <!-- =========================================================== -->
    <section class="bm-section">
      <header class="bm-section__header">
        <span class="material-symbols-outlined bm-section__icon">compare_arrows</span>
        <div>
          <h3 class="bm-section__title">Modularity and Layout: Bango vs. VOSviewer</h3>
          <p class="bm-section__subtitle">
            Why Bango's networks may look different from VOSviewer's on the same dataset.
          </p>
        </div>
      </header>

      <div class="bm-section__body">
        <p>
          If you run the same RIS bibliography dataset through both Bango and VOSviewer, you will
          likely notice differences in the number of clusters and the relative spacing of authors on
          the screen. This is due to different core mathematical formulations:
        </p>

        <div class="bm-table-wrapper">
          <table class="bm-table">
            <thead>
              <tr>
                <th>Feature / Dimension</th>
                <th>Bango Approach</th>
                <th>VOSviewer Approach</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td><strong>Link Normalization</strong></td>
                <td>
                  Uses <strong>Absolute Weights</strong> (the exact number of co-authored papers).
                </td>
                <td>
                  Uses <strong>Association Strength Normalization</strong> (link weight is divided
                  by the product of both authors' total publications).
                </td>
              </tr>
              <tr>
                <td><strong>Modularity & Clustering</strong></td>
                <td>
                  Optimizes standard Louvain modularity. Connected groups are kept together into
                  cohesive communities (typically yielding 3–4 clusters on small datasets).
                </td>
                <td>
                  Optimizes normalized modularity, heavily penalizing weakly-associated links.
                  Splits nodes into many fine-grained clusters (typically 14+ clusters on the same
                  small dataset).
                </td>
              </tr>
              <tr>
                <td><strong>Visual Layout</strong></td>
                <td>
                  ForceAtlas2 force-directed layout (spring/magnet simulation). Highly connected
                  hubs are drawn strongly to the center.
                </td>
                <td>
                  VOS layout (multidimensional scaling / similarity distance minimization). Relies
                  strictly on normalized association strengths.
                </td>
              </tr>
              <tr>
                <td><strong>Advantages</strong></td>
                <td>
                  <ul class="bm-table-list">
                    <li>
                      Reveals <strong>real-world collaboration hubs</strong> and direct working
                      cohorts.
                    </li>
                    <li>Very easy to read and understand on small-to-medium datasets.</li>
                    <li>Perfect for departmental or local-group collaboration reviews.</li>
                  </ul>
                </td>
                <td>
                  <ul class="bm-table-list">
                    <li>Prevents highly prolific authors from dominating the entire map.</li>
                    <li>Allows small, specialized sub-fields to stand out clearly.</li>
                    <li>Very clean cluster boundaries on huge datasets (e.g., 5,000+ papers).</li>
                  </ul>
                </td>
              </tr>
              <tr>
                <td><strong>Disadvantages</strong></td>
                <td>
                  <ul class="bm-table-list">
                    <li>
                      In large-scale networks, nodes can collapse into a single dense central
                      cluster ("hairball"), requiring manual adjustment of the resolution slider to
                      split them.
                    </li>
                  </ul>
                </td>
                <td>
                  <ul class="bm-table-list">
                    <li>
                      Normalization can distort the visual sense of scale. A single paper by two
                      isolated authors can look just as strong as 10 papers by productive authors.
                    </li>
                  </ul>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p style="margin-top: 16px">
          <strong>Tip:</strong> Neither approach is "wrong." Bango is optimized for interpreting
          small-to-medium review corpora (the typical 50–2,000 article systematic review), while
          VOSviewer shines on macro-scale bibliographic datasets. For the same dataset, use Bango's
          modularity resolution slider to control cluster granularity.
        </p>
      </div>
    </section>
  </div>
</template>

<style scoped>
.ht-biblio {
  /* Container; uses shared .ht-* intro/footer/about classes */
}

/* Callout (getting started) */
.ht-biblio__callout {
  margin-bottom: var(--space-6);
}

.ht-biblio__callout-card {
  display: flex;
  gap: var(--space-4);
  align-items: flex-start;
  background-color: #eff6ff;
  border: 1px solid #bfdbfe;
  border-radius: var(--radius-md);
  padding: var(--space-5);
}

.ht-biblio__callout-icon {
  font-size: 22px;
  color: #2563eb;
  flex-shrink: 0;
  margin-top: 2px;
}

.ht-biblio__callout-title {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: var(--space-2);
}

.ht-biblio__callout-desc {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0;
}

/* Module sections */
.bm-section {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
  box-shadow: var(--shadow-sm);
  margin-bottom: var(--space-6);
}

.bm-section__header {
  display: flex;
  gap: var(--space-3);
  align-items: flex-start;
  border-bottom: 1px solid var(--color-border);
  padding-bottom: var(--space-3);
  margin-bottom: var(--space-4);
}

.bm-section__icon {
  font-size: 28px;
  flex-shrink: 0;
}

.bm-section__title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-1) 0;
}

.bm-section__subtitle {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  margin: 0;
}

.bm-section__body {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
}

.bm-section__body p {
  margin: 0 0 var(--space-3) 0;
}

.bm-section__body ul,
.bm-section__body ol {
  margin: 0 0 var(--space-3) 0;
  padding-left: var(--space-5);
}

.bm-section__body li {
  margin-bottom: var(--space-2);
}

.bm-section__body li:last-child {
  margin-bottom: 0;
}

.bm-section__body code {
  background-color: #eef2ff;
  padding: 1px 6px;
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
}

.bm-body__heading {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-top: var(--space-4);
  margin-bottom: var(--space-2);
}

.bm-body__heading:first-child {
  margin-top: 0;
}

/* Comparison table */
.bm-table-wrapper {
  width: 100%;
  overflow-x: auto;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  margin-top: var(--space-4);
}

.bm-table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
  font-size: var(--font-size-body);
}

.bm-table th,
.bm-table td {
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-border);
  vertical-align: top;
}

.bm-table th {
  background-color: #f1f5f9;
  color: var(--color-on-surface);
  font-weight: var(--font-weight-bold);
  font-size: var(--font-size-caption);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.bm-table tr:last-child td {
  border-bottom: none;
}

.bm-table td strong {
  color: var(--color-on-surface);
}

.bm-table ul {
  margin: 0;
  padding-left: var(--space-4);
}

.bm-table li {
  margin-bottom: var(--space-1);
}

@media (max-width: 767px) {
  .ht-biblio__callout-card {
    flex-direction: column;
  }
}
</style>
