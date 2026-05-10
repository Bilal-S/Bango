# Bango Design Patterns Reference

> Source: Google Stitch project "Bango AI Literature Reviewer" (4799487491058521486)
> Spec: v3 (bango-v3-spec.md)

> **IMPLEMENT ONLY elements listed as "v3 scope". Ignore Stitch elements marked "outdated/skip".**

---

## Design Tokens (Shared Tailwind Config)

All 10 screens share an identical Tailwind config with these tokens:

**Colors (Material Design 3 inspired):**

| Token | Hex | Usage |
|-------|-----|-------|
| `primary` | `#3525cd` | Primary actions, links, active elements |
| `primary-container` | `#4f46e5` | Primary button backgrounds, sidebar logo bg |
| `on-primary` | `#ffffff` | Text on primary backgrounds |
| `on-primary-fixed` | `#0f0069` | Text on primary-fixed backgrounds |
| `primary-fixed` | `#e2dfff` | Light primary surfaces |
| `primary-fixed-dim` | `#c3c0ff` | Dimmed primary (PRISMA Included box) |
| `surface` | `#fcf8ff` | Main workspace background |
| `background` | `#fcf8ff` | Body background |
| `on-surface` | `#1b1b24` | Primary text color |
| `on-surface-variant` | `#464555` | Secondary text color |
| `outline` | `#777587` | Borders, dividers, muted text |
| `outline-variant` | `#c7c4d8` | Subtle borders |
| `surface-container` | `#f0ecf9` | Elevated surface background |
| `surface-container-low` | `#f5f2ff` | Low-elevation surface |
| `surface-container-lowest` | `#ffffff` | Cards, panels |
| `surface-container-high` | `#eae6f4` | Stepper inactive steps |
| `surface-container-highest` | `#e4e1ee` | Highest elevation surface |
| `secondary-container` | `#d5e0f8` | Secondary surfaces |
| `error` | `#ba1a1a` | Error states, destructive actions |
| `error-container` | `#ffdad6` | Error background |
| `on-error-container` | `#93000a` | Text on error backgrounds |
| `surface-dim` | `#dcd8e5` | Dimmed surface |
| `surface-bright` | `#fcf8ff` | Bright surface |
| `surface-variant` | `#e4e1ee` | Variant surface |
| `surface-tint` | `#4d44e3` | Surface tint overlay |

**Font Sizes:**

| Token | Size | Line Height | Weight | Letter Spacing |
|-------|------|-------------|--------|----------------|
| `display` | 24px | 32px | 600 | -0.02em |
| `h1` | 20px | 28px | 600 | -0.01em |
| `h2` | 16px | 24px | 600 | 0 |
| `body-main` | 14px | 20px | 400 | 0 |
| `body-sm` | 13px | 18px | 400 | 0 |
| `label-caps` | 11px | 16px | 600 | 0.05em |
| `mono` | 13px | 18px | 400 | 0 |

**Spacing:**

| Token | Value |
|-------|-------|
| `unit` | 4px |
| `container-padding` | 24px |
| `gutter` | 16px |
| `stack-gap` | 12px |
| `sidebar-width` | 260px (Tailwind `w-64`) |

**Border Radius:**

| Token | Value |
|-------|-------|
| `DEFAULT` | 0.25rem (4px) |
| `lg` | 0.5rem (8px) |
| `xl` | 0.75rem (12px) |
| `full` | 9999px (pill) |

**Font Family:** Inter, system-ui (all variants)

**Icon Font:** Material Symbols Outlined with `font-variation-settings: 'FILL' 0, 'wght' 400, 'GRAD' 0, 'opsz' 24`

---

## 1. Navigation Sidebar

**Consistency:** Present on all 10 screens. Structure is nearly identical across all files.

**Layout:**
- Width: `w-64` (256px, config token `sidebar-width: 260px`)
- Background: `bg-slate-800` (dark slate, approx `#1e293b`)
- Border-right: `border-r border-slate-700`
- Position: `fixed left-0 top-0 h-screen`
- Z-index: `z-50`
- Padding: `p-4`
- Flex direction: column with `flex flex-col`
- Gap between nav items: `gap-2` (outer), `space-y-1` (inner nav)

**Logo Area:**
- Container: `flex items-center gap-3`, padding `px-2`, margin-bottom `mb-6` to `mb-8`
- Logo icon: `w-8 h-8` rounded box with icon centered
  - Background varies by screen: `bg-indigo-500`, `bg-primary-container`, `bg-indigo-600`
- App name: `text-xl font-bold tracking-tight text-white leading-tight` (or `leading-none`)
- Subtitle: `text-[10px] text-slate-400 uppercase tracking-widest` or `text-slate-500 uppercase tracking-wider`
  - Text: "Literature Review" or "LITERATURE REVIEW"

**Nav Items (Inactive):**
- Container: `flex items-center gap-3 px-3 py-2`
- Text: `text-slate-400 hover:text-slate-100 hover:bg-slate-700/50 rounded-md`
- Font: `font-sans text-sm font-medium`
- Transition: `transition-all duration-200 ease-in-out`
- Icon: `<span class="material-symbols-outlined">icon_name</span>`

**Nav Items (Active):**
- Background: `bg-slate-700` (or `bg-slate-800`)
- Text: `text-white`
- Border-radius: `rounded-md`
- Some screens add icon color: `text-indigo-400` for the icon
- Active icon sometimes uses `font-variation-settings: 'FILL' 1` (filled icon)

**Primary CTA Button (sidebar):**
- Position: at bottom section, `w-full`
- Background: `bg-indigo-600 hover:bg-indigo-500` (or `hover:bg-indigo-700`)
- Text: `text-white font-semibold text-sm`
- Padding: `py-2 px-4 rounded-md`
- Layout: `flex items-center justify-center gap-2`
- Icon: `add` Material Symbol at `text-sm`
- Label: "New Search" or "New Project"

**Bottom Section:**
- Separator: `border-t border-slate-700 pt-4`
- Items: Support, Account, Profile (same style as inactive nav items)
- Some screens show user avatar at bottom with name/role

---

## 2. Page Header / Title Area

**Top App Bar (shared across all screens):**
- Position: `fixed top-0 right-0 left-64 h-16 z-40`
- Background: `bg-white/80 backdrop-blur-md`
- Border: `border-b border-slate-200`
- Shadow: `shadow-none`
- Padding: `px-6`
- Layout: `flex items-center justify-between`

**Left side of header:**
- App name or breadcrumb: `text-lg font-semibold text-slate-900` or page title
- Breadcrumb format: `text-base font-semibold / text-slate-500` (screen 02)
- Navigation tabs: Projects / Archive / Team
  - Active tab: `text-indigo-600 font-bold border-b-2 border-indigo-600 pb-1 font-sans text-sm`
  - Inactive tab: `text-slate-500 hover:text-slate-900 transition-colors font-sans text-sm`

**Right side of header:**
- Search input: `pl-10 pr-4 py-1.5 bg-slate-100` (or `bg-slate-50`), `border-transparent focus:bg-white focus:border-indigo-600 rounded-lg text-sm w-64`
- Icon buttons: `p-2 text-slate-500 hover:bg-slate-50 rounded-md` (or `rounded-full`)
- Divider: `h-8 w-[1px] bg-slate-200 mx-2` (or `h-6`)
- Export button: `bg-indigo-600 hover:bg-indigo-700 text-white px-4 py-1.5 rounded-lg text-sm font-medium`
- Avatar: `w-8 h-8 rounded-full border border-slate-200`

**Page Title (within content area):**
- Section headers: `font-display text-display` (24px/600) for hero titles
- `font-h1 text-h1` (20px/600) for section titles within cards
- `font-h2 text-h2` (16px/600) for card titles
- Subtitle: `text-slate-500 font-body-main` (14px) or `text-on-surface-variant`
- Active project badge: `text-indigo-600 bg-indigo-50 px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider`

---

## 3. Data Table Pattern

**Container:**
- Wrapper: `bg-white rounded-xl border border-slate-200 shadow-sm overflow-hidden`
- Header row background: `bg-slate-50/50` or `bg-surface-container-low border-b border-slate-200` (or `border-outline-variant`)

**Header Row (`<thead>`):**
- Cell padding: `px-6 py-3` (or `py-4 px-4`)
- Font: `font-label-caps text-on-surface-variant` (11px/600/uppercase/0.05em tracking) or `font-display text-label-caps text-slate-500 uppercase`
- Content: uppercase column names

**Data Rows (`<tbody>`):**
- Row separation: `divide-y divide-slate-100` (or `divide-outline-variant`)
- Row padding: `py-5 px-2` to `py-4 px-6` depending on density
- Hover state: `hover:bg-slate-50/80 transition-colors` or `hover:bg-surface-container`
- Group marker: `group` class for triggering child hover states

**Checkbox:**
- Input: `rounded border-slate-300 text-indigo-600 focus:ring-indigo-500`
- Padding: `py-5 px-4`

**Column Values:**
- Title column: `text-body-main font-semibold text-slate-900 truncate max-w-xs`
- Text columns: `text-body-sm text-slate-600` (or `text-on-surface-variant`)
- Year column: `text-body-sm text-slate-600 font-mono`
- DOI column: `font-mono text-[12px] text-primary` (link-colored)

**Toolbar (above table):**
- Container: `flex items-center justify-between mb-6 bg-white p-3 rounded-xl border border-slate-200 shadow-sm`
- Filter button: `flex items-center gap-2 px-3 py-1.5 bg-slate-100 rounded-lg text-slate-700 text-sm font-medium`
- Sort button: `flex items-center gap-2 px-3 py-1.5 bg-white border border-slate-200 rounded-lg text-slate-600 text-sm`
- Selection indicator: `text-xs text-slate-400`
- Bulk action: `px-3 py-1.5 text-indigo-600 bg-indigo-50 rounded-lg text-sm font-medium`

**Pagination (table footer):**
- Container: `p-4 bg-slate-50 border-t border-slate-200 flex items-center justify-between`
- Info: `text-xs text-slate-500 font-medium`
- Buttons: `px-3 py-1 bg-white border border-slate-200 rounded text-xs font-medium text-slate-600 hover:bg-slate-50`

---

## 4. Status Badge

**Shared properties:**
- Shape: pill (`rounded-full`)
- Padding: `px-2.5 py-0.5`
- Font: `text-[11px] font-bold` (or `font-semibold`)
- Tracking: `tracking-tight` or no tracking
- Uppercase: varies (some use uppercase, some don't)

**Status colors (from screens 01, 03, 05):**

| Status | Background | Text Color | Additional |
|--------|-----------|------------|------------|
| **Imported** | `bg-blue-100` | `text-blue-800` | `uppercase tracking-tight` |
| **Working** / **Pending** | `bg-amber-100` | `text-amber-800` / `text-amber-700` | `uppercase tracking-tight` |
| **Included** | `bg-emerald-100` | `text-emerald-700` / `text-emerald-800` | `uppercase tracking-tight` |
| **Rejected** | `bg-rose-100` | `text-rose-700` / `text-rose-800` | `uppercase tracking-tight` |
| **Reviewing** | `bg-indigo-100` | `text-indigo-700` | From screen 03 table |

**Live stream badges (screen 05):**

| Status | Background | Text Color | Border |
|--------|-----------|------------|--------|
| Included (stream) | `bg-indigo-100` | `text-indigo-700` | `border border-indigo-200` |
| Rejected (stream) | `bg-slate-100` | `text-slate-600` | `border border-slate-200` |

**Dashboard stat badges use the same color scheme but with slightly different Tailwind tints:**
- Imported: `bg-blue-100 text-blue-800`
- Working: `bg-amber-100 text-amber-800`
- Included: `bg-emerald-100 text-emerald-800`
- Rejected: `bg-rose-100 text-rose-800`

---

## 5. Priority Indicator

**From screen 06 (Criteria Editor) -- colored left border system:**

Each criterion card uses a left border to indicate priority level, with a matching tinted background.

| Priority | Left Border | Background | Label Text Color | Label Style |
|----------|------------|------------|-----------------|-------------|
| **Critical** | `border-l-4 border-red-500` | `bg-red-50/30` | `text-red-700` | "Critical Criterion" |
| **High** | `border-l-4 border-orange-500` | `bg-orange-50/30` | `text-orange-700` | "High Priority" |
| **Standard** | `border-l-4 border-indigo-500` | `bg-indigo-50/30` | `text-indigo-700` | "Standard Criterion" |
| **Low** | `border-l-4 border-slate-400` | `bg-slate-50/50` | `text-slate-600` | "Low Priority" |
| **Optional** | `border-l-4 border-slate-300 border-dashed` | `bg-white` | `text-slate-400` | "Optional/Draft" |

**Card structure for each criterion:**
- Container: `flex items-start gap-4 p-4 rounded-r-lg group`
- Label: `text-[10px] uppercase font-bold tracking-wider` with priority text color
- Textarea: `w-full bg-transparent border-none p-0 focus:ring-0 text-body-main resize-none`
- Priority dropdown: `text-xs bg-white border border-slate-200 rounded px-2 py-1 focus:ring-primary outline-none`
- Options: Critical, High, Standard, Low, Optional

**v3 spec defines these exact priority colors:**
- Critical: `#EF4444` (bright red)
- High: `#F97316` (orange)
- Standard: `#3B82F6` (indigo/blue -- note: Stitch uses `indigo-500` which maps closely)
- Low: `#6B7280` (medium gray)
- Optional: `#9CA3AF` (subtle gray, dashed border)

---

## 6. Tag Chip vs Label Chip

**Tags (content-category, solid background):**

From screens 03, 04, 08:

- Container: `inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg`
- Background: solid colored background
- Text: `font-mono text-mono` (13px monospace)
- Border: `border border-{color}-200` (in management view) or no border (in table view)

**Tag colors found across screens:**

| Tag Example | Background | Text Color |
|-------------|-----------|------------|
| "Artificial Intelligence" | `bg-blue-100` | `text-blue-700` |
| "NLP" | `bg-teal-100` | `text-teal-700` |
| "Data Management" | `bg-purple-100` | `text-purple-700` |
| "Case Study" | `bg-amber-100` | `text-amber-700` |
| "Ethics" | `bg-rose-100` | `text-rose-700` |
| "Renewable Energy" (screen 04) | `bg-[#EEF2FF]` | `text-[#4338CA]` |
| "High Efficiency" (screen 04) | `bg-[#FFF7ED]` | `text-[#9A3412]` |
| machine-learning (screen 08) | `bg-blue-100` | `text-blue-800` |
| clinical-trial (screen 08) | `bg-green-100` | `text-green-800` |
| nlp-models (screen 08) | `bg-purple-100` | `text-purple-800` |

**Tag in table (screen 03):**
- Compact: `px-2 py-0.5 bg-{color}-100 text-{color}-700 rounded-lg text-[11px] font-medium`
- No border, no icon

**Tag in management view (screen 08):**
- Full: `px-2.5 py-1 rounded-lg bg-{color}-100 text-{color}-800 font-mono text-mono border border-{color}-200`
- With hover actions (edit/delete) that appear on group hover

**Labels (workflow markers, outlined):**

From screens 03, 04, 08:

- Container: `inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg`
- Background: `bg-transparent`
- Border: `border border-{color}-300` (1px solid)
- Text: `font-mono text-mono`

**Label colors found:**

| Label Example | Border Color | Text Color | Dot Color |
|---------------|-------------|------------|-----------|
| "Systematic" (screen 03) | `border-slate-200` | `text-slate-500` | none |
| "Meta-Analysis" (screen 04) | `border-slate-200` | `text-slate-600` | none |
| "Healthcare" (screen 03) | `border-slate-200` | `text-slate-500` | none |
| priority-read (screen 08) | `border-red-300` | `text-red-700` | `bg-red-500` dot |
| disputed (screen 08) | `border-orange-300` | `text-orange-700` | `bg-orange-500` dot |
| needs-review (screen 08) | `border-slate-300` | `text-slate-700` | `bg-slate-400` dot |

**Label in management view (screen 08) has a colored dot indicator:**
- `w-1.5 h-1.5 rounded-full bg-{color}-500` inside the chip

**"Add Tag" button (screen 04):**
- `border border-dashed border-slate-300 text-slate-400 px-3 py-1 rounded-lg text-xs font-medium flex items-center gap-1 hover:border-indigo-400 hover:text-indigo-400 transition-colors`

---

## 7. Card / Panel Pattern

**Quick-Action Cards (screen 01):**
- Container: `w-full bg-white border border-slate-200 p-4 rounded-xl shadow-sm hover:border-indigo-400 transition-all flex items-center gap-4 group text-left`
- Icon circle: `w-10 h-10 bg-indigo-50 rounded-lg flex items-center justify-center text-indigo-600 group-hover:bg-indigo-600 group-hover:text-white transition-colors`
- Title: `font-semibold text-slate-900 text-sm`
- Subtitle: `text-slate-400 text-xs`

**Stat Cards / Bento Grid (screens 01, 05):**
- Container: `bg-white border border-slate-200 rounded-xl p-5 shadow-sm hover:border-indigo-200 transition-colors` (dashboard) or `bg-white p-6 rounded-xl border border-slate-200 shadow-sm` (screening)
- Header row: `flex justify-between items-start mb-4`
- Stat icon: `material-symbols-outlined text-slate-400`
- Stat value: `text-3xl font-extrabold text-slate-900` or `text-3xl font-bold text-indigo-600`
- Stat label: `text-slate-400 text-xs mt-1`

**AI Decision Card (screen 04):**
- Container: `bg-[#ECFDF5] border border-[#10B981]/20 rounded-xl p-4`
- Icon: `material-symbols-outlined text-[#059669]` with `data-weight="fill"` (filled checkmark)
- Decision label: `font-bold text-[#064E3B]`
- Confidence badge: `text-[11px] font-bold text-[#047857] bg-white px-2 py-0.5 rounded-full shadow-sm`
- Reasoning text: `text-body-sm text-[#065F46] leading-relaxed`

**Article Detail Side Panel (screen 04):**
- Width: `w-[480px]`
- Shadow: `shadow-[0_4px_24px_rgba(0,0,0,0.15)]`
- Border: `border-l border-slate-200`
- Header: `p-6 border-b border-slate-100 sticky top-0 bg-white z-10`
- Scrollable content: `flex-1 overflow-y-auto p-6 space-y-8`
- Footer: `p-4 border-t border-slate-100 flex gap-3 bg-slate-50/50`

**Section Panel (screen 06, 10):**
- Container: `bg-surface-container-lowest rounded-xl p-6 border border-slate-200 shadow-[0_4px_12px_rgba(0,0,0,0.05)]`
- Section header: `flex items-center justify-between mb-6`
  - Icon + title: `flex items-center gap-3` with icon + `font-h1 text-h1 text-on-surface`

**System Status Card (screen 01, dark variant):**
- Container: `bg-slate-900 text-white rounded-xl p-5 shadow-lg overflow-hidden relative`
- Title: `text-sm font-semibold mb-1`
- Description: `text-xs text-slate-400 mb-4`

---

## 8. Form / Input Pattern

**Text Input:**
- Default: `w-full bg-surface-container-lowest border border-outline-variant rounded-lg px-4 py-2.5 font-body-main text-body-main text-on-surface`
- Focus: `focus:border-primary focus:ring-1 focus:ring-primary outline-none transition-colors`
- Placeholder: `placeholder:text-outline` or `placeholder:text-slate-400`
- Search variant: `pl-10 pr-4 py-1.5 bg-slate-100 border-transparent focus:bg-white focus:border-indigo-600 rounded-lg text-sm w-64`

**Inline Text Input (Criteria Editor):**
- Style: `flex-1 bg-transparent border-b border-slate-100 py-2 focus:border-primary-container focus:ring-0 outline-none transition-colors text-body-main`
- Empty/add-new: `border-b border-dashed border-slate-200 italic text-slate-400`

**Password Input (screen 10):**
- Same as text input but with visibility toggle button: `absolute right-3 top-1/2 -translate-y-1/2 text-outline hover:text-on-surface transition-colors`

**Select / Dropdown:**
- Style: `w-full appearance-none bg-surface-container-lowest border border-outline-variant rounded-lg px-4 py-2.5 font-body-main text-body-main text-on-surface focus:border-primary focus:ring-1 focus:ring-primary outline-none transition-colors`
- Dropdown arrow: positioned `<span class="material-symbols-outlined absolute right-3 top-1/2 -translate-y-1/2 text-outline pointer-events-none">expand_more</span>`
- Small variant (criteria priority): `text-xs bg-white border border-slate-200 rounded px-2 py-1 focus:ring-primary outline-none`

**Textarea (criteria):**
- Style: `w-full bg-transparent border-none p-0 focus:ring-0 text-body-main resize-none`
- Rows: 2

**Range Slider (screen 10):**
- Input: `w-full h-1.5 bg-surface-variant rounded-lg appearance-none cursor-pointer accent-primary`
- Value label: `font-mono text-mono text-primary font-medium bg-primary-fixed text-on-primary-fixed px-2 py-0.5 rounded`
- Range labels: `flex justify-between text-xs text-outline mt-1 font-mono`

**Toggle Switch (screen 09):**
- Track: `block bg-primary w-10 h-6 rounded-full`
- Thumb: `dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition transform translate-x-4` (checked state)
- Hidden input: `sr-only`

**Number Input (screen 10):**
- Style: `w-20 bg-surface-container-lowest border border-outline-variant rounded-lg px-3 py-2 font-mono text-mono text-center text-on-surface focus:border-primary focus:ring-1 focus:ring-primary outline-none transition-colors`

---

## 9. Button Styles

**Primary Button:**
- Background: `bg-indigo-600` (or `bg-primary` = `#3525cd`, or `bg-primary-container` = `#4f46e5`)
- Text: `text-white`
- Padding: `px-8 py-2.5` (large), `px-4 py-1.5` (medium), `px-4 py-2.5` (sidebar)
- Border-radius: `rounded-lg` (8px) or `rounded-md` (4px) or `rounded-xl` (12px)
- Font: `font-semibold text-sm` or `font-h2 text-h2` (16px/600)
- Shadow: `shadow-sm`, `shadow-md`, or `shadow-lg`
- Hover: `hover:bg-indigo-700` (or `hover:bg-indigo-500`)
- Active: `active:scale-95`
- Transition: `transition-all`

**Secondary / Ghost Button:**
- Background: `bg-white` or transparent
- Text: `text-slate-700` or `text-on-surface-variant`
- Border: `border border-slate-200` or `border border-outline-variant`
- Padding: `px-6 py-2.5`
- Border-radius: `rounded-lg`
- Font: `font-semibold text-sm` or `font-h2 text-h2`
- Hover: `hover:bg-slate-50` or `hover:bg-gray-100`

**Revert / Cancel Button:**
- Same as secondary but with no border emphasis
- Style: `px-6 py-2.5 bg-white border border-slate-200 text-slate-700 rounded-lg font-semibold text-sm hover:bg-slate-50 transition-all shadow-sm`

**Destructive Button (screen 06 hover):**
- Icon color: `hover:text-error`
- Background on hover: `hover:bg-error-container`

**Icon Button:**
- Style: `p-2 text-slate-500 hover:bg-slate-50 rounded-md transition-all active:scale-95`
- Rounded variant: `rounded-full`
- Size: `p-1.5` for compact, `p-2` for standard

**FAB (Floating Action Button, screen 06):**
- Container: `fixed bottom-8 right-8`
- Button: `w-14 h-14 bg-indigo-600 text-white rounded-full shadow-2xl flex items-center justify-center hover:scale-110 transition-transform active:scale-95`

**Button with icon:**
- Layout: `flex items-center gap-2` (or `gap-3`)
- Icon size: `text-sm` for inline, default for standalone

**Sticky Bottom Bar (screens 02, 07):**
- Container: `fixed bottom-0 left-64 right-0 p-6 bg-white/95 backdrop-blur-sm border-t border-outline-variant shadow-[0_-4px_12px_rgba(0,0,0,0.05)] z-40`
- Content: `max-w-5xl mx-auto flex items-center justify-between`

---

## 10. Progress Bar

**Large Progress Bar (screen 05):**
- Track: `h-4 w-full bg-slate-200 rounded-full overflow-hidden shadow-inner`
- Fill: `h-full bg-primary-container w-[65%] rounded-full relative transition-all duration-1000 ease-in-out`
- Animated overlay: `absolute inset-0 bg-white/20 animate-pulse`

**Small Confidence Bar (screen 03, table column):**
- Track: `w-full bg-slate-100 h-1.5 rounded-full overflow-hidden`
- Fill: `bg-indigo-500 h-full w-[92%]` (or `bg-indigo-300` for lower confidence)
- No text label on bar itself; percentage shown separately if needed

**Stream Item Mini Progress (screen 05):**
- Track: `h-1 w-full bg-slate-200 rounded-full mt-1`
- Fill: `h-full bg-indigo-500 w-[98%] rounded-full` (included) or `bg-slate-400` (rejected)
- Width container: `w-16 text-right`
- Percentage: `text-xs font-mono text-slate-600`

---

## 11. Stepper / Wizard

**From screen 02 (RIS Import):**

- Container: `max-w-5xl mx-auto mb-10`
- Layout: `flex items-center justify-between`
- Each step is a flex column: `flex flex-col items-center gap-2 flex-1`

**Active Step:**
- Circle: `w-10 h-10 rounded-full bg-primary text-on-primary flex items-center justify-center font-bold shadow-sm`
- Label: `font-h2 text-primary` (16px/600 in primary color)

**Inactive Step:**
- Circle: `w-10 h-10 rounded-full bg-surface-container-high text-on-surface-variant flex items-center justify-center font-medium`
- Label: `font-body-sm text-on-surface-variant` (13px/400 in secondary color)

**Connector Line:**
- Style: `h-px bg-outline-variant flex-1 mb-6`
- Position: between step columns

**Step labels:** "Upload File", "Parse & Validate", "Deduplication", "Import Complete"

---

## 12. PRISMA Diagram Layout

**From screen 09:**

**Main Flow Box:**
- Width: `w-64` (256px)
- Background: `bg-surface` (or `bg-primary-fixed` for final "Included" box)
- Border: `border border-outline-variant rounded-lg`
- Padding: `p-4`
- Shadow: `shadow-sm`
- Text alignment: `text-center`
- Title: `font-h2 text-h2 text-on-surface mb-1` (16px/600)
- Description: `font-body-sm text-body-sm text-on-surface-variant` (13px/400)

**Included (Final) Box:**
- Background: `bg-primary-fixed` (`#e2dfff`)
- Border: `border border-primary-fixed-dim` (`#c3c0ff`)
- Title text: `text-on-primary-fixed`
- Description: `text-on-primary-fixed-variant`

**Side Box (exclusions/duplicates):**
- Width: `w-48` (192px)
- Background: `bg-surface-container-low`
- Border: `border border-outline-variant border-dashed rounded-lg p-3 text-center`
- Text: `font-body-sm text-body-sm text-on-surface-variant`
- Position: offset to the right via absolute positioning

**Connector Lines:**
- Vertical line: `w-px h-full bg-outline-variant absolute left-1/2 transform -translate-x-1/2`
- Horizontal line: `w-32 h-px bg-outline-variant`
- Arrow icons: `material-symbols-outlined text-outline-variant text-[16px]` using `arrow_right` and `arrow_drop_down`

**Exclusion Reason List:**
- Text: `text-[11px] leading-[16px] text-on-surface-variant/80 list-disc pl-4 space-y-1`

**Toggle (show/hide exclusion reasons):**
- Standard toggle switch (see Form pattern)

**Export Buttons:**
- Style: `flex items-center gap-2 px-4 py-2 border border-outline-variant rounded-lg font-body-main text-on-surface hover:bg-surface-container transition-colors`

---

## 13. Empty State / Placeholder

**No explicit empty state screen was designed.** However, the following patterns serve as scaffolding for empty states:

**Skeleton / Placeholder Items (screen 04, background list):**
- Container: `bg-white p-4 rounded-xl border border-slate-200 flex items-start gap-4`
- Placeholder image: `w-10 h-10 rounded bg-slate-100 flex-shrink-0`
- Placeholder line 1: `h-4 w-3/4 bg-slate-200 rounded mb-2`
- Placeholder line 2: `h-3 w-1/2 bg-slate-100 rounded`

**Upload Zone (screen 02, also serves as initial state):**
- Container: `bg-white p-8 rounded-xl border-2 border-dashed border-outline-variant flex flex-col items-center justify-center transition-all cursor-pointer group min-h-[300px]`
- Hover: `ris-upload-zone:hover { border-color: #3525cd; background-color: rgba(79, 70, 229, 0.04); }`
- Icon: `w-16 h-16 bg-surface-container-low rounded-full flex items-center justify-center mb-4 group-hover:scale-110 transition-transform duration-300`
- Large icon inside: `material-symbols-outlined text-primary text-4xl` (cloud_upload)
- Title: `font-h1 text-on-surface mb-2` (20px/600)
- Subtitle: `font-body-sm text-on-surface-variant` (13px/400)

---

## 14. Screen-Specific Notes

### Screen 01: Project Dashboard (`01-dashboard.html`)

**v3 scope elements:**
- Project name display with "Active Project" badge
- Article count stats by status (Imported, Working, Included, Rejected) in bento grid cards
- "Start AI Screening" CTA button
- Quick-action cards: Import RIS, Edit Criteria, View PRISMA
- Recent Activity feed with timeline items
- Screening Progress bar/chart area

**Stitch elements to skip:**
- Top app bar with Projects/Archive/Team tabs (v3 is single-project, no workspace tabs)
- Collaboration Sync card with team member avatars (multi-user is out of scope)
- Screening Progress "Over Time" chart with bar visualization (v3 spec only requires simple progress bar)
- Export Results button in header (premature at dashboard level; export is per-view)

---

### Screen 02: RIS Import (`02-ris-import.html`)

**v3 scope elements:**
- Stepper/wizard (4 steps: Upload File, Parse & Validate, Deduplication, Import Complete)
- Drag-and-drop upload zone (`.ris` files, max 50MB per v3 spec)
- Parsed articles preview table (Title, Authors, Year, DOI columns)
- Sticky bottom summary card with total articles, duplicates detected, parsing validation status
- "Add to Project" and "Cancel" action buttons
- File count badge: `10 of 1,240 rows shown`

**Stitch elements to skip:**
- Sidebar nav items "Sources" and "Library" (v3 uses Dashboard/Articles/Criteria/Screening/Settings)
- Header breadcrumb "Project Alpha" (v3 is single-project)
- Sidebar "New Project" button label (v3 uses "New Search" pattern or no button at all since single-project)

---

### Screen 03: Article List View (`03-article-list.html`)

**v3 scope elements:**
- Filterable/sortable data table with columns: checkbox, Title, Authors, Year, Journal, Status, Confidence, Tags
- Status badges (Included, Pending, Rejected, Reviewing)
- Confidence mini progress bar per row
- Tag chips (solid bg) and label chips (outlined) in Tags column
- Toolbar with Filter, Sort, Bulk Actions
- Pagination footer
- Sidebar with article status counts (112 Included, 678 Rejected)

**Stitch elements to skip:**
- Top header nav tabs (Projects/Archive/Team)
- Sidebar nav item labels differ slightly from v3 canonical set (should use: Dashboard, Articles, Criteria, Screening, Settings)
- "Export Results" button placement in header (keep but may move per view)

---

### Screen 04: Article Detail Panel (`04-article-detail.html`)

**v3 scope elements:**
- Right-sliding side panel (`w-[480px]`) over dimmed article list
- Full article title, metadata grid (Journal, Year, DOI with open_in_new link)
- AI Decision card (green tinted for Included): decision, confidence %, reasoning text
- Matched Criteria list with check/radio icons
- Abstract section
- Tags section with solid-bg chips and close buttons
- Labels section with outlined chips and close buttons
- "Add Tag" button (dashed border)
- Audit Trail timeline with vertical line and dot indicators
- Footer actions: "Move to Archive", "Export Analysis"

**Stitch elements to skip:**
- AI Decision card uses hardcoded hex colors (`#ECFDF5`, `#059669`, etc.) -- implement with design tokens or Tailwind `emerald-*` classes
- "Move to Archive" footer button (no archive concept in v3)
- Background list skeleton placeholders are reference only

---

### Screen 05: AI Screening Progress (`05-screening-progress.html`)

**v3 scope elements:**
- Large progress bar (h-4, with animated pulse overlay)
- Percentage display (65%) with "Completion" label
- Processing count: "Processing: 806 / 1240 articles"
- Status bento grid: Included count, Rejected count, Errors count
- Live Decision Stream with scrolling feed
- Stream items: timestamp (mono), article title, status badge, confidence mini-bar
- Live indicator (ping animation): `animate-ping` dot
- Control buttons: Pause (primary), Stop (secondary with red icon)
- "View Current Results" link

**Stitch elements to skip:**
- Background decorative blurred circles (`bg-indigo-50/50 rounded-tl-full blur-3xl`)
- Header nav tabs (Projects/Archive/Team)

---

### Screen 06: Criteria Editor (`06-criteria-editor.html`)

**v3 scope elements:**
- Three-section layout: Research Aims, Inclusion Criteria, Exclusion Criteria
- Research Aims: numbered entries with inline text inputs, delete on hover
- Inclusion Criteria: cards with priority-colored left border, textarea, priority dropdown (Critical/High/Standard/Low/Optional)
- Exclusion Criteria: same pattern as inclusion
- "Add Criterion" buttons per section
- Sticky save/cancel action bar at bottom
- FAB button (publish icon)

**v3 Priority colors to implement:**
- Critical: red (`border-red-500 bg-red-50/30`)
- High: orange (`border-orange-500 bg-orange-50/30`)
- Standard: indigo (`border-indigo-500 bg-indigo-50/30`)
- Low: gray (`border-slate-400 bg-slate-50/50`)
- Optional: gray dashed (`border-slate-300 border-dashed bg-white`)

**Stitch elements to skip:**
- FAB button at bottom-right (not in v3 spec for this screen)
- Sidebar icon variations across screens (use canonical set: Dashboard, Articles, Criteria, Screening, Settings)

---

### Screen 07: Deduplication Review (`07-dedup-review.html`)

**v3 scope elements:**
- Header with title and description
- Pair navigation: chevron left/right with "Pair 14 of 82" counter
- Similarity Score Banner: match type badges (Title Match, Metadata Match, Same DOI)
- Side-by-side comparison grid (2 columns): Record A vs Record B
- Record cards with: letter badge (A/B), source label, title, authors, year, journal, DOI, abstract
- Yellow-highlighted differences in text: `bg-yellow-100 px-1 rounded`
- Sticky bottom action bar: "Keep Both (Unique)", "Keep Record A", "Keep Record B"
- Keyboard shortcut hint: `kbd` styled keys

**Stitch elements to skip:**
- Sidebar nav items differ from canonical set (uses Library, Deduplication, Synthesis)
- "New Project" sidebar button label (v3 uses canonical sidebar)
- Sidebar background `bg-slate-900` differs from standard `bg-slate-800`
- "ScholarSync" in title tag (should be "Bango")

---

### Screen 08: Tag & Label Management (`08-tags-labels.html`)

**v3 scope elements:**
- Dual-panel layout (2 columns): Tags (left), Labels (right)
- Each panel: scrollable list with header, count badge, add input, AI generate button
- Tags: solid bg chips with border, monospace text, article count, hover actions (edit/delete)
- Labels: outlined chips with colored dot indicator, monospace text, article count, hover actions
- "Generate from AI" button per panel (maps to v3's "Suggest Tags" / "Suggest Labels")
- Search input within each panel header
- Count badges: `bg-surface-variant text-on-surface-variant px-2 py-0.5 rounded-full font-label-caps text-label-caps`

**Stitch elements to skip:**
- Sidebar nav items differ from canonical set (uses Library, Research, Tags & Labels, Collections)
- "AI Assistant" button in header (not a v3 feature)
- Header search is bare input (no standard search component wrapper)

---

### Screen 09: PRISMA 2020 Flow Diagram (`09-prisma-diagram.html`)

**v3 scope elements:**
- Four-phase vertical flow: Identification, Screening, Eligibility, Included
- Each box: 256px wide, rounded-lg, centered text, with count
- Side boxes (dashed border) for excluded/duplicate counts with branching arrows
- Toggle switch for "Show exclusion reasons breakdown"
- Exclusion reason list in final side box
- Export buttons: SVG, PNG (v3 does not include PDF export)
- Final "Included" box uses `bg-primary-fixed` highlighting

**Stitch elements to skip:**
- "Export as PDF" button (v3 spec: SVG and PNG only)
- Sidebar nav items differ from canonical set (uses Library, PRISMA Flow, Data Extraction, Synthesis, Project Settings)
- "ResearchStream" in title tag (should be "Bango")
- Sidebar uses `bg-slate-900` instead of standard `bg-slate-800`

---

### Screen 10: LLM Configuration (`10-llm-config.html`)

**v3 scope elements:**
- Page title and description
- Warning banner: `bg-yellow-50 border border-yellow-200 rounded-lg p-4` with warning icon and text about VRAM requirements
- Bento layout grid (2/3 + 1/3): Connection Details + Parameters
- Provider dropdown (OpenAI, Google, z.ai, llama.cpp, Ollama, LM Studio, Custom)
- Endpoint URL input (mono font)
- Model Name input (mono font)
- API Key input (password type, visibility toggle)
- Max Context Tokens range slider (1k-50k) with value badge
- Concurrency Threads number input
- Request Delay number input
- Connection status indicator: dot + label ("Disconnected")
- Footer: Revert button, Test Connection button

**Stitch elements to skip:**
- Sidebar "New Project" button (v3 is single-project)
- Sidebar active state uses `border-l-4 border-indigo-500` variant (use standard `bg-slate-700` active pattern)
- Header search placement (some screens show search on left, others on right -- standardize)
- "Systematic Review" subtitle (v3 uses "Literature Review")

---

## Appendix: Shared CSS Patterns

**Scrollbar styling (screen 04):**
```css
::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: #e2e8f0; border-radius: 10px; }
::-webkit-scrollbar-thumb:hover { background: #cbd5e1; }
```

**Custom scrollbar (screen 06):**
```css
.custom-scrollbar::-webkit-scrollbar { width: 4px; }
.custom-scrollbar::-webkit-scrollbar-track { background: transparent; }
.custom-scrollbar::-webkit-scrollbar-thumb { background: #e2dfff; border-radius: 10px; }
```

**Stream scroll mask (screen 05):**
```css
.stream-scroll {
    mask-image: linear-gradient(to bottom, transparent, black 10%, black 90%, transparent);
}
```

**Upload zone hover (screen 02):**
```css
.ris-upload-zone:hover {
    border-color: #3525cd;
    background-color: rgba(79, 70, 229, 0.04);
}
```

**Text selection color (screen 07):**
```css
selection:bg-primary-container selection:text-on-primary-container
```

---

## 15. Responsive Design System

Bango implements a mobile-first responsive design using CSS breakpoints. All views adapt from small screens (360px+) to full desktop (1440px+).

### Breakpoints

| Name | Width | Tailwind Class | Target Devices |
|------|-------|---------------|----------------|
| Mobile | 0–767px | Default | Phones, small tablets |
| Tablet | 768–1023px | `md:` | Tablets, small laptops |
| Desktop | 1024px+ | `lg:` | Laptops, desktops |
| Wide | 1280px+ | `xl:` | Large desktops |

### Responsive Tokens (tokens.css)

```css
--container-padding: 24px;
--container-padding-sm: 16px;
```

All views use `--container-padding` as their outer padding, switching to `--container-padding-sm` below 768px.

### Navigation Sidebar Behavior

| Breakpoint | Behavior |
|-----------|----------|
| Desktop (≥1024px) | Full sidebar visible (260px wide), content shifts right |
| Tablet/Mobile (<1024px) | Sidebar hidden, hamburger menu in header toggles overlay |

On mobile, the sidebar slides over content with a dark backdrop (`bg-black/50`). The viewport composable (`use-viewport.ts`) provides reactive `isMobile` and `isTablet` refs.

### Article Detail Panel Behavior

| Breakpoint | Behavior |
|-----------|----------|
| Desktop (≥768px) | Side panel (480px wide) slides in from right, content area remains visible but dimmed |
| Mobile (<768px) | Full-screen overlay panel with close button |

### Article Table Adaptations

| Breakpoint | Hidden Columns |
|-----------|---------------|
| Desktop (≥1024px) | All columns visible |
| Tablet (768–1023px) | "Imported" date column hidden |
| Mobile (<768px) | "Journal", "Confidence", "Imported" columns hidden; table scrolls horizontally |

### Grid Layout Adaptations

| View | Desktop | Tablet/Mobile |
|------|---------|---------------|
| Dashboard stats | 4-column grid | 2-column grid |
| Dashboard main/sidebar | 2:1 grid | Single column, sidebar below |
| Tags & Labels | 2-column grid | Single column, stacked |
| LLM Config | 2:1 grid | Single column |
| Criteria cards | Row layout | Column layout (stacked) |

### Viewport Composable (`use-viewport.ts`)

```ts
const { isMobile, isTablet, isDesktop, width } = useViewport();
```

- `isMobile`: `width < 768`
- `isTablet`: `width >= 768 && width < 1024`
- `isDesktop`: `width >= 1024`

### Responsive Pattern Summary per View

| View | Mobile Adaptations |
|------|--------------------|
| `dashboard.vue` | 2-col stats, stacked grid, column header |
| `article-list.vue` | Scrollable status tabs, full-screen detail panel |
| `article-table.vue` | Hidden columns, horizontal scroll wrapper |
| `article-detail-panel.vue` | Full-screen overlay mode |
| `criteria-editor.vue` | Stacked criterion cards, wrapped inputs |
| `tag-label-management.vue` | Stacked panels, auto height |
| `llm-config.vue` | Single-column form, stacked footer |
| `screening-progress.vue` | Column header, wrapped controls |
| `dedup-review.vue` | Column header, wrapped stats |
| `import-ris.vue` | Wrapped stats, responsive padding |
| `prisma-diagram.vue` | Column header, scrollable diagram |
| `summary-view.vue` | Column header, responsive padding |
