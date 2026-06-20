//! The verbatim `AGENTS.md` contract written into `wiki-root/AGENTS.md`.
//!
//! Kept in code (not as a bundled resource) so it is always in sync with the
//! binary and trivially testable. The ingest prompt prepends this content so
//! the LLM operates under a stable contract.

/// The full agent contract. Written to `wiki-root/AGENTS.md` on `wiki_init`.
///
/// Rules kept aligned with `.worktrees/llmwiki-plan.md` §5 and `docs/CLAUDE.md`
/// (no em dashes, kebab-case slugs, mandatory frontmatter).
#[must_use]
pub fn agents_md_content() -> &'static str {
    "# Bango LLM Wiki - Agent Contract\n\
     \n\
     ## Purpose\n\
     Build and maintain a research knowledge base from the project's included\n\
     articles. Synthesize concepts, methods, authors, and themes with strict\n\
     provenance back to /raw sources.\n\
     \n\
     ## Directory Layout\n\
     - /raw            Immutable article exports. NEVER edit or delete from agent.\n\
     - /wiki           LLM-generated pages. Read/write here.\n\
     - /wiki/index.md  Master catalog. Regenerate on every ingest.\n\
     - /templates      Page skeletons. Read-only reference.\n\
     - /wiki/log.md   Append-only run audit trail.\n\
     \n\
     ## Content Hierarchy (per article, first available wins)\n\
     1. full_text\n\
     2. full_text_ai_summary (summary_150_250_words field if JSON)\n\
     3. abstract_text\n\
     ALWAYS include keywords, tags, authors, year, doi as frontmatter.\n\
     \n\
     ## Ingest Workflow\n\
     1. Read every .md in /raw (skip unchanged by hashing frontmatter exported_at).\n\
     2. Extract entities: concepts, methods, authors, themes.\n\
     3. For each entity, upsert /wiki/{type}/{slug}.md (kebab-case slug).\n\
     4. Insert [[wikilinks]] between related entities and source raw files.\n\
     5. Cite every factual claim with a footnote [^art-{article_id}] linking /raw.\n\
     6. Generate the `summary` frontmatter field (1-2 sentence digest).\n\
     7. Regenerate /wiki/index.md with the full page list grouped by type.\n\
     8. Append a run entry to /wiki/log.md (timestamp, counts, model, tokens).\n\
     \n\
     ## Lint Workflow (deterministic, no LLM required)\n\
     1. Walk /wiki/**/*.md, parse frontmatter + [[links]].\n\
     2. Build link graph. Detect:\n\
        - broken links (target page missing)\n\
        - orphan pages (zero inbound links, not in index)\n\
        - duplicate concepts (same normalized title)\n\
        - missing/invalid frontmatter\n\
        - stale pages (source article no longer included)\n\
     3. Auto-fix safe issues (slug normalization, index rebuild).\n\
     4. Report unfixable issues; mark affected pages status: stale.\n\
     \n\
     ## Rules\n\
     - Never fabricate citations. Every claim must trace to a /raw file.\n\
     - Never overwrite a page whose frontmatter status is 'reviewed'; mark it\n\
       status: stale and log a conflict instead.\n\
     - Use kebab-case filenames. No spaces, no unicode in slugs.\n\
     - Frontmatter is mandatory; pages without it are lint errors.\n\
     - Em dashes are forbidden in generated prose (project rule).\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_mentions_all_key_sections() {
        let c = agents_md_content();
        // Core sections present
        assert!(c.contains("## Purpose"));
        assert!(c.contains("## Directory Layout"));
        assert!(c.contains("## Content Hierarchy"));
        assert!(c.contains("## Ingest Workflow"));
        assert!(c.contains("## Lint Workflow"));
        assert!(c.contains("## Rules"));
        // Directory tokens referenced
        assert!(c.contains("/raw"));
        assert!(c.contains("/wiki"));
        assert!(c.contains("/templates"));
        assert!(c.contains("/wiki/log.md"));
        // No em dash anywhere (project rule)
        assert!(!c.contains('\u{2014}'), "em dash forbidden in generated text");
        assert!(!c.contains('\u{2013}'), "en dash forbidden in generated text");
    }

    #[test]
    fn contract_forbids_fabrication_and_protects_reviewed() {
        let c = agents_md_content();
        assert!(c.contains("Never fabricate citations"));
        assert!(c.contains("reviewed"));
    }
}
