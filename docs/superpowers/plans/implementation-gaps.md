# Implementation Gaps — v3 Design Reference vs Current Code

> Generated from audit of `docs/design-reference/` HTML files against current Vue views in `.worktrees/v3-implementation`.
> These are features visible in the Stitch reference designs but not yet implemented. Do NOT build them now — document for future planning.

---

## Screen 07: Dedup Review (`/dedup` → `dedup-review.vue`)

**Reference:** `docs/design-reference/07-dedup-review.html`

**Gap severity:** HIGH — current implementation is a basic pair listing; reference has a rich comparison UI.

| Missing Feature | Reference Description |
|----------------|----------------------|
| Side-by-side comparison grid | Two-column layout with Record A vs Record B cards showing full metadata (title, authors, year, journal, DOI, abstract) |
| Similarity score banner | Badges above the comparison: "Title Match", "Metadata Match", "Same DOI" with colored backgrounds |
| Yellow-highlighted text diffs | Matching/differing text highlighted with `bg-yellow-100 px-1 rounded` to visually indicate where records differ |
| Pair navigation controls | Chevron left/right with "Pair 14 of 82" counter |
| Letter badges (A/B) | Record A has `bg-indigo-100 text-indigo-700` badge, Record B has `bg-slate-100 text-slate-700` badge |
| Sticky bottom action bar | Fixed bar with: "Keep Both (Unique)", "Keep Record A", "Keep Record B" buttons + keyboard shortcut hints |
| Keyboard shortcuts | Styled `kbd` elements showing keyboard hints (←/→ for navigate, A/B/C for actions) |
| Source labels | Each record shows source label (e.g., "Scopus", "PubMed") |

---

## Screen 05: Screening Progress (`/screening` → `screening-progress.vue`)

**Reference:** `docs/design-reference/05-screening-progress.html`

**Gap severity:** MEDIUM — current has basic progress/stats; reference adds a live decision stream.

| Missing Feature | Reference Description |
|----------------|----------------------|
| Live decision stream | Scrolling feed showing real-time AI screening decisions as they happen |
| Stream item structure | Each item: timestamp (mono font), article title (truncated), status badge (Included/Rejected), confidence mini-bar |
| Live indicator | Animated ping dot (`animate-ping`) with "Live" label showing screening is active |
| Stream scroll mask | CSS `mask-image: linear-gradient(to bottom, transparent, black 10%, black 90%, transparent)` for fade effect at edges |
| Control buttons | Pause button (primary style) and Stop button (secondary with red icon) — current may have different button styles |
| Decorative background | Background has blurred circles (`bg-indigo-50/50 rounded-tl-full blur-3xl`) — low priority visual element |

---

## Screen 09: PRISMA Diagram (`/prisma` → `prisma-diagram.vue`)

**Reference:** `docs/design-reference/09-prisma-diagram.html`

**Gap severity:** LOW — core diagram exists but missing one toggle feature.

| Missing Feature | Reference Description |
|----------------|----------------------|
| Exclusion reasons toggle | Toggle switch labeled "Show exclusion reasons breakdown" that expands/collapses a detailed list of why articles were excluded |
| Exclusion reason list | When toggled on: `text-[11px] leading-[16px] text-on-surface-variant/80 list-disc pl-4 space-y-1` with individual reasons |
| Side boxes with dashed borders | Exclusion/duplicate count boxes use `border border-outline-variant border-dashed` styling |
| Connector arrows | Arrow icons (`arrow_right`, `arrow_drop_down`) using Material Symbols between flow boxes |
| Export buttons style | `flex items-center gap-2 px-4 py-2 border border-outline-variant rounded-lg` — may differ from current |

---

## Screen 01: Dashboard (`/` → `dashboard.vue`)

**Reference:** `docs/design-reference/01-dashboard.html`

**Gap severity:** LOW — structure exists but some visual elements are simplified.

| Missing Feature | Reference Description |
|----------------|----------------------|
| Bento grid stat cards | Reference uses a more elaborate card layout with `bg-white border border-slate-200 rounded-xl p-5 shadow-sm` with hover effects (`hover:border-indigo-200`) |
| Quick-action card hover effects | Reference has `group-hover:bg-indigo-600 group-hover:text-white` on icon circles |
| System status card (dark) | Reference has a dark card (`bg-slate-900 text-white`) for system status — may not be v3 scope |
| Activity timeline styling | Reference has more elaborate timeline items with colored dot indicators and relative timestamps |

---

## Screen 02: Import RIS (`/import` → `import-ris.vue`)

**Reference:** `docs/design-reference/02-ris-import.html`

**Gap severity:** LOW — stepper and drop zone exist but reference has richer preview.

| Missing Feature | Reference Description |
|----------------|----------------------|
| Sticky bottom summary card | Fixed bottom bar with: total articles count, duplicates detected count, parsing validation status, "Add to Project" + "Cancel" buttons with `bg-white/95 backdrop-blur-sm` |
| File count badge | "10 of 1,240 rows shown" badge on preview table |
| Upload zone hover effect | Custom CSS: `border-color: #352cd; background-color: rgba(79, 70, 229, 0.04)` on hover with `group-hover:scale-110` on icon |

---

## Cross-Screen: Sidebar Navigation

**Reference:** All 10 HTML files

**Gap severity:** LOW — structure exists, icons need updating.

| Missing Feature | Reference Description |
|----------------|----------------------|
| Material Symbols icons | All 9 nav items use `<span class="material-symbols-outlined">icon_name</span>` instead of Unicode |
| Logo background | Reference uses `bg-primary-container` for logo box, current may differ |
| Active icon fill | Some reference screens use `font-variation-settings: 'FILL' 1` for active icon (filled style) |
| App subtitle | Reference shows "LITERATURE REVIEW" in uppercase tracking-widest below "Bango" |

---

## Cross-Screen: Top App Bar

**Reference:** Most HTML files include a top header bar.

**Gap severity:** LOW — current app shell may not have this component.

| Missing Feature | Reference Description |
|----------------|----------------------|
| Top app bar | `fixed top-0 right-0 left-64 h-16 bg-white/80 backdrop-blur-md border-b border-slate-200` with search, notification bell, help, avatar |
| Global search input | `pl-10 pr-4 py-1.5 bg-slate-100 border-transparent focus:bg-white focus:border-indigo-600 rounded-lg text-sm w-64` |
| Notification/help buttons | Icon buttons in header right side |

Note: The top app bar may be out of v3 scope per `00-design-patterns.md` — some header elements are marked as "skip" (Projects/Archive/Team tabs, AI Assistant button). Verify with stakeholder.
