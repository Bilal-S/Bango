//! Wiki chat — token-budgeted RAG over the FTS5 index.
//!
//! Given a user question:
//! 1. BM25-search the `wiki_pages_fts` index for the top matches.
//! 2. Build a context string from the hits, respecting a token budget
//!    (approximate: 1 token ~= 4 chars). Higher-ranked hits are included first.
//! 3. Send to the LLM via `LlmOrchestrator` with `LlmRequestType::WikiChat`.
//!
//! The system prompt instructs the model to answer from the wiki context, cite
//! pages by slug, and admit when the wiki does not cover the question.

use std::sync::Arc;

use tauri::State;

use crate::commands::chat::ChatMessage;
use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::wiki::fts;

/// Approximate character budget for the wiki context (1 token ~= 4 chars).
/// Conservative to leave room for the system prompt + user question + history.
const CONTEXT_CHAR_BUDGET: usize = 12_000; // ~3000 tokens

/// The maximum number of FTS5 hits to consider for context.
///
/// Raised from 8 to 16 in T1.2 because chunk rows are smaller than whole-page
/// rows, so more fit the char budget. `build_context` dedupes by `parent_slug`
/// so multiple chunks of the same page do not crowd out other pages.
const MAX_HITS: usize = 16;

/// Send a wiki-grounded chat message. Returns the assistant response text.
pub async fn wiki_chat(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    question: &str,
    history: &[ChatMessage],
) -> Result<String, AppError> {
    // 1. Resolve the wiki root.
    let root = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        crate::wiki::storage::resolve_root(&conn)?
    };

    // 2. BM25 search for the most relevant wiki pages. `ensure_index_populated`
    //    self-heals the desync where pages exist on disk but the FTS table is
    //    empty (e.g. after a schema rebuild / DB reset that dropped the table
    //    but left the wiki/*.md files intact).
    let hits = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        fts::ensure_index_populated(&conn, &root)?;
        fts::search(&conn, question, MAX_HITS)?
    };

    // 3. Build the token-budgeted context from the hits.
    let context = build_context(&hits);

    // 4. Load the LLM config.
    let config = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?
    };

    // 5. Build prompts (delegated to a pure, testable helper).
    let (system_prompt, user_prompt) = build_wiki_prompts(&context, history, question);

    // 6. Send through the orchestrator.
    let (response, _tokens) =
        orchestrator.send(&config, system_prompt, &user_prompt, LlmRequestType::WikiChat).await?;

    Ok(response)
}

/// The wiki-chat system prompt (static). Returned by `build_wiki_prompts` and
/// exposed for tests so the prompt contract is documented alongside its tests.
#[must_use]
pub fn wiki_chat_system_prompt() -> &'static str {
    "You are a research wiki assistant. Answer the researcher's question using \
     the provided wiki page context. Cite pages by their slug in [[double brackets]] \
     when the answer draws on a specific page. When a passage includes a section label \
     like (§Methods), include it in the citation so the reader can locate the passage: \
     [[slug]] (§Methods). Do not invent information. If the wiki context does not cover \
     the question, say so explicitly and suggest which page might need to be created or \
     expanded. Format your response in clean Markdown."
}

/**
 * Build the (system, user) prompt pair for `wiki_chat`.
 *
 * Pure & testable: no DB, no orchestrator. `context` is the token-budgeted
 * string produced by `build_context`; when empty, the user prompt asks the
 * model to tell the user to ingest sources. `history` is rendered as
 * `User: ... / Assistant: ...` lines. The final line is always
 * `User: {question}\nAssistant:`.
 */
#[must_use]
pub fn build_wiki_prompts(
    context: &str,
    history: &[ChatMessage],
    question: &str,
) -> (&'static str, String) {
    let mut user_prompt = String::new();
    if context.is_empty() {
        user_prompt.push_str(
            "The wiki does not yet contain any indexed pages. Let the user know they should \
             ingest sources first (Prepare Raw, then Ingest).\n\n",
        );
    } else {
        user_prompt.push_str("Wiki page context (BM25-ranked, most relevant first):\n\n");
        user_prompt.push_str(context);
        user_prompt.push_str("\n\n");
    }

    if !history.is_empty() {
        user_prompt.push_str("Conversation history:\n");
        for msg in history {
            let role_name = if msg.role == "user" { "User" } else { "Assistant" };
            user_prompt.push_str(&format!("{role_name}: {}\n", msg.content));
        }
        user_prompt.push('\n');
    }

    user_prompt.push_str(&format!("User: {question}\nAssistant:"));

    (wiki_chat_system_prompt(), user_prompt)
}

/// Build a token-budgeted context string from BM25 hits.
///
/// T1.2 chunk-aware behavior:
/// - Dedupe by `parent_slug`: when multiple chunks of the same page match,
///   keep the top-ranked chunk and append "(+N more passages from this page)"
///   so one paper does not crowd out other pages.
/// - Include the section label in the entry header when present, e.g.
///   `## [[slug]] - Title (§Methods)`, so the model can cite the passage.
///
/// Higher-ranked hits are included first; once the char budget is exhausted,
/// remaining hits are skipped (their titles are still listed as "see also").
fn build_context(hits: &[fts::WikiPageHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }

    // Dedupe by parent_slug (falls back to slug for legacy whole-page rows).
    // Keep the first (highest-ranked) hit per page; count the rest as "more
    // passages".
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deduped: Vec<&fts::WikiPageHit> = Vec::new();
    let mut extra_by_page: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for hit in hits {
        let page_key = hit.parent_slug.clone().unwrap_or_else(|| hit.slug.clone());
        if seen.insert(page_key.clone()) {
            deduped.push(hit);
        } else {
            *extra_by_page.entry(page_key).or_insert(0) += 1;
        }
    }

    let mut out = String::new();
    let mut budget = CONTEXT_CHAR_BUDGET;
    let mut deferred: Vec<&fts::WikiPageHit> = Vec::new();

    for hit in &deduped {
        let entry = format_entry(hit);
        if entry.len() <= budget {
            out.push_str(&entry);
            // Append the "+N more passages" note if this page had extra chunks.
            let page_key = hit.parent_slug.clone().unwrap_or_else(|| hit.slug.clone());
            if let Some(extra) = extra_by_page.get(&page_key) {
                out.push_str(&format!("*(+{extra} more passages from this page)*\n\n"));
            }
            out.push_str("---\n\n");
            budget = budget.saturating_sub(entry.len());
        } else {
            // Include just the title + summary if the full body would overflow.
            let summary_entry = format!(
                "## [[{}]] - {}\n\n{}\n\n*(full body omitted to fit context)*\n\n---\n\n",
                hit.slug, hit.title, hit.summary
            );
            if summary_entry.len() <= budget {
                out.push_str(&summary_entry);
                budget = budget.saturating_sub(summary_entry.len());
            }
            deferred.push(hit);
        }
    }

    if !deferred.is_empty() {
        out.push_str("**Additional relevant pages:** ");
        let slugs: Vec<String> = deferred.iter().map(|h| format!("[[{}]]", h.slug)).collect();
        out.push_str(&slugs.join(", "));
        out.push('\n');
    }

    out
}

/// Format a single hit as a context entry, including the section label when the
/// hit carries chunk metadata.
fn format_entry(hit: &fts::WikiPageHit) -> String {
    let mut s = String::new();
    // Header: include (§Section) when the chunk metadata carries a section.
    if let Some(section) = &hit.section {
        s.push_str(&format!("## [[{}]] - {} (§{section})\n\n", hit.slug, hit.title));
    } else {
        s.push_str(&format!("## [[{}]] - {}\n\n", hit.slug, hit.title));
    }
    if !hit.summary.is_empty() {
        s.push_str(&format!("> {}\n\n", hit.summary));
    }
    s.push_str(&hit.body);
    s.push_str("\n\n");
    s
}

/// Pure helper: estimate the token count of a string (1 token ~= 4 chars).
/// Exported for tests and for the frontend to display budget usage.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wiki::fts::WikiPageHit;

    fn hit(slug: &str, title: &str, summary: &str, body: &str) -> WikiPageHit {
        WikiPageHit {
            slug: slug.to_string(),
            title: title.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            page_type: "concept".to_string(),
            source_articles: "[]".to_string(),
            file_path: format!("wiki/concepts/{slug}.md"),
            rank: -1.0,
            chunk_index: None,
            section: None,
            parent_slug: None,
        }
    }

    /// Build a hit that carries chunk metadata (simulates a chunk row).
    fn chunk_hit(
        slug: &str,
        title: &str,
        summary: &str,
        body: &str,
        section: &str,
        parent_slug: &str,
        chunk_index: i32,
    ) -> WikiPageHit {
        WikiPageHit {
            slug: slug.to_string(),
            title: title.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            page_type: "source".to_string(),
            source_articles: "[]".to_string(),
            file_path: format!("wiki/sources/{parent_slug}.md"),
            rank: -1.0,
            chunk_index: Some(chunk_index),
            section: Some(section.to_string()),
            parent_slug: Some(parent_slug.to_string()),
        }
    }

    #[test]
    fn build_context_empty_hits_returns_empty() {
        assert!(build_context(&[]).is_empty());
    }

    #[test]
    fn build_context_includes_full_body_when_within_budget() {
        let hits = vec![hit("alpha", "Alpha", "alpha summary", "alpha body content")];
        let ctx = build_context(&hits);
        assert!(ctx.contains("[[alpha]]"));
        assert!(ctx.contains("Alpha"));
        assert!(ctx.contains("alpha body content"));
        assert!(ctx.contains("alpha summary"));
    }

    #[test]
    fn build_context_falls_back_to_summary_when_body_too_large() {
        // Create a hit whose body far exceeds the budget.
        let huge_body = "x".repeat(CONTEXT_CHAR_BUDGET + 1000);
        let hits = vec![hit("big", "Big", "big summary", &huge_body)];
        let ctx = build_context(&hits);
        // The full body should NOT be present; the summary should be.
        assert!(ctx.contains("big summary"));
        assert!(ctx.contains("*(full body omitted to fit context)*"));
        assert!(!ctx.contains(&huge_body));
    }

    #[test]
    fn build_context_defers_overflow_hits_to_see_also() {
        // First hit fits; second hit overflows.
        let big_body = "y".repeat(CONTEXT_CHAR_BUDGET);
        let hits = vec![hit("alpha", "Alpha", "", "small"), hit("beta", "Beta", "", &big_body)];
        let ctx = build_context(&hits);
        // Beta should appear in the "Additional relevant pages" line.
        assert!(ctx.contains("Additional relevant pages"));
        assert!(ctx.contains("[[beta]]"));
    }

    #[test]
    fn build_context_respects_budget_order() {
        // Multiple small hits; all should fit.
        let hits = vec![
            hit("alpha", "Alpha", "", "a"),
            hit("beta", "Beta", "", "b"),
            hit("gamma", "Gamma", "", "g"),
        ];
        let ctx = build_context(&hits);
        let alpha_idx = ctx.find("[[alpha]]").unwrap_or(usize::MAX);
        let beta_idx = ctx.find("[[beta]]").unwrap_or(usize::MAX);
        let gamma_idx = ctx.find("[[gamma]]").unwrap_or(usize::MAX);
        // Order preserved (alpha first).
        assert!(alpha_idx < beta_idx);
        assert!(beta_idx < gamma_idx);
    }

    #[test]
    fn estimate_tokens_is_chars_divided_by_four() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn format_entry_includes_slug_title_summary_and_body() {
        let h = hit("alpha", "Alpha", "the summary", "the body");
        let entry = format_entry(&h);
        assert!(entry.contains("[[alpha]]"));
        assert!(entry.contains("Alpha"));
        assert!(entry.contains("the summary"));
        assert!(entry.contains("the body"));
    }

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.to_string(), content: content.to_string() }
    }

    #[test]
    fn wiki_chat_system_prompt_mentions_citations_and_non_invention() {
        let s = wiki_chat_system_prompt();
        assert!(s.contains("research wiki assistant"));
        assert!(s.contains("[[double brackets]]"));
        assert!(s.contains("Do not invent information"));
        assert!(s.contains("Markdown"));
    }

    #[test]
    fn build_context_distinct_pages_not_deduped() {
        // Two chunks with different parent_slugs should both appear. The
        // `[[...]]` link uses `hit.slug` (the chunk row's slug, which for
        // source pages equals the article id); deduping keys on parent_slug.
        let hits = vec![
            chunk_hit(
                "art-1",
                "A",
                "s1",
                "first page methods body content text",
                "Methods",
                "page-a",
                0,
            ),
            chunk_hit(
                "art-2",
                "B",
                "s2",
                "second page results body content text",
                "Results",
                "page-b",
                0,
            ),
        ];
        let ctx = build_context(&hits);
        assert!(ctx.contains("[[art-1]]"), "first chunk slug should appear: {ctx}");
        assert!(ctx.contains("[[art-2]]"), "second chunk slug should appear: {ctx}");
        assert!(!ctx.contains("more passages"), "distinct pages must not trigger dedupe note");
    }

    /// Test D (vertical slice): build a real FTS5 table with chunk rows
    /// (carrying `section`), run `fts::search`, pass the hits to
    /// `build_context`, and assert the context includes the section label
    /// `(§Methods)`. This crosses the fts.rs -> chat.rs boundary that the
    /// manual-hit tests bypass, so it would catch `collect_page_rows` failing
    /// to populate `section` on real FTS rows.
    #[test]
    fn build_context_includes_section_label_from_real_fts_rows() {
        use crate::wiki::fts;
        use rusqlite::Connection;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let dir = root.join("wiki").join("sources");
        std::fs::create_dir_all(&dir).unwrap();
        let mut fm = crate::wiki::frontmatter::Frontmatter::default();
        fm.set("slug", "smith-2023");
        fm.set("title", "Smith 2023");
        fm.set("type", "source");
        fm.set("summary", "summary");
        fm.set("status", "draft");
        fm.set("source_articles", "[]");
        fm.set("links", "[]");
        let methods_sentence = "This study employed a randomised controlled trial design \
            across multiple sites to evaluate the primary outcome measure with covariate \
            adjustment for baseline characteristics and sensitivity analyses.";
        let body = format!(
            "## Methods\n{}\n\n## Results\nThe results showed a significant effect.",
            methods_sentence.repeat(50)
        );
        crate::wiki::frontmatter::write_file(&dir.join("smith-2023.md"), &fm, &body).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        fts::ensure_table(&conn).unwrap();
        fts::rebuild_index(&conn, root).unwrap();

        let hits = fts::search(&conn, "randomised", 10).unwrap();
        assert!(!hits.is_empty(), "should find the Methods chunk in the real FTS index");

        let ctx = build_context(&hits);
        assert!(
            ctx.contains("(§Methods)"),
            "context must include the section label from real FTS rows: {ctx}"
        );
    }

    #[test]
    fn build_wiki_prompts_empty_context_asks_model_to_tell_user_to_ingest() {
        let (_system, user) = build_wiki_prompts("", &[], "anything");
        assert!(user.contains("does not yet contain any indexed pages"));
        assert!(user.contains("ingest sources first"));
        assert!(user.contains("User: anything\nAssistant:"));
    }

    #[test]
    fn build_wiki_prompts_renders_history_in_order() {
        let history = vec![msg("user", "q1"), msg("assistant", "a1"), msg("user", "q2")];
        let (_system, user) = build_wiki_prompts("", &history, "q3");
        assert!(user.contains("Conversation history:"));
        let q1 = user.find("User: q1").unwrap_or(usize::MAX);
        let a1 = user.find("Assistant: a1").unwrap_or(usize::MAX);
        let q2 = user.find("User: q2").unwrap_or(usize::MAX);
        let q3 = user.find("User: q3").unwrap_or(usize::MAX);
        assert!(q1 < a1);
        assert!(a1 < q2);
        assert!(q2 < q3);
    }

    #[test]
    fn build_wiki_prompts_omits_history_section_when_empty() {
        let (_system, user) = build_wiki_prompts("ctx", &[], "q");
        assert!(!user.contains("Conversation history:"));
    }

    #[test]
    fn build_wiki_prompts_treats_unknown_role_as_assistant() {
        let history = vec![msg("system", "sys note")];
        let (_system, user) = build_wiki_prompts("", &history, "q");
        assert!(user.contains("Assistant: sys note"));
        assert!(!user.contains("User: sys note"));
    }

    // ── T1.2 chunk-aware context builder tests ─────────────────────────

    #[test]
    fn build_context_includes_section_label_in_header() {
        let hits = vec![chunk_hit(
            "art-uuid",
            "Smith 2023",
            "summary",
            "We used a randomised controlled design.",
            "Methods",
            "smith-2023",
            0,
        )];
        let ctx = build_context(&hits);
        assert!(ctx.contains("(§Methods)"), "section label must be in header: {ctx}");
        assert!(
            ctx.contains("[[smith-2023]]") || ctx.contains("[[art-uuid]]"),
            "slug must be present"
        );
    }

    #[test]
    fn build_context_dedupes_chunks_of_same_page() {
        let hits = vec![
            chunk_hit(
                "a",
                "A",
                "s1",
                "methods body text here is the first chunk content body",
                "Methods",
                "page-x",
                0,
            ),
            chunk_hit(
                "a",
                "A",
                "s2",
                "results body text here is the second chunk content body",
                "Results",
                "page-x",
                1,
            ),
            chunk_hit(
                "a",
                "A",
                "s3",
                "discussion body text here is the third chunk content body",
                "Discussion",
                "page-x",
                2,
            ),
        ];
        let ctx = build_context(&hits);
        assert!(ctx.contains("+2 more passages from this page"), "should note extra chunks: {ctx}");
        assert!(ctx.contains("(§Methods)"));
    }
}
