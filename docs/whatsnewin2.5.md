# What's New in Bango v2.5

*31 commits since v2.3.1 (June 22, 2026)*

---

## 🌐 Multi-Language Translation (New Feature)

The headline feature of this release - Bango now translates articles into multiple languages:

- **Automatic translation** of article metadata (titles, abstracts, keywords) using your configured LLM provider
- **Batch translation** - translate many articles at once, with smart batching to avoid rate limits
- **10 languages supported** with tested assets: Arabic, German, Spanish, French, Italian, Japanese, Portuguese, Russian, Turkish, and Chinese
- A new **Translation** section in the article detail panel
- Translations persist in the database across sessions

## 🤖 AI Summaries & LLM Improvements (Tier System)

The AI pipeline is significantly upgraded across multiple tiers:

- **Tier 1**: Better support for reasoning models (e.g. OpenAI o-series) - the app now detects truncated model output and surfaces a warning instead of silently accepting incomplete results
- **Tier 2**: Refined LLM prompts and reliability fixes
- **Tier 4**: Toast notifications for AI operations so you can track what's happening in the background
- **Per-section summaries** - AI can now generate separate summaries for *Methods*, *Results*, and *Discussion* sections, in addition to the whole-paper summary
- **Figure & table descriptions** - AI can describe captioned figures and tables, making visual content discoverable
- LLM timeout increased from 2 minutes to 10 minutes

## 📋 Screening Overhaul

The systematic review screening workflow has been redesigned:

- **Two-stage screening**: Stage 1 screens by abstract; Stage 2 retrieves and reviews relevant full-text chunks for more informed decisions
- **Screening limits** - set a maximum number of articles per screening session to prevent runaway jobs
- **Redesigned screening screen** with better progress tracking and a cleaner layout
- **Evidence tracking** - screening decisions now capture the specific text evidence behind include/exclude choices

## 📦 Batch Import

Import multiple articles at once through a phased workflow:

- Citations import phase
- Full-text retrieval phase
- AI summary generation phase
- Translation phase
- Each phase runs independently - import references now, fetch full-texts later

## 📄 PDF Improvements

- **Better extraction** with a fallback mechanism - if one method fails, Bango tries alternatives
- **Legacy encoding support** - older Japanese PDFs with pre-Unicode encodings are now handled correctly
- **Graceful failure handling** - empty or corrupted PDF uploads surface a clear error instead of breaking the workflow

## ⚙️ Settings Restructured

Settings have been reorganized for clarity:

- **AI Summaries** - new dedicated settings panel
- **Reprocessing** - re-trigger AI operations on existing articles
- **Notification History** - review past AI operation notifications
- **Storage** - renamed and reorganized from "Full-Text Storage"

## 🏠 Landing Page & Help

- New **landing page** for the app
- Expanded **help guide** with new Reference and Troubleshooting tabs
- **Toast notification system** for non-intrusive status updates throughout the app

## 🔧 Other Improvements

- Clearer, more actionable **error messages** for database connection issues and operation failures
- **Improved deduplication** when importing references
- **More flexible button layout** on the dashboard
- Documentation updated to cover full-text, tables, and figures
