# PRISMA Diagram Improvement Plan

## Overview

This plan addresses gaps between the current PRISMA implementation and the design reference / spec requirements. The core change is shifting the interactive diagram from a Rust-generated SVG injected via `v-html` to a proper Vue template using Tailwind design tokens, while retaining and improving the Rust SVG renderer for export.

---

## Current State

### Backend (Rust)
- **`src-tauri/src/prisma/data.rs`** — Computes `PrismaData` struct with counts from SQLite. Queries are correct per spec.
- **`src-tauri/src/prisma/svg.rs`** — Generates a basic SVG with hardcoded colors, no phase labels, no connector lines, no exclusion reason rendering.

### Frontend (Vue)
- **`src/composables/use-prisma.ts`** — Fetches SVG blob + structured data, provides export helpers.
- **`src/views/prisma-diagram.vue`** — Uses `var(--color-*)` custom CSS properties instead of Tailwind. Injects Rust SVG via `v-html`. No toggle for exclusion reasons. No phase labels.

### Design Reference
- **`docs/design-reference/09-prisma-diagram.html`** — Full Tailwind-based layout with phase labels, side boxes with dashed borders, Material Symbols arrows, toggle switch for exclusion reasons.
- **`docs/design-reference/00-design-patterns.md`** (Section 12) — Defines PRISMA layout using Tailwind `@theme` tokens.

---

## Issues Identified

| # | Issue | Severity |
|---|-------|----------|
| 1 | Vue view uses custom CSS vars, not Tailwind `@theme` tokens | High |
| 2 | Diagram is an opaque SVG string — no interactivity, no responsive behavior | High |
| 3 | No exclusion reason toggle (spec §12.1 requires it) | High |
| 4 | Rust SVG has no phase labels (Identification / Screening / Eligibility / Included) | Medium |
| 5 | Rust SVG has no horizontal connector lines to side boxes | Medium |
| 6 | Exclusion reasons not rendered in diagram or export | Medium |
| 7 | Side boxes missing dashed borders per design pattern | Medium |
| 8 | "Included" box has no special highlight styling | Low |
| 9 | Rust SVG uses hardcoded hex colors instead of design-system values | Low |

---

## Architecture: Dual-Path Rendering

```
                    PrismaData (from Rust)
                   /                      \
          Vue Template               Rust SVG Renderer
          (interactive)              (export-only)
                |                          |
        Tailwind @theme tokens      Design-system hex colors
                |                          |
        Display in app              SVG download / PNG export
```

### Why Dual-Path?
- **Vue template**: Responsive, interactive (toggle, hover), uses live Tailwind tokens
- **Rust SVG**: Self-contained, no runtime dependency, consistent export output
- Both consume the same `PrismaData` — no data duplication

---

## Planned Changes

### A. Rewrite `src/views/prisma-diagram.vue` (Major)

**Goal**: Replace the SVG `v-html` approach with a Vue template matching the design reference.

**Changes**:
1. Use Tailwind `@theme` tokens from `base.css`:
   - `bg-surface`, `bg-surface-container`, `bg-surface-container-high`
   - `text-on-surface`, `text-on-surface-variant`
   - `border-outline-variant`
   - `bg-primary-fixed`, `border-primary-fixed-dim` (included box)
   - `bg-error-container`, `text-on-error-container` (exclusion side boxes)
2. Implement the four-phase vertical flow:
   - **Identification** → `Records identified (n=...)`
   - **Screening** → `Records screened (n=...)`
   - **Eligibility** → `Full-text articles assessed (n=...)`
   - **Included** → `Studies included in review (n=...)`
3. Add side boxes with `border-dashed` for:
   - Duplicates removed (next to Identification)
   - Records excluded (next to Screening)
   - Exclusion reasons list (next to Eligibility) — shown when toggle is on
4. Use CSS for vertical connector lines (`w-px bg-outline-variant`)
5. Use Material Symbols `arrow_downward` for flow arrows, `arrow_forward` for side connectors
6. Add toggle switch in page header for "Show exclusion reasons"
7. Keep export buttons (SVG / PNG) wired to the composable
8. Add responsive behavior — horizontal scroll wrapper for narrow viewports

### B. Enhance `src/composables/use-prisma.ts` (Minor)

**Goal**: Add state management for the toggle and improve error handling.

**Changes**:
1. Add `showExclusionReasons` ref (boolean, default false)
2. Add `error` ref for error state display
3. Add computed `formattedNumbers` for display strings like `(n=1,240)`
4. Keep `svgData` for export — fetched alongside structured data on mount
5. Keep `exportSvg()` and `exportPng()` working from Rust-generated SVG

### C. Improve `src-tauri/src/prisma/svg.rs` (Moderate)

**Goal**: Generate a higher-quality SVG for export that matches the design.

**Changes**:
1. Add phase title text above each box (e.g., "IDENTIFICATION", "SCREENING")
2. Add horizontal connector lines from main boxes to side boxes
3. Add dashed stroke on side box borders (`stroke-dasharray="6 3"`)
4. Use design-system-aligned colors that map to the Tailwind tokens:
   - Main boxes: `#f0ecf9` fill, `#4f46e5` stroke (maps to `primary-fixed` / `primary-fixed-dim`)
   - Side boxes: `#fee2e2` fill, `#ef4444` stroke (maps to `error-container` / `error`)
   - Included box: `#d4fc79` fill, `#84cc16` stroke (maps to `primary-fixed` highlight)
5. Render exclusion reasons inside the exclusion side box (comma-separated or stacked)
6. Add proper `font-family` referencing Inter + system-ui fallback
7. Ensure the SVG has proper `xmlns` and is self-contained for download

### D. No Changes to `src-tauri/src/prisma/data.rs`

The SQL queries and `PrismaData` struct are correct per spec. No modifications needed.

### E. Verify Design Tokens (No file changes expected)

Confirm these tokens exist in `src/styles/base.css` under `@theme`:
- `--color-primary-fixed`, `--color-primary-fixed-dim`, `--color-on-primary-fixed` ✅
- `--color-error-container`, `--color-on-error-container` ✅
- `--color-surface`, `--color-surface-container`, `--color-surface-container-high` ✅
- `--color-outline-variant` ✅

---

## Export Flow (Unchanged Concept, Improved Output)

```
User clicks "Export SVG"
  → composable reads svgData (from Rust get_prisma_svg)
  → triggers download of .svg file

User clicks "Export PNG"
  → composable creates Image from svgData blob
  → draws to canvas
  → canvas.toBlob('image/png')
  → triggers download of .png file
```

The Rust `get_prisma_svg` command continues to exist. We just make it produce a better SVG.

---

## Files Affected

| File | Change Type | Priority |
|------|-------------|----------|
| `src/views/prisma-diagram.vue` | Major rewrite | P0 |
| `src/composables/use-prisma.ts` | Minor enhancement | P0 |
| `src-tauri/src/prisma/svg.rs` | Moderate improvement | P1 |
| `src-tauri/src/prisma/data.rs` | No change | — |
| `src/styles/base.css` | No change (verify only) | — |

---

## Risk Assessment

- **Low risk**: Changes are self-contained to the PRISMA view + composable + Rust SVG module
- **No data risk**: SQL queries in `data.rs` are correct and unchanged
- **Export compatibility**: SVG export continues to work; output quality improves
- **Design token dependency**: All required tokens already exist in `base.css`
- **No new dependencies**: No npm or cargo packages added

---

## Implementation Order

1. Rewrite `prisma-diagram.vue` with Tailwind template
2. Enhance `use-prisma.ts` with toggle state and error handling
3. Improve `svg.rs` for better export quality
4. Test interactive display and both export formats
5. Verify responsive behavior at different viewport widths