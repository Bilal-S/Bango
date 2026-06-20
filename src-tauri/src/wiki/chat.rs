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
const MAX_HITS: usize = 8;

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
     when the answer draws on a specific page. Do not invent information. If the wiki \
     context does not cover the question, say so explicitly and suggest which page \
     might need to be created or expanded. Format your response in clean Markdown."
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
/// Higher-ranked hits are included first; once the char budget is exhausted,
/// remaining hits are skipped (their titles are still listed as "see also").
fn build_context(hits: &[fts::WikiPageHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut budget = CONTEXT_CHAR_BUDGET;
    let mut deferred: Vec<&fts::WikiPageHit> = Vec::new();

    for hit in hits {
        let entry = format_entry(hit);
        if entry.len() <= budget {
            out.push_str(&entry);
            out.push_str("\n---\n\n");
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

/// Format a single hit as a context entry.
fn format_entry(hit: &fts::WikiPageHit) -> String {
    let mut s = String::new();
    s.push_str(&format!("## [[{}]] - {}\n\n", hit.slug, hit.title));
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
    fn build_wiki_prompts_with_context_includes_context_block_and_question() {
        let ctx = "## [[alpha]] - Alpha\n\nbody text";
        let (system, user) = build_wiki_prompts(ctx, &[], "What is alpha?");
        // System prompt is the static contract.
        assert_eq!(system, wiki_chat_system_prompt());
        // The provided context is embedded.
        assert!(user.contains("Wiki page context (BM25-ranked"));
        assert!(user.contains("[[alpha]]"));
        assert!(user.contains("body text"));
        // The "ingest first" fallback must NOT appear when context is present.
        assert!(!user.contains("ingest sources first"));
        // Final line carries the question.
        assert!(user.ends_with("User: What is alpha?\nAssistant:"));
    }

    #[test]
    fn build_wiki_prompts_empty_context_asks_model_to_tell_user_to_ingest() {
        let (_system, user) = build_wiki_prompts("", &[], "anything");
        assert!(user.contains("does not yet contain any indexed pages"));
        assert!(user.contains("ingest sources first"));
        // Still ends with the question prompt.
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
        // History precedes the final question, in order.
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
        // Any role string other than exactly "user" renders as "Assistant".
        let history = vec![msg("system", "sys note")];
        let (_system, user) = build_wiki_prompts("", &history, "q");
        assert!(user.contains("Assistant: sys note"));
        assert!(!user.contains("User: sys note"));
    }
}
