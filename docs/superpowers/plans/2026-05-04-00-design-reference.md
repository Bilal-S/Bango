# Plan 0: Stitch Design Reference Pull & Analysis

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pull all 10 Stitch screen designs from the Bango project, save HTML/CSS and screenshots as reference artifacts, analyze shared UI patterns for consistency, and create a design reference document that all subsequent plans use when building Vue components.

**Architecture:** Use the Stitch MCP server to pull screen code and images for each of the 10 screens. Analyze the HTML/CSS to extract shared patterns (sidebar, toolbar, cards, tables, badges, chips, forms, panels). Document which design elements map to the v3 spec and which are outdated/out-of-scope. Save everything to `docs/design-reference/`.

**Tech Stack:** Stitch MCP (`@_davideast/stitch-mcp`), file I/O

**Depends on:** Nothing (this is the first plan to execute)

---

## Stitch Screen Inventory

| Screen | Screen ID | Stitch Dimensions | Maps to Plan |
|--------|-----------|-------------------|--------------|
| Project Dashboard | `00800b2e6d3a44068ae18ca59a1b4eff` | 3072×2106 | Plan 1 (`dashboard.vue`) |
| RIS Import | `5d529b4d40b14252b5c9f934a9c2185b` | 2560×2668 | Plan 2 (`import-ris.vue`) |
| Article List View | `3070d2fe46e443828560e39f129a3cca` | 2560×2048 | Plan 7 (`article-list.vue`) |
| Article Detail Panel | `93f41b0e0b16462783a9caa295bdb330` | 2560×2048 | Plan 7 (`article-detail-panel.vue`) |
| AI Screening Progress | `f00aa494475e4f82b9fdacdd560456f6` | 2560×2048 | Plan 6 (`screening-progress.vue`) |
| Criteria Editor | `a04d3b19294c411d83ed6d685a46cf3f` | 3072×2048 | Plan 4 (`criteria-editor.vue`) |
| Deduplication Review | `66cf787d2ace46fdae0a066334bc1175` | 2560×2116 | Plan 3 (`dedup-review.vue`) |
| Tag & Label Management | `fa963f86a8f1468c9180b27a22274585` | 2560×2048 | Plan 5 (`tag-label-management.vue`) |
| PRISMA Flow Diagram | `a69258df28794325acc37b81ac43d902` | 2560×2048 | Plan 8 (`prisma-diagram.vue`) |
| LLM Configuration | `1df44ebddbf44369aaa9efba5dd57941` | 2560×2048 | Plan 4 (`llm-config.vue`) |

**Project ID:** `4799487491058521486`

---

## File Structure

```
docs/
├── design-reference/
│   ├── 00-design-patterns.md      (new: shared patterns extracted from all screens)
│   ├── 01-dashboard.html          (new: Stitch screen HTML)
│   ├── 01-dashboard.png           (new: Stitch screenshot)
│   ├── 02-ris-import.html
│   ├── 02-ris-import.png
│   ├── 03-article-list.html
│   ├── 03-article-list.png
│   ├── 04-article-detail.html
│   ├── 04-article-detail.png
│   ├── 05-screening-progress.html
│   ├── 05-screening-progress.png
│   ├── 06-criteria-editor.html
│   ├── 06-criteria-editor.png
│   ├── 07-dedup-review.html
│   ├── 07-dedup-review.png
│   ├── 08-tags-labels.html
│   ├── 08-tags-labels.png
│   ├── 09-prisma-diagram.html
│   ├── 09-prisma-diagram.png
│   ├── 10-llm-config.html
│   └── 10-llm-config.png
```

---

## Task 1: Create Output Directory

- [ ] **Step 1: Create the design reference directory**

```bash
mkdir -p docs/design-reference
```

- [ ] **Step 2: Commit**

```bash
git add docs/design-reference/.gitkeep
git commit -m "chore: create design reference directory"
```

---

## Task 2: Pull All Screen Screenshots

Pull screenshots first since they're quick and give a visual overview.

- [ ] **Step 1: Pull all 10 screenshots using Stitch MCP**

For each screen, run via the Stitch MCP CLI:

```bash
cd /home/user/code/bango

# Dashboard
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "00800b2e6d3a44068ae18ca59a1b4eff"}' > docs/design-reference/01-dashboard.png

# RIS Import
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "5d529b4d40b14252b5c9f934a9c2185b"}' > docs/design-reference/02-ris-import.png

# Article List
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "3070d2fe46e443828560e39f129a3cca"}' > docs/design-reference/03-article-list.png

# Article Detail
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "93f41b0e0b16462783a9caa295bdb330"}' > docs/design-reference/04-article-detail.png

# Screening Progress
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "f00aa494475e4f82b9fdacdd560456f6"}' > docs/design-reference/05-screening-progress.png

# Criteria Editor
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "a04d3b19294c411d83ed6d685a46cf3f"}' > docs/design-reference/06-criteria-editor.png

# Dedup Review
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "66cf787d2ace46fdae0a066334bc1175"}' > docs/design-reference/07-dedup-review.png

# Tags & Labels
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "fa963f86a8f1468c9180b27a22274585"}' > docs/design-reference/08-tags-labels.png

# PRISMA Diagram
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "a69258df28794325acc37b81ac43d902"}' > docs/design-reference/09-prisma-diagram.png

# LLM Config
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "1df44ebddbf44369aaa9efba5dd57941"}' > docs/design-reference/10-llm-config.png
```

**Note:** If the MCP CLI returns JSON with a base64-encoded image instead of a raw PNG, adjust the command to extract and decode the base64 data.

- [ ] **Step 2: Verify screenshots exist and are valid**

```bash
ls -la docs/design-reference/*.png
file docs/design-reference/*.png
```

Expected: 10 PNG files, each recognized as a valid image.

- [ ] **Step 3: Commit**

```bash
git add docs/design-reference/*.png
git commit -m "docs(design): pull Stitch screen screenshots for all 10 screens"
```

---

## Task 3: Pull All Screen HTML/CSS Code

Pull the actual HTML/CSS code from Stitch for each screen. This code contains the layout structure, component hierarchy, and styling that Vue components should match.

- [ ] **Step 1: Pull all 10 screen code files**

```bash
cd /home/user/code/bango

# Dashboard
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "00800b2e6d3a44068ae18ca59a1b4eff"}' > docs/design-reference/01-dashboard.html

# RIS Import
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "5d529b4d40b14252b5c9f934a9c2185b"}' > docs/design-reference/02-ris-import.html

# Article List
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "3070d2fe46e443828560e39f129a3cca"}' > docs/design-reference/03-article-list.html

# Article Detail
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "93f41b0e0b16462783a9caa295bdb330"}' > docs/design-reference/04-article-detail.html

# Screening Progress
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "f00aa494475e4f82b9fdacdd560456f6"}' > docs/design-reference/05-screening-progress.html

# Criteria Editor
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "a04d3b19294c411d83ed6d685a46cf3f"}' > docs/design-reference/06-criteria-editor.html

# Dedup Review
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "66cf787d2ace46fdae0a066334bc1175"}' > docs/design-reference/07-dedup-review.html

# Tags & Labels
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "fa963f86a8f1468c9180b27a22274585"}' > docs/design-reference/08-tags-labels.html

# PRISMA Diagram
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "a69258df28794325acc37b81ac43d902"}' > docs/design-reference/09-prisma-diagram.html

# LLM Config
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "1df44ebddbf44369aaa9efba5dd57941"}' > docs/design-reference/10-llm-config.html
```

- [ ] **Step 2: Verify HTML files exist and are non-empty**

```bash
ls -la docs/design-reference/*.html
wc -l docs/design-reference/*.html
```

Expected: 10 HTML files, each with substantial content (likely 200-2000 lines).

- [ ] **Step 3: Commit**

```bash
git add docs/design-reference/*.html
git commit -m "docs(design): pull Stitch screen HTML/CSS code for all 10 screens"
```

---

## Task 4: Analyze Shared Design Patterns

Read all 10 HTML files and both screenshots to extract shared patterns. This is the key step that ensures consistency across all views.

- [ ] **Step 1: Read and analyze all screen HTML files and screenshots**

For each HTML file, identify:
- **Navigation sidebar**: structure, items, icons, active states, width, colors
- **Top toolbar / header**: layout, actions, breadcrumbs
- **Content area**: padding, max-width, column layouts
- **Shared components**: buttons, inputs, badges, chips, cards, tables
- **Spacing and alignment**: padding, margins, gaps
- **Color usage**: which colors for which elements

Use the Read tool on each HTML file and the screenshots to build a complete picture.

- [ ] **Step 2: View all 10 screenshots to visually cross-check patterns**

Read each PNG file using the Read tool (it renders images). Note:
- Sidebar consistency across all screens
- Toolbar/header patterns
- Button placement and style
- Card/panel layouts
- Table/list row patterns
- Status badge colors and shapes
- Tag chip vs label chip visual distinction

- [ ] **Step 3: Document any inconsistencies between Stitch screens**

Compare the sidebar across all screens - are the nav items identical? Are the colors consistent? Is the spacing uniform? Note any differences and decide which version is canonical.

---

## Task 5: Write Design Patterns Reference Document

This is the deliverable that all subsequent plans reference.

- [ ] **Step 1: Create `docs/design-reference/00-design-patterns.md`**

Write a comprehensive reference document with these sections. Fill in exact values by reading the Stitch HTML/CSS output from Task 3.

The document structure:

```markdown
# Bango Design Patterns Reference

> Source: Google Stitch project "Bango AI Literature Reviewer" (4799487491058521486)
> Spec: v3 (bango-v3-spec.md)
> This document extracts shared UI patterns from Stitch screens for consistent implementation.
> **IMPLEMENT ONLY elements listed as "v3 scope". Ignore Stitch elements marked "outdated/skip".**

---

## 1. Navigation Sidebar

**Source screens:** All 10 screens
**Reference files:** `01-dashboard.html` through `10-llm-config.html`
**Plan:** Plan 1 (`nav-sidebar.vue`)

### Structure
[Extract from Stitch: sidebar width, background color, logo placement, nav item structure, icon + label layout, active/hover states, bottom section if any]

### Nav Items (v3 scope)
| Item | Icon | Route | Notes |
|------|------|-------|-------|
| Dashboard | [from Stitch] | / | |
| Articles | [from Stitch] | /articles | |
| Import RIS | [from Stitch] | /import | |
| Deduplicate | [from Stitch] | /dedup | |
| Criteria | [from Stitch] | /criteria | |
| Screening | [from Stitch] | /screening | |
| Tags & Labels | [from Stitch] | /tags | |
| PRISMA | [from Stitch] | /prisma | |
| Settings | [from Stitch] | /settings | |

### Nav Items (outdated/skip)
[List any nav items in Stitch that don't map to v3 spec - e.g., "Summary" might be a separate nav item or merged into Dashboard]

---

## 2. Page Header / Title Area

**Source screens:** All screens
**Pattern:** [Extract: page title typography, action button placement, subtitle/description pattern]

### v3 scope
- H1 title: [font-size, weight, letter-spacing from Stitch]
- Optional subtitle: [style from Stitch]
- Action buttons: [placement - top-right of header? Below title?]
- Breadcrumbs: [does Stitch use them?]

---

## 3. Data Table Pattern

**Source screens:** `03-article-list.html`, `02-ris-import.html`, `07-dedup-review.html`
**Plan:** Plan 7 (`article-table.vue`), Plan 2 (`import-preview.vue`)

### Structure
[Extract from Stitch: table container, header row styling, data row height, hover state, selected state, column padding, dividers, checkbox column width, sort indicators]

### Row Layout
[Extract: checkbox | title | authors | year | journal | status badge | confidence | tags | labels]

### v3 scope
- Row height: [from Stitch]
- Horizontal dividers only (no vertical)
- Hover color: [from Stitch]
- Checkbox in first column
- Status badge column: pill badge shape
- Confidence: bar or percentage
- Tag chips: solid background, 8px radius
- Label chips: outlined, 8px radius

---

## 4. Status Badge

**Source screens:** `03-article-list.html`, `04-article-detail.html`, `05-screening-progress.html`
**Plan:** Plan 7 (`status-badge.vue`)

### Styles
| Status | Background | Text Color | Border Radius |
|--------|-----------|------------|---------------|
| Imported | [from Stitch] | [from Stitch] | pill (9999px) |
| Working | [from Stitch] | [from Stitch] | pill |
| Included | [from Stitch] | [from Stitch] | pill |
| Rejected | [from Stitch] | [from Stitch] | pill |

---

## 5. Priority Indicator

**Source screens:** `06-criteria-editor.html`
**Plan:** Plan 4 (`criteria-editor.vue`)

### Styles
| Priority | Color | Visual Treatment |
|----------|-------|-----------------|
| Critical | #EF4444 | [from Stitch: circle? left border? pill?] |
| High | #F97316 | [from Stitch] |
| Standard | #4F46E5 | [from Stitch] |
| Low | #6B7280 | [from Stitch] |
| Optional | #9CA3AF | [from Stitch: dashed border?] |

---

## 6. Tag Chip vs Label Chip

**Source screens:** `08-tags-labels.html`, `03-article-list.html`
**Plan:** Plan 5 (`tag-chip.vue`, `label-chip.vue`)

### Tag Chip (solid)
- Background: [from Stitch - is it tinted with the tag color? Or neutral?]
- Border: none
- Border radius: 8px
- Text: [color from Stitch]
- Remove button: × on hover or always visible?

### Label Chip (outlined)
- Background: transparent
- Border: 1px solid [color from Stitch]
- Border radius: 8px
- Text: [color from Stitch]
- Remove button: same as tag?

---

## 7. Card / Panel Pattern

**Source screens:** `01-dashboard.html` (quick-action cards), `04-article-detail.html` (side panel)
**Plans:** Plan 1 (dashboard), Plan 7 (detail panel)

### Quick-Action Card (Dashboard)
[Extract: card padding, border, shadow, background, icon placement, title/subtitle layout]

### AI Decision Card (Article Detail)
[Extract: background color, sections, confidence bar style, reasoning text style]

### Side Panel (Article Detail)
[Extract: width, border-left, slide-in animation, header with close button, content sections]

---

## 8. Form / Input Pattern

**Source screens:** `06-criteria-editor.html`, `10-llm-config.html`
**Plans:** Plan 4 (`criteria-editor.vue`, `llm-config.vue`)

### Text Input
[Extract: border, padding, focus state, placeholder color, label style]

### Select / Dropdown
[Extract: styling - native or custom? Border, padding, arrow icon]

### Priority Selector (Criteria Editor)
[Extract: how does Stitch show the priority dropdown? Inline? Separate column?]

### Slider (LLM Config)
[Extract: range input styling for temperature]

---

## 9. Button Styles

**Source screens:** All screens
**Plans:** All plans with UI

### Primary Button
[Extract: background, text color, padding, border-radius, hover state, disabled state]

### Secondary Button
[Extract: background (ghost/outline?), text color, padding, border-radius, hover]

### Icon Button
[Extract: close button (×), add button (+), remove button - size, radius, color]

---

## 10. Progress Bar

**Source screens:** `05-screening-progress.html`
**Plan:** Plan 6 (`screening-progress-bar.vue`)

### Structure
[Extract: track height, fill color, track background, label position, percentage display]

### Stats Panel
[Extract: stat card layout - value size, label style, background, spacing between cards]

---

## 11. Stepper / Wizard

**Source screens:** `02-ris-import.html`
**Plan:** Plan 2 (`import-stepper.vue`)

### Step Indicator
[Extract: dot/circle style, active state, completed state (checkmark?), connector line style, step label position]

---

## 12. PRISMA Diagram Layout

**Source screens:** `09-prisma-diagram.html`
**Plan:** Plan 8 (SVG rendering)

### Box Layout
[Extract: box width, height, background, border, border-radius, font size, connector arrow style, side box style (exclusion counts)]

### Export Buttons
[Extract: SVG/PNG button placement - below diagram or in toolbar?]

---

## 13. Empty State / Placeholder

**Source screens:** Check if any Stitch screen shows an empty state
**Plans:** Multiple

[Extract or note: does Stitch show empty list states? What's the pattern?]

---

## 14. Screen-Specific Notes (v3 vs Stitch Deltas)

### Dashboard (`01-dashboard.html`)
**v3 scope:** Project name, article counts by status (pill badges), "Start Screening" CTA, activity feed, quick-action cards (Import RIS, Edit Criteria, View PRISMA)
**Stitch elements to skip:** [List any Stitch elements not in v3 spec - e.g., project selector if present, any mobile-specific elements]

### RIS Import (`02-ris-import.html`)
**v3 scope:** Drag-and-drop zone, parsed article preview table, import summary card, stepper (Upload → Parse → Dedup → Complete)
**Stitch elements to skip:** [List any differences from v3 spec workflow]

### Article List (`03-article-list.html`)
**v3 scope:** Left sidebar with status tabs + counts, filterable/sortable table, top toolbar
**Stitch elements to skip:** [e.g., PICO sidebar if present (v3 excludes this), star ratings (v3 excludes), mobile swipe gestures]

### Article Detail (`04-article-detail.html`)
**v3 scope:** Right-sliding side panel, full abstract, metadata, AI decision card, editable tags/labels, audit trail timeline
**Stitch elements to skip:** [e.g., keyword highlighting in abstract (v3 excludes)]

### Screening Progress (`05-screening-progress.html`)
**v3 scope:** Progress bar, processed/total, batch info, live decision feed, pause/resume/stop, stats panel
**Stitch elements to skip:** [any elements not matching v3 Section 9]

### Criteria Editor (`06-criteria-editor.html`)
**v3 scope:** Three-section editor (Aims, Inclusion with priority, Exclusion with priority), colored left borders
**Stitch elements to skip:** [e.g., any drag-to-reorder UI if present (v3 doesn't mention reordering)]

### Dedup Review (`07-dedup-review.html`)
**v3 scope:** Side-by-side two-panel view, similarity score, Keep A / Keep B / Keep Both buttons, pair list
**Stitch elements to skip:** [any elements not matching v3 Section 5]

### Tags & Labels (`08-tags-labels.html`)
**v3 scope:** Dual-panel, tags with solid chips, labels with outlined chips, add inputs, AI suggest buttons
**Stitch elements to skip:** [any elements not matching v3 Section 8]

### PRISMA Diagram (`09-prisma-diagram.html`)
**v3 scope:** Standard four-phase flow, record counts, exclusion arrows, SVG/PNG export buttons, exclusion reason breakdown toggle
**Stitch elements to skip:** [any elements not matching v3 Section 12]

### LLM Configuration (`10-llm-config.html`)
**v3 scope:** Provider dropdown, endpoint URL, model name, API key (masked), sliders, Test Connection button, VRAM warning banner for local providers
**Stitch elements to skip:** [any elements not matching v3 Section 10]
```

The actual content between `[Extract ...]` markers must be filled in by reading the Stitch HTML files pulled in Task 3. This is the critical step - the agent executing this plan reads each HTML file and fills in the exact values.

- [ ] **Step 2: Commit the design patterns document**

```bash
git add docs/design-reference/00-design-patterns.md
git commit -m "docs(design): add shared design patterns reference extracted from Stitch screens"
```

---

## Task 6: Update Existing Plans to Reference Stitch Designs

Add a reference step to each plan's UI tasks. This doesn't change the implementation code - it adds a "before you build" step.

For each of plans 1–9, the following note should be prepended to each Vue component task:

> **Design reference:** Before implementing, read `docs/design-reference/NN-<screen>.html` and `docs/design-reference/NN-<screen>.png`. Extract the exact layout structure, spacing, and component hierarchy from the Stitch HTML. Implement only v3-scoped elements per `docs/design-reference/00-design-patterns.md` Section 14.

Specific plan updates:

- [ ] **Step 1: Update Plan 1** - Dashboard task references `01-dashboard.html`
- [ ] **Step 2: Update Plan 1** - Sidebar task references sidebar pattern from `00-design-patterns.md` Section 1
- [ ] **Step 3: Update Plan 2** - Import UI task references `02-ris-import.html`
- [ ] **Step 4: Update Plan 3** - Dedup UI task references `07-dedup-review.html`
- [ ] **Step 5: Update Plan 4** - Criteria editor references `06-criteria-editor.html`
- [ ] **Step 6: Update Plan 4** - LLM config references `10-llm-config.html`
- [ ] **Step 7: Update Plan 5** - Tags/labels references `08-tags-labels.html`
- [ ] **Step 8: Update Plan 6** - Screening progress references `05-screening-progress.html`
- [ ] **Step 9: Update Plan 7** - Article list references `03-article-list.html`
- [ ] **Step 10: Update Plan 7** - Article detail references `04-article-detail.html`
- [ ] **Step 11: Update Plan 8** - PRISMA diagram references `09-prisma-diagram.html`
- [ ] **Step 12: Commit all plan updates**

```bash
git add docs/superpowers/plans/
git commit -m "docs(plans): add Stitch design references to all UI implementation tasks"
```

---

## Task 7: Final Verification

- [ ] **Step 1: Verify all artifacts exist**

```bash
ls -la docs/design-reference/
```

Expected: `00-design-patterns.md`, 10 `.html` files, 10 `.png` files = 21 files.

- [ ] **Step 2: Verify design patterns document is complete**

Read `docs/design-reference/00-design-patterns.md` and confirm:
- All 14 sections have actual values filled in (no `[Extract...]` placeholders remaining)
- Sidebar nav items match v3 spec routes
- Status badge colors are specified
- Tag vs label chip distinction is documented
- Section 14 has v3-scope vs skip lists for all 10 screens

- [ ] **Step 3: Verify plan references are correct**

Spot-check 3 plans to confirm they reference the correct Stitch screen file.

- [ ] **Step 4: Commit any final fixes**

```bash
git add -A
git commit -m "docs(design): finalize Plan 0 design reference artifacts"
```
