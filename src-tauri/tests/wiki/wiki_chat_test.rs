//! Tests for `wiki::chat` (context assembly + prompt building).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` in
//! `src/wiki/chat.rs` to keep the source file compact.

use bango_lib::commands::chat::ChatMessage;
use bango_lib::wiki::chat::{
    build_context, build_wiki_prompts, estimate_tokens, format_entry, wiki_chat_system_prompt,
    CONTEXT_CHAR_BUDGET,
};

use bango_lib::wiki::fts::WikiPageHit;

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
    use bango_lib::wiki::fts;
    use rusqlite::Connection;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let dir = root.join("wiki").join("sources");
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = bango_lib::wiki::frontmatter::Frontmatter::default();
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
    bango_lib::wiki::frontmatter::write_file(&dir.join("smith-2023.md"), &fm, &body).unwrap();

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
    assert!(ctx.contains("[[smith-2023]]") || ctx.contains("[[art-uuid]]"), "slug must be present");
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
