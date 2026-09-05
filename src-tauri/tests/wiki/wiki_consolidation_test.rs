//! Integration tests for the multi-batch consolidation pipeline.
//!
//! Exercises the full `run_chunked_ingest` flow with a `FakeSender` that
//! simulates the cross-batch duplication problem (two batches independently
//! producing `childhood-obesity` and `obesity-childhood` for the same concept).
//! Verifies the deterministic consolidation pass merges them into one page
//! and rewrites inbound `[[wikilinks]]` to the canonical slug.

use std::sync::Arc;

use async_trait::async_trait;
use bango_lib::error::AppError;
use bango_lib::wiki::ingest::{self, IngestLlmSender, IngestReport, ParsedPage};
use bango_lib::wiki::storage;
use tempfile::TempDir;

/// A fake sender that simulates the cross-batch duplication problem.
///
/// - The batch processing `art-0` emits a page `childhood-obesity` that links
///   to `[[obesity-childhood]]`.
/// - The batch processing `art-1` emits a page `obesity-childhood` that links
///   back to `[[childhood-obesity]]`.
///
/// Without consolidation, this produces two separate files. With consolidation,
/// they merge into one canonical page (`childhood-obesity`, the first
/// encountered) and both inbound links resolve to it.
struct DupSimulatingSender;

#[async_trait]
impl IngestLlmSender for DupSimulatingSender {
    async fn send(&self, prompt: &str) -> Result<String, AppError> {
        // Detect which batch this is by looking at the unique "THIS Batch"
        // source header. Each batch fully processes exactly one source.
        let is_batch_0 = prompt.contains("### Source: Article 0 (slug: art-0)");
        let is_batch_1 = prompt.contains("### Source: Article 1 (slug: art-1)");

        let out = if is_batch_0 {
            // Batch 0: produces childhood-obesity, links to obesity-childhood.
            "<!-- PAGE:childhood-obesity -->\n\
             ---\n\
             id: childhood-obesity\n\
             title: \"Childhood Obesity\"\n\
             type: concept\n\
             slug: childhood-obesity\n\
             summary: \"Obesity in children.\"\n\
             status: draft\n\
             links: [\"[[obesity-childhood]]\"]\n\
             source_articles: [\"art-0\"]\n\
             ---\n\n\
             # Childhood Obesity\n\n\
             A concept backed by [^art-0]. See [[obesity-childhood]].\n"
        } else if is_batch_1 {
            // Batch 1: produces obesity-childhood, links to childhood-obesity.
            "<!-- PAGE:obesity-childhood -->\n\
             ---\n\
             id: obesity-childhood\n\
             title: \"Obesity Childhood\"\n\
             type: concept\n\
             slug: obesity-childhood\n\
             summary: \"Childhood obesity.\"\n\
             status: draft\n\
             links: [\"[[childhood-obesity]]\"]\n\
             source_articles: [\"art-1\"]\n\
             ---\n\n\
             # Obesity Childhood\n\n\
             Same concept from [^art-1], different slug. See [[childhood-obesity]].\n"
        } else {
            // Any other batch: no pages (shouldn't happen in this test).
            ""
        };
        Ok(out.to_string())
    }
}

/// Write a raw source file with the given slug + body.
fn write_source(root: &std::path::Path, slug: &str, title: &str, body: &str) {
    use bango_lib::wiki::frontmatter::{self, Frontmatter};
    std::fs::create_dir_all(root.join("raw")).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", slug);
    fm.set("title", title);
    fm.set("type", "source");
    fm.set("slug", slug);
    fm.set("status", "draft");
    fm.set("summary", "");
    fm.set("links", "[]");
    frontmatter::write_file(&root.join(format!("raw/{slug}.md")), &fm, body).unwrap();
}

/// Build two batches that each fully process one large source. The small
/// context window forces a split so each source lands in its own batch.
fn build_two_batches(root: &std::path::Path) -> Vec<ingest::IngestBatch> {
    std::fs::write(root.join("AGENTS.md"), "# Contract").unwrap();
    write_source(root, "art-0", "Article 0", &"x".repeat(3_000));
    write_source(root, "art-1", "Article 1", &"x".repeat(3_000));
    // Tiny context window -> 2 batches (each source is larger than the budget).
    ingest::build_ingest_prompt_batches(root, 2_000, None, false).unwrap()
}

#[tokio::test]
async fn multi_batch_ingest_consolidates_cross_batch_duplicates() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    storage::scaffold_tree(root).unwrap();

    let batches = build_two_batches(root);
    assert_eq!(batches.len(), 2, "expected exactly 2 batches for this fixture");

    let sender: Arc<dyn IngestLlmSender> = Arc::new(DupSimulatingSender);
    let report =
        ingest::run_chunked_ingest(root, batches, sender, None, (25, 95), None).await.unwrap();

    // Two pages came in; consolidation merged them into one.
    assert_eq!(report.pages_written, 1, "duplicate pages should consolidate to 1");
    assert!(report.errors.is_empty(), "unexpected errors: {:?}", report.errors);

    // The canonical page (childhood-obesity, first encountered) exists.
    let canonical = root.join("wiki/concepts/childhood-obesity.md");
    assert!(canonical.exists(), "canonical page should exist");
    // The duplicate's file does NOT exist (it was merged in-memory before write).
    let duplicate = root.join("wiki/concepts/obesity-childhood.md");
    assert!(!duplicate.exists(), "duplicate slug file should not exist after consolidation");

    // The canonical page body contains both perspectives (the append).
    let full_file = std::fs::read_to_string(&canonical).unwrap();
    assert!(full_file.contains("Childhood Obesity"), "canonical title present");
    assert!(full_file.contains("Additional perspectives"), "merged body appended");
    assert!(full_file.contains("Obesity Childhood"), "duplicate body merged in");

    // Tier A1 grounding contract: the merged body must carry [^art-id] citations
    // for all source articles so the post-ingest lint doesn't flag the page as
    // ungrounded.
    assert!(full_file.contains("[^art-0]"), "merged body must cite art-0");
    assert!(full_file.contains("[^art-1]"), "merged body must cite art-1");

    // Split frontmatter from body: the link rewrite targets the BODY (clickable
    // [[wikilinks]]), not the frontmatter `links:` declaration list (which is a
    // lint hint and is regenerated by the next lint run).
    let body_start = full_file.find("\n---\n").map(|i| i + "\n---\n".len()).unwrap_or(0);
    let body = &full_file[body_start..];

    // The inbound link [[obesity-childhood]] was rewritten to [[childhood-obesity]]
    // in the body (the clickable link), so it should no longer appear as a
    // clickable link in the body text.
    assert!(!body.contains("[[obesity-childhood]]"), "old slug body link should be rewritten away");
    // The canonical self-link should still be present (rewritten to point to itself).
    assert!(
        body.matches("[[childhood-obesity]]").count() >= 1,
        "canonical slug body link should be present"
    );
}

#[tokio::test]
async fn single_batch_ingest_skips_consolidation() {
    // When there is only one batch, no consolidation should run even if the
    // LLM happens to emit two pages with the same slug. This test confirms
    // the single-batch fast path is untouched.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    storage::scaffold_tree(root).unwrap();
    std::fs::write(root.join("AGENTS.md"), "# Contract").unwrap();
    // One small source -> one batch.
    write_source(root, "art-0", "Article 0", "small body");

    let batches = ingest::build_ingest_prompt_batches(root, 50_000, None, false).unwrap();
    assert_eq!(batches.len(), 1, "fixture should produce a single batch");

    // Sender emits two pages with the SAME slug to prove consolidation is
    // skipped (last-write-wins would clobber; consolidation would merge).
    struct SameSlugSender;
    #[async_trait]
    impl IngestLlmSender for SameSlugSender {
        async fn send(&self, _prompt: &str) -> Result<String, AppError> {
            Ok(concat!(
                "<!-- PAGE:alpha -->\n---\nid: alpha\ntitle: \"Alpha\"\ntype: concept\n",
                "slug: alpha\nsummary: \"\"\nstatus: draft\nlinks: []\n",
                "source_articles: [\"art-0\"]\n---\n\n# Alpha v1\n\nBody one.\n",
                "<!-- PAGE:alpha -->\n---\nid: alpha\ntitle: \"Alpha\"\ntype: concept\n",
                "slug: alpha\nsummary: \"\"\nstatus: draft\nlinks: []\n",
                "source_articles: [\"art-0\"]\n---\n\n# Alpha v2\n\nBody two.\n"
            )
            .to_string())
        }
    }

    let sender: Arc<dyn IngestLlmSender> = Arc::new(SameSlugSender);
    let report =
        ingest::run_chunked_ingest(root, batches, sender, None, (25, 95), None).await.unwrap();

    // Single-batch path: both pages "written" (count = 2), last-write-wins on
    // disk. No consolidation happened (otherwise count would be 1).
    assert_eq!(report.pages_written, 2, "single-batch path should not consolidate");
    let body = std::fs::read_to_string(root.join("wiki/concepts/alpha.md")).unwrap();
    // Last write wins: body two is on disk, body one is NOT merged in.
    assert!(body.contains("Body two."));
    assert!(!body.contains("Additional perspectives"));
}

#[tokio::test]
async fn multi_batch_ingest_preserves_unrelated_pages() {
    // Two batches each emit a distinct, unrelated page. Consolidation should
    // NOT merge them (no slug overlap, no shared sources).
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    storage::scaffold_tree(root).unwrap();
    let batches = build_two_batches(root);
    assert_eq!(batches.len(), 2);

    struct DistinctSender;
    #[async_trait]
    impl IngestLlmSender for DistinctSender {
        async fn send(&self, prompt: &str) -> Result<String, AppError> {
            let is_batch_0 = prompt.contains("### Source: Article 0 (slug: art-0)");
            let out = if is_batch_0 {
                "<!-- PAGE:sugar-tax -->\n---\nid: sugar-tax\ntitle: \"Sugar Tax\"\n\
                 type: concept\nslug: sugar-tax\nsummary: \"\"\nstatus: draft\nlinks: []\n\
                 source_articles: [\"art-0\"]\n---\n\n# Sugar Tax\n\nBody.\n"
            } else {
                "<!-- PAGE:exercise -->\n---\nid: exercise\ntitle: \"Exercise\"\n\
                 type: concept\nslug: exercise\nsummary: \"\"\nstatus: draft\nlinks: []\n\
                 source_articles: [\"art-1\"]\n---\n\n# Exercise\n\nBody.\n"
            };
            Ok(out.to_string())
        }
    }

    let sender: Arc<dyn IngestLlmSender> = Arc::new(DistinctSender);
    let report =
        ingest::run_chunked_ingest(root, batches, sender, None, (25, 95), None).await.unwrap();

    // Two unrelated pages, no merges.
    assert_eq!(report.pages_written, 2, "unrelated pages should not merge");
    assert!(root.join("wiki/concepts/sugar-tax.md").exists());
    assert!(root.join("wiki/concepts/exercise.md").exists());
}

#[test]
fn consolidate_pages_merges_shared_sources_in_integration() {
    // Direct unit-style exercise of `consolidate_pages` with the same shape
    // the LLM produces (ParsedPage with frontmatter + body).
    use bango_lib::wiki::frontmatter::Frontmatter;

    fn make_page(slug: &str, sources: &[&str]) -> ParsedPage {
        let mut fm = Frontmatter::default();
        fm.set("id", slug);
        fm.set("title", slug);
        fm.set("type", "concept");
        fm.set("slug", slug);
        fm.set("status", "draft");
        fm.set("summary", "");
        fm.set("links", "[]");
        fm.set(
            "source_articles",
            &format!(
                "[{}]",
                sources.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ")
            ),
        );
        ParsedPage { slug: slug.to_string(), frontmatter: fm, body: format!("# {slug}") }
    }

    // Two pages with different slugs but 2 shared source articles.
    let mut pages = vec![
        make_page("sugar-levy-impact", &["art-1", "art-3"]),
        make_page("ssb-tax-effects", &["art-1", "art-3"]),
    ];
    let slug_map = ingest::consolidate_pages(&mut pages);
    assert_eq!(pages.len(), 1);
    assert_eq!(slug_map.len(), 1);
}

// Reference the IngestReport type so unused-import warnings stay clean when
// the build configuration changes.
#[allow(dead_code)]
fn _ensure_ingest_report_linked(_: IngestReport) {}
