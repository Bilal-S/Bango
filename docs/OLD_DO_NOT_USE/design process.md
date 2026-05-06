Ready for review
Select text to add comments on the plan
Plan: Google Stitch UI Design Prompts & MCP Integration for Bango
Context
Bango is in the requirements/planning phase with a complete specification but no UI yet. Google Stitch (stitch.withgoogle.com) can generate production-ready UI designs from text prompts, and its MCP server integrates directly with Claude Code for design-to-code handoff. This plan creates the Stitch prompts to generate Bango's UI and recommends an MCP-based workflow.

1. Initial Stitch Prompt - Full Application Shell
Paste this as the first prompt in Google Stitch. It generates the main layout and navigation.

Design a cross-platform desktop app called "Bango" - an AI-powered systematic literature review tool built with Tauri (lightweight native feel, no electron bloat).

CONCEPT: Researchers import bibliography files, define research criteria, and let AI screen hundreds of abstracts to produce a categorized list of included/excluded articles.

SCREENS TO GENERATE (show all on one canvas):

1. **Project Dashboard** - landing screen after opening a project. Shows: project name, article counts by status (Imported / Working / Included / Rejected as pill badges), a "Start Screening" CTA button, recent activity feed, and quick-action cards for "Import RIS", "Edit Criteria", "View PRISMA Diagram".

2. **Article List View** - the core data-heavy screen. Left sidebar with status tabs (Imported, Working, Included, Rejected) showing counts. Main area: a filterable/sortable table of articles with columns: checkbox, title, authors, year, journal, status badge, confidence score bar, tags as colored chips, labels as outlined chips. Top toolbar: search bar, sort dropdown, filter panel toggle, bulk actions dropdown.

3. **Article Detail Panel** - slides in from the right as a side panel (not a modal). Shows: full title, abstract text in a scrollable block, metadata fields (DOI, journal, year, keywords), AI decision card (Included/Rejected with confidence %, reasoning paragraph, matched criteria list), tags section with editable chips, labels section, audit trail timeline at the bottom with timestamped entries.

4. **Criteria Editor** - split into three sections: Research Aims (list of text entries with add/delete), Inclusion Criteria (each entry has text + priority dropdown: Critical/High/Standard/Low/Optional), Exclusion Criteria (same format). Use colored left-border indicators for priority levels (red=Critical, orange=High, blue=Standard, gray=Low, dashed=Optional).

5. **AI Screening Progress** - shows a progress bar, articles processed / total count, current batch info, a live-updating list of recently screened articles with their decisions, and controls for pause/resume/stop. Include a small stats panel: included count, rejected count, error count.

VISUAL TONE: Clean, academic, professional. Think Notion meets Zotero. Light theme with a cool gray palette, indigo accent color for primary actions, subtle shadows, rounded corners (8px), generous whitespace. Data-dense but not cluttered. Sidebar navigation is dark (slate-800). Typography: system font stack, clear hierarchy.

TARGET: Desktop layout (1280px width). No mobile layout for this prompt.
2. Follow-Up Iteration Prompts
Run these one at a time after the initial generation to refine individual screens.

Screen: RIS Import Flow
Add a new screen to the canvas: RIS Import. Show a drag-and-drop zone for .ris files with a dashed border and upload icon. Below it: a table preview of parsed articles (first 10 rows showing title, authors, year, DOI). At the bottom: import summary card showing total articles parsed, duplicates detected, and an "Add to Project" button. Include a progress state with a stepper: 1. Upload File → 2. Parse & Validate → 3. Deduplication → 4. Import Complete.
Screen: Deduplication Review
Add a Deduplication Review screen. Show a side-by-side comparison of two duplicate articles. Left panel: "Record A" with all metadata. Right panel: "Record B" with all metadata. Differences highlighted in yellow. Bottom bar with three action buttons: "Keep A", "Keep B", "Keep Both". Above the comparison: a list of duplicate pairs with similarity score (e.g., "95% title match") and navigation arrows to move between pairs.
Screen: PRISMA 2020 Flow Diagram
Add a PRISMA 2020 Flow Diagram viewer screen. Display the standard four-phase flow diagram: Identification → Screening → Eligibility → Included. Each phase is a rounded rectangle with record counts, connected by downward arrows. Branching arrows show records removed at each stage with reason labels (e.g., "duplicates removed (n=142)", "excluded by AI (n=387)"). Below the diagram: export buttons for SVG, PNG, PDF. Include a toggle switch for "Show exclusion reasons breakdown".
Screen: LLM Configuration
Add an LLM Configuration screen. Show a form with: provider dropdown (OpenAI, Google, z.ai, llama.cpp, Ollama, LM Studio, Custom), endpoint URL text input (with placeholder "https://api.openai.com/v1/chat/completions"), model name input, API key input (masked with show/hide toggle), max tokens slider (range 1000–50000, default 4000), concurrency input (default 3), request delay input in ms (default 500). At the bottom: a "Test Connection" button and a status indicator showing connection result. Include a VRAM warning banner: "⚠️ Local providers require 16+ GB VRAM for 50k token context".
Screen: Tag & Label Management
Add a Tag & Label Management screen. Two sections side by side. Left: "Tags" - a list of content-category labels (e.g., "machine-learning", "clinical-trial") as colored removable chips, with an input to add new tags. Each tag shows article count. Right: "Labels" - workflow markers (e.g., "priority-read", "disputed") as outlined chips, with same add/delete capability. Both sections have a "Generate from AI" button that triggers tag/label suggestions based on criteria.
3. MCP Integration Recommendation
Setup Steps
Enable Stitch MCP Server: In Google Stitch settings, enable the MCP server endpoint. This exposes a standard MCP interface that Claude Code, Cursor, and Gemini CLI can connect to.

Configure Claude Code: Add the Stitch MCP server to .claude/settings.json or project-level .claude/settings.local.json:

{
  "mcpServers": {
    "stitch": {
      "command": "npx",
      "args": ["-y", "@anthropic/mcp-client"],
      "env": {
        "STITCH_API_KEY": "<from-stitch-settings>"
      }
    }
  }
}

Create DESIGN.md: After generating designs in Stitch, export the design system as a DESIGN.md file and place it in the Bango project root. This captures colors, typography, spacing, and component specs in a portable markdown format that both Stitch and Claude Code can read.

Workflow
Stitch (generate designs)
    → Export DESIGN.md
    → Claude Code reads DESIGN.md + generates Vue components
    → Iterate in Stitch for refinements
    → Re-sync via DESIGN.md
Key Benefits
Design-to-code consistency: DESIGN.md acts as a single source of truth for design tokens (colors, spacing, typography)
Iterative refinement: Make visual tweaks in Stitch, re-export DESIGN.md, and Claude Code regenerates affected components
No Figma middleware: Direct Stitch → code pipeline without manual design handoff
Version controlled: DESIGN.md is plain markdown, so it lives in the repo alongside code
DESIGN.md Template for Bango
After generating designs, create a DESIGN.md in the project root with this structure (fill in values from Stitch output):

# Bango Design System

## Colors
- Primary (indigo): #4F46E5
- Surface: #FFFFFF
- Background: #F8FAFC
- Sidebar: #1E293B
- Text primary: #0F172A
- Text secondary: #64748B
- Accent/Critical: #EF4444
- Accent/High: #F97316
- Accent/Standard: #3B82F6
- Accent/Low: #6B7280
- Accent/Optional: #9CA3AF
- Border: #E2E8F0

## Typography
- Font family: Inter, system-ui, sans-serif
- H1: 24px/600
- H2: 20px/600
- Body: 14px/400
- Caption: 12px/400
- Mono: JetBrains Mono for code/data

## Spacing
- Base unit: 4px
- Component padding: 12px
- Section gap: 24px
- Card padding: 16px
- Border radius: 8px (cards, buttons), 6px (inputs)

## Components
[Populate from Stitch output - button variants, badge styles, table styles, etc.]
4. Verification
Copy the initial prompt into Google Stitch at https://stitch.withgoogle.com/
Verify it generates 5 screens on the canvas matching the Bango specification
Run follow-up prompts one at a time to add additional screens
Export the design system as DESIGN.md and place in project root
Configure the Stitch MCP server in Claude Code settings
Test the MCP connection by asking Claude Code to read DESIGN.md and generate a Vue component
Add Comment