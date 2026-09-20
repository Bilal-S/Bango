//! Wiki chat: token-budgeted RAG over the FTS5 index. BM25-search wiki pages, build context
//! respecting a ~3k token budget, send to LLM as `LlmRequestType::WikiChat`. System prompt
//! instructs the model to answer from context, cite by slug, admit gaps.

use std::sync::Arc;

use tauri::State;

use crate::commands::chat::ChatMessage;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::wiki::fts;

/// Approximate char budget for wiki context (1 token ~= 4 chars). ~3k tokens.
pub const CONTEXT_CHAR_BUDGET: usize = 12_000; // ~3000 tokens

/// Max FTS5 hits to consider. 16 since T1.2 (chunk rows are smaller than whole-page rows).
/// `build_context` dedupes by `parent_slug` so one paper doesn't crowd out others.
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
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        crate::wiki::storage::resolve_root(&conn)?
    };

    /* Self-heal: BM25 search for relevant wiki pages. `ensure_index_populated`
    recovers from desync where pages exist on disk but FTS table is empty
    (e.g. after schema rebuild/DB reset that dropped table). */
    let hits = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        fts::ensure_index_populated(&conn, &root)?;
        fts::search(&conn, question, MAX_HITS)?
    };

    // 3. Build the token-budgeted context from the hits.
    let context = build_context(&hits);

    // 4. Load the LLM config.
    let config = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        crate::llm::effective_config::resolve(&conn)?.ok_or_else(|| {
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

/// Wiki-chat system prompt. Exposed for tests alongside the prompt contract.
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

/// Build a token-budgeted context string from BM25 hits. Dedupes by `parent_slug` (T1.2):
/// keeps the top-ranked chunk, appends "(+N more passages)". Includes §Section label in header.
/// Higher-ranked hits first; over-budget hits are "see also" with summary-only fallback.
pub fn build_context(hits: &[fts::WikiPageHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }

    /* Dedupe by parent_slug (falls back to slug for legacy whole-page rows).
    Keep the first (highest-ranked) hit per page; count rest as "more passages". */
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

/// Format a single hit as a context entry, with §Section label when chunk metadata is present.
pub fn format_entry(hit: &fts::WikiPageHit) -> String {
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
