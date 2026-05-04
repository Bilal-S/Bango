---
name: Scholarly Precision
colors:
  surface: '#fcf8ff'
  surface-dim: '#dcd8e5'
  surface-bright: '#fcf8ff'
  surface-container-lowest: '#ffffff'
  surface-container-low: '#f5f2ff'
  surface-container: '#f0ecf9'
  surface-container-high: '#eae6f4'
  surface-container-highest: '#e4e1ee'
  on-surface: '#1b1b24'
  on-surface-variant: '#464555'
  inverse-surface: '#302f39'
  inverse-on-surface: '#f3effc'
  outline: '#777587'
  outline-variant: '#c7c4d8'
  surface-tint: '#4d44e3'
  primary: '#3525cd'
  on-primary: '#ffffff'
  primary-container: '#4f46e5'
  on-primary-container: '#dad7ff'
  inverse-primary: '#c3c0ff'
  secondary: '#545f73'
  on-secondary: '#ffffff'
  secondary-container: '#d5e0f8'
  on-secondary-container: '#586377'
  tertiary: '#7e3000'
  on-tertiary: '#ffffff'
  tertiary-container: '#a44100'
  on-tertiary-container: '#ffd2be'
  error: '#ba1a1a'
  on-error: '#ffffff'
  error-container: '#ffdad6'
  on-error-container: '#93000a'
  primary-fixed: '#e2dfff'
  primary-fixed-dim: '#c3c0ff'
  on-primary-fixed: '#0f0069'
  on-primary-fixed-variant: '#3323cc'
  secondary-fixed: '#d8e3fb'
  secondary-fixed-dim: '#bcc7de'
  on-secondary-fixed: '#111c2d'
  on-secondary-fixed-variant: '#3c475a'
  tertiary-fixed: '#ffdbcc'
  tertiary-fixed-dim: '#ffb695'
  on-tertiary-fixed: '#351000'
  on-tertiary-fixed-variant: '#7b2f00'
  background: '#fcf8ff'
  on-background: '#1b1b24'
  surface-variant: '#e4e1ee'
typography:
  display:
    fontFamily: Inter, system-ui
    fontSize: 24px
    fontWeight: '600'
    lineHeight: 32px
    letterSpacing: -0.02em
  h1:
    fontFamily: Inter, system-ui
    fontSize: 20px
    fontWeight: '600'
    lineHeight: 28px
    letterSpacing: -0.01em
  h2:
    fontFamily: Inter, system-ui
    fontSize: 16px
    fontWeight: '600'
    lineHeight: 24px
  body-main:
    fontFamily: Inter, system-ui
    fontSize: 14px
    fontWeight: '400'
    lineHeight: 20px
  body-sm:
    fontFamily: Inter, system-ui
    fontSize: 13px
    fontWeight: '400'
    lineHeight: 18px
  label-caps:
    fontFamily: Inter, system-ui
    fontSize: 11px
    fontWeight: '600'
    lineHeight: 16px
    letterSpacing: 0.05em
  mono:
    fontFamily: ui-monospace, SFMono-Regular
    fontSize: 13px
    fontWeight: '400'
    lineHeight: 18px
rounded:
  sm: 0.25rem
  DEFAULT: 0.5rem
  md: 0.75rem
  lg: 1rem
  xl: 1.5rem
  full: 9999px
spacing:
  unit: 4px
  container-padding: 24px
  sidebar-width: 260px
  gutter: 16px
  stack-gap: 12px
---

## Brand & Style

This design system is engineered for the high-context world of academic research and data management. It adopts a **Minimalist-Corporate** aesthetic that prioritizes utility and clarity over decorative flair. Drawing inspiration from "Notion meets Zotero," the UI provides a lightweight, native-app feel (Tauri-inspired) that feels responsive and unobtrusive.

The brand personality is intellectual, organized, and reliable. It aims to evoke a sense of "quiet focus," allowing the user’s research data to take center stage. The style utilizes heavy whitespace and a restricted color palette to reduce cognitive load during long research sessions.

## Colors

The palette is anchored by a cool gray background to provide a modern, "app-like" canvas. 

- **Primary Indigo (#4F46E5):** Used for primary actions, active states, and "Standard" priority markers.
- **Sidebar Slate (#1E293B):** A deep, authoritative dark slate reserved for navigation to provide clear structural anchoring.
- **Priority Logic:** A specific semantic scale is used for criteria ranking:
    - **Critical:** Bright Red for immediate attention.
    - **High:** Orange for secondary urgency.
    - **Standard:** Indigo to align with the primary brand.
    - **Low:** Medium Gray for routine items.
    - **Optional:** Subtle Gray with dashed borders for supplementary data.

## Typography

This design system utilizes a clean system font stack (Inter as the primary typeface) to ensure native performance and high legibility in data-heavy views. 

The hierarchy is intentionally tight. Because academic software often displays large quantities of text, we favor smaller base sizes (14px) with generous line-heights. **Display** and **H1** styles use slight negative letter spacing to maintain a sophisticated, "published" look, while **Label-caps** are used for metadata headers and sidebar categories to provide visual distinction without increasing size.

## Layout & Spacing

The layout philosophy follows a **Fixed-Fluid Hybrid** model. 
- **Sidebar:** A fixed-width navigation panel in Dark Slate.
- **Master-Detail View:** A 3-pane layout (Navigation > List > Content) common in research tools. 
- **Rhythm:** An 8pt grid system guides spacing, but 4px increments are used for tight data-dense components. 

Generous whitespace (24px container padding) is used to separate high-level functional areas, while internal component spacing (12px gaps) remains compact to maximize information density on laptop screens.

## Elevation & Depth

To maintain a lightweight, "Tauri" feel, this design system avoids heavy shadows. 

**Tonal Layers:** Depth is primarily communicated through background color shifts. The main workspace sits on the cool gray background, while modals and floating panels use a pure white surface.

**Subtle Shadows:** When elevation is required (e.g., for dropdowns or floating action buttons), use a single, highly diffused shadow: `0 4px 12px rgba(0, 0, 0, 0.05)`. 

**Borders:** Content panes are separated by 1px soft borders (`#E5E7EB`) rather than shadows to keep the interface feeling "flat" and efficient.

## Shapes

The shape language is consistent and approachable. A standard radius of **8px (0.5rem)** is applied to all primary containers, buttons, and input fields. 

Secondary elements like **Pill Badges** use a fully rounded (999px) radius to distinguish them from interactive buttons. This contrast between the structured 8px containers and the organic pill shapes helps users quickly identify status indicators versus action triggers.

## Components

- **Buttons:** Primary buttons are solid Indigo with white text. Secondary buttons use a light gray ghost style or a subtle outline.
- **Pill Badges:** Used exclusively for **Status**. These are fully rounded with a soft background tint and dark text (e.g., "Published," "Draft").
- **Colored Chips:** Used for **Tags** (User-defined categories). They feature a solid background and are 8px rounded.
- **Outlined Chips:** Used for **Labels** (System-defined metadata). These have a 1px border, no background, and 8px rounded corners.
- **Priority Indicators:** 
    - **Critical/High/Standard/Low:** Small solid circles or text-pills in the respective semantic color.
    - **Optional:** A dashed border chip with a transparent background.
- **Input Fields:** Minimalist design with a 1px gray border that transitions to a 2px Indigo border on focus. No inner shadows.
- **Data Tables:** Row-based with subtle hover states (`#F3F4F6`). Vertical dividers are omitted to favor horizontal scanning; use thin horizontal rules only.