# What's New in Bango 3.0

## At a Glance

**New Tools**

- **OpenAlex search and import.** Search and import from 300 million scholarly works with metadata, PDF download, and automatic reference harvesting.
- **Citation Finder.** Paste prose from your manuscript and Bango finds which articles in your library support or contradict each claim.
- **Semantic embedding engine.** Automatic background vector generation for fast relevance search, powering the Citation Finder prefilter.
- **Citation Chaser.** Automated browser-based forward and backward citation scraping from any DOI.
- **Wiki site export.** Export your LLM-generated wiki as a standalone, searchable static website.
- **Research Gap Analysis.** One-click LLM report identifying underexplored areas in your literature.
- **Search Strategy Builder.** Generate database-specific Boolean query strings for PubMed, Scopus, Web of Science, and five other databases.

**Screening Pipeline**

- **Custom screening logic.** Write your own AND/OR/NOT rules referencing criteria by number. The engine respects your logic without override.
- **Immediate Stop.** Cancel an in-flight screening run within milliseconds. No more waiting up to two minutes.
- **Smart error recovery.** Transient failures leave articles unscreened for the next run. Auth failures stop immediately. A slow-LLM warning banner appears after the first timeout.
- **Clear AI decision.** Remove the LLM's reasoning while keeping your own Include/Exclude choice and screening history.
- **Engine diagnostics.** Always-on logging of every screening phase, LLM call, cancel event, and lock acquisition.

**Article Management**

- **View state preservation.** The article list remembers your tab, filters, sort order, page, search text, selected articles, open detail panel, and fullscreen state when you navigate away.
- **Inline metadata editing.** Double-click the title, journal, authors, year, DOI, or keywords to edit in place.
- **Permanent article delete.** Hard-delete cascade that cleans up all related data and on-disk files in one transaction.
- **Export selected articles.** Check any rows and export exactly those as an RIS file.
- **Keyboard navigation.** Arrow keys browse the article list and move between articles.
- **DOI filtering.** Search by DOI or find all articles missing one.
- **Bulk tag/label add, remove, and merge.** Apply or remove tags and labels across selected articles. Merge one into another.

**Tag & Label System**

- **NOT filters.** Exclude articles with specific tags or labels from your filtered view.
- **Inline rename.** Double-click any chip to rename it. Cascades through bibliometrics and wiki.
- **Merge command.** Consolidate duplicates with "Replace A with B".
- **Color preservation.** Tag and label colors now survive project export and import.

**References, Wiki, and Infrastructure**

- **Reference workflow.** Improved reference promotion, citation loading performance, and co-citation refinements.
- **Journal index matching.** Hardened ISSN normalization and journal name matching during import and metadata editing.
- **Wiki improvements.** Parallel batch ingest with consolidation, five-layer pre-seed matrix, and external-edit drift detection.
- **Batch import pipeline.** Five-phase background processor: full text, citations, translation, summaries, and embeddings.
- **Database VACUUM.** Reclaim disk space after major operations like project reset.
- **Start screen project load.** Open recent projects from the start screen.
- **Code health.** Screening engine and LLM layer refactored, project backup decomposed, 70% line coverage target met.

---

## New Tools & Major Features

### OpenAlex Search and Import

Bango now connects directly to OpenAlex, the largest open catalog of scholarly work with over 300 million publications. A new Search tab lets you query the catalog with keyword searches, then filter by year range, work type, language, and open access status. You can add any result to your project as a Working article in one click. Bango imports the full metadata including title, authors, affiliations, journal, DOI, and keywords. It can also download the open access PDF, attach it as full text, and optionally pull in the paper's references and cited-by relationships to populate your reference graph.

When your AI provider is configured, the Smart Search button reads your research aims and inclusion criteria, generates an optimized Boolean query, sets the filter defaults, and runs the search. Each result opens a detail panel showing the complete abstract, author list with affiliations, keywords, and a direct link to the publication on OpenAlex.

### Citation Finder

The Citation Finder answers a question every researcher has: which papers in my library support this statement? You paste any paragraph of prose (from a manuscript you are writing, for example) into the Chat view and the system finds which articles in your project support or contradict each claim.

It works through three layers. First, a semantic embedding search narrows the entire library to the top thirty most relevant candidates using cosine similarity. Then a text matching algorithm finds the best passage from each candidate. Finally, the LLM classifies each match as validating or opposing and writes a short explanation. You can process a whole paragraph as a single claim, or let the LLM split your text into individual statements and match each one independently. Results show the matched passage, a classification badge, and a confidence score. You can copy formatted citations in APA, MLA, Chicago, IEEE, or AMA style with a single click.

### Semantic Embedding Engine

Behind the scenes, Bango now generates semantic understanding vectors for every article in your project. Each article gets a vector for its title and abstract, plus one for every chunk of its full text. These vectors are what makes the Citation Finder fast. Instead of scanning the full text of every article, the prefilter step uses these embeddings to narrow to the most relevant candidates instantly.

The embedding engine works automatically once your AI provider is confirmed to support embeddings. Vectors are generated as a background task whenever you attach full text, regenerate AI summaries, or run the batch import pipeline. It supports OpenAI, Mistral, Google, Ollama, LM Studio, llama.cpp, and any custom OpenAI compatible endpoint. Providers that do not support embeddings (like Anthropic) are gracefully skipped with a clear message.

### Citation Chaser

You can now pull forward and backward citations directly from the browser, without leaving Bango. Given a DOI, the Citation Chaser drives the CitationChaser web app through an automated browser session, downloads RIS files of the article's references and cited-by papers, and imports them into your reference graph. It detects genuinely empty results (zero references or zero citations) and returns promptly instead of timing out. You can cancel a scrape at any time, and partial files are never left behind to corrupt future runs.

### Wiki Site Export

The entire LLM generated wiki knowledge base can now be exported as a standalone, searchable static website. Every page renders through the same Markdown engine used in the in-app viewer. Article source pages, author hubs, concept pages, method pages, and synthesis pages all get proper HTML pages with working wikilinks and a client side search index. The result is zipped to a file you can share with colleagues or host anywhere.

### Research Gap Analysis

A new one click report reads your included articles corpus, research aims, and inclusion and exclusion criteria, then uses your AI provider to identify underexplored areas and promising research directions. The report surfaces gaps in the literature and suggests future work. It is rendered as a formatted Markdown document in the Literature Review view with a citation style selector so references appear in whichever format you need.

### Search Strategy Builder

If you need formal Boolean search strings for database queries, the Search Strategy Builder generates them from your research aims and criteria. It includes syntax cheatsheets for eight databases: PubMed, Scopus, Web of Science, Cochrane, EBSCOhost, JSTOR, ScienceDirect, and arXiv. Each database gets its own tailored query string that you can copy and paste directly into that database's search interface.

---

## Screening Pipeline Redesign

We overhauled the screening engine for speed, reliability, and transparency.

### Custom Screening Logic

You can now write your own combinatorial screening rules using AND, OR, and NOT logic. The rules reference your existing criteria by their global number (so "criterion 3" means the same thing everywhere) and are injected into every screening prompt under a "Custom Screening Instructions" section. When custom rules are active, the LLM's decision is final. The generic priority resolver does not override it. You write your own logic, and the engine respects it.

### Immediate Stop

Clicking Stop during screening now cancels the in-flight LLM call within milliseconds. The previous behavior could leave a call running for up to two minutes after you clicked Stop because the cancel signal was being lost. We rewrote the cancellation mechanism using Tokio's notify primitives and fixed a subtle scheduling bug where the cancel branch was being skipped. Stopping also cancels the inter-batch delay throttle immediately, so the run really stops right when you tell it to.

### Smart Error Recovery

The engine now distinguishes between transient errors (rate limits, server errors, timeouts, transport failures) and permanent errors (malformed responses, parse failures). Transient errors leave affected articles unscreened so the next run picks them up naturally. You no longer need to manually "Reset Errors" workaround. Auth failures (wrong API key, invalid credentials) stop the run immediately so you can fix the problem instead of waiting through repeated failures.

A new slow-LLM warning banner appears after the first timeout, suggesting you reduce batch size or increase request delay. After three timeouts across the run (even if successful batches happen in between), the run stops with a clear message. Progress reporting is now accurate: deferred articles do not inflate the completion percentage, and a separate "N article(s) deferred" notice keeps you informed.

### Clear AI Decision

A trashcan icon on the AI Decision card lets you clear the LLM's reasoning and confidence score while preserving the screening timestamp and your own Include or Exclude choice. The card auto-collapses (your collapse preference is remembered), and the action writes an audit entry visible in the Timeline.

### Engine Diagnostics

Always-on diagnostic logging traces every phase transition, LLM call start and end, timeout detection, and cancel detection during screening. Run Bango with `Bango 2>screening.log` and grep for `screening:diag` to see the full trace. A five second heartbeat during active runs confirms the engine has not silently stalled. The database lock acquisition is timed and any lock held for more than one hundred milliseconds is reported. This instrumentation is always on. No debug build needed.

---

## Article Management Upgrades

### View State Preservation

The Article List view now remembers exactly where you were when you navigate away. Your active status tab, applied filters, sort order, current page, search text, selected articles, open detail panel, and fullscreen state all survive navigation to the Wiki, Chat, or Bibliometrics views. When you return, the data refreshes silently so changes from other views are reflected. Deep links from the dashboard or tag and label panels override the preserved state when they differ from what you were last looking at.

### Inline Metadata Editing

Double-click the article title in the detail header or any field in the Metadata card to edit it in place. Title changes show the old to new transition in the audit trail. The Journal field uses a combobox with autocomplete against the local journal index, and selecting a match links the article to that journal's metadata including ISSN. Year enforces a sensible range. Keywords and Authors accept arrays. Every edit refreshes the bibliometrics and wiki staleness so downstream tools stay accurate.

### Permanent Article Delete

A red trashcan icon in the detail header permanently deletes an article and all its related data in a single transaction. Tags, labels, chunks, audit entries, reference links, original content archives, bibliometric author associations, and biblio term links are all cleaned up. The on-disk full text PDF is removed. Shared reference papers that are still linked to other articles are preserved, and orphaned unmatched papers are swept. A confirmation dialog prevents accidental deletions.

### Export Selected Articles

Check any number of article rows in the list, then click Export in the bulk action bar to download exactly those articles as an RIS file. This is separate from the status-scoped toolbar Export button. You pick the save location through the OS file dialog.

### Keyboard Navigation

With the detail panel open, the left and right arrow keys move to the previous or next article. With the detail panel closed and the table focused, the up and down arrow keys navigate row selection. Shortcuts are disabled while you are typing in a text field.

### DOI Filtering

The article filter panel now includes a DOI text input for partial matching. An "Only no DOI" checkbox finds all articles missing a DOI, which is especially useful for data cleanup after importing records that arrived without one.

### Bulk Actions for Tags and Labels

The bulk action bar at the bottom of the article list supports adding and removing tags and labels across all selected articles. The result toast shows the actual number of articles affected. A new merge command ("Replace with...") moves all articles from one tag or label to another, handles overlaps correctly, and writes a single coalesced audit entry per article.

---

## Tag & Label System

The tag and label panel gained several quality-of-life improvements. You can now filter by NOT having a tag or label in the article filter panel by clicking the body of a selected pill. Excluded pills show a bold NOT prefix with a strikethrough on the name.

Double-click any tag or label chip to rename it inline. The rename cascades across all articles that carry it and updates the bibliometrics keyword network and wiki concept hubs.

The merge command lets you consolidate duplicate or similar tags and labels. Pick the one you want to keep, and the system reassigns all articles from the other one to the survivor, handles co-occurrence overlaps correctly, and writes clean audit entries showing "Replaced A -> B (merge)".

Both tags and labels now preserve their color through project export and import. A previous bug was silently clearing custom colors on restore.

---

## Reference & Citation Improvements

The reference promotion workflow now shows clearer feedback when you promote a reference paper to a full article. The references list tab was reworked for better browsing. Citation loading performance improved, and co-citation analysis got several refinements including better normalization modes.

The journal index matching used during import, project restore, and metadata editing was hardened. ISSN normalization now handles hyphen insertion, EBSCO suffixes, and cross-checking against both the ISSN and eISSN columns. Journal name matching is symbol-insensitive and intentionally avoids LIKE substring matching during automatic import to prevent false matches between journals with similar names. The interactive journal autocomplete in the Metadata card still uses substring search since you review the candidates there.

---

## Wiki Generator

The LLM wiki engine now runs multiple parallel batches when your corpus is large enough to need them, with a deterministic consolidation step that merges near-duplicate pages and rewrites wikilinks to the canonical slugs. A five-layer pre-seed matrix builds author pages, synthesis pages from AI summaries, concept hubs from user tags and bibliometric terms, method pages from study design extraction, and source pages for uploaded documents. These exist as a connected graph backbone before the LLM runs, so the wiki is never missing author, concept, method, or source pages regardless of which model you use.

External programs that edit wiki markdown files are now detected and re-indexed transparently, without re-running the LLM ingest. A tiered check makes this cheap: a directory fingerprint short-circuits when nothing changed, and per-file content hashes distinguish real edits from touch operations.

A new grounding gate counts pages that lack source article provenance after ingest, so ungrounded AI generated content can be flagged.

---

## Infrastructure & Polish

The batch import pipeline processes files in your Bango Documents directory through five phases: full text attachment, citation RIS import, translation, AI summary generation, and embedding vector creation. Each phase runs in parallel where possible, releases the database lock between articles so the rest of the app stays responsive, and can be cancelled at any time.

A database VACUUM command reclaims disk space after major operations like project reset. The app startup is faster thanks to improved journal index loading that uses separate read-only and read-write connections instead of the ATTACH DATABASE pattern that was failing on Windows.

The start screen now includes a project load button. A Share Bango menu item helps you tell colleagues about the tool. The landing page at bango.boncode.net was refreshed, and the in-app Help documentation was updated with content covering all six bibliometric tools, the wiki system, and the citation finder.

Test coverage reached the 70 percent line coverage target for both Rust and TypeScript, and many large inline test blocks were extracted to dedicated test files to keep the source code compact.

The old screening engine monolith was split into submodules for types, prompt construction, and stage two processing, with pure functions extracted for decision resolution, error classification, and JSON parsing. The LLM call layer was refactored to improve JSON resilience through control character escaping (LLMs sometimes put literal newlines inside JSON string values, which serde cannot parse). The project backup import was decomposed into twenty per-table import functions with a shared ID remap map.

---

## Everything Else

Beyond the headlines, this release includes dozens of smaller improvements. The AI Decision card is collapsible. Bulk action buttons wrap responsively on narrow viewports. The filter panel shows a running article count as you build your filter. The dashboard activity feed entries are clickable and link directly to the article. Criteria support inline double-click editing. Notes have their own audit entry type instead of being lumped in with status changes. The screening stepper got a width fix. The PDF viewer expanded view was corrected. Tag generation during screening now includes a standard taxonomy of twenty methodology and study type tags. Recent activity supports a "more" button to load additional history. The Tag and Label management view has a Filter button that deep-links to the filtered article list. The Clear Filters toolbar button turns red when filters are active but the panel is collapsed, making it obvious that filters are silently narrowing your view.

---

All of this ships as a single update. Your existing projects upgrade in place through the migration system. Your data stays local on your machine. No cloud, no account, no subscription required.
