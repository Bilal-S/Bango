//! E2E pipeline tests for the Citation Finder (`run_phase_c` + mock senders).
//!
//! Reproduces the user-reported failure: the claim "The soft drinks industry
//! levy has lead to decline in consumption." must surface the SDIL article
//! whose PDF states "The soft drinks industry levy (SDIL) in the United
//! Kingdom has led to a significant reduction in household purchasing of
//! sugar in drinks." - a vocabulary mismatch (`decline`/`consumption` vs
//! `reduction`/`purchasing`) that the pre-2026-09 pipeline dropped silently.
//!
//! The mocks sit at the `CitationLlmSender` seam (recall + classification +
//! claim-split), so everything else - passage gating, finalist pooling,
//! prompt building, grounding, funnel emission - runs for real against a
//! seeded SQLite DB.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bango_lib::citation_finder::search::{run_phase_c, CitationLlmSender};
use bango_lib::citation_finder::{CitationFinderMode, CitationFinderProgress, CitationResult};
use bango_lib::db::chunk_repo;
use bango_lib::db::connection::{create_connection, DbState};
use bango_lib::db::migration::run_migrations;
use bango_lib::embedding::recall::EmbeddingHit;
use bango_lib::error::AppError;
use bango_lib::utils::chunking::Chunk;
use rusqlite::Connection;

const SDIL_TITLE: &str = "Impact of the UK soft drinks industry levy on health and health inequalities in children and adolescents in England: An interrupted time series analysis and population health modelling study";

const SDIL_THESIS: &str = "The soft drinks industry levy (SDIL) in the United Kingdom has led to a significant reduction in household purchasing of sugar in drinks.";

const SDIL_ABSTRACT: &str = "The soft drinks industry levy (SDIL) in the United Kingdom has led to a significant reduction in household purchasing of sugar in drinks. We modelled the impact on health and health inequalities using interrupted time series analysis of household purchasing data. The levy was associated with changes in sugar purchasing across the distribution.";

const SDIL_CLAIM: &str = "The soft drinks industry levy has lead to decline in consumption.";

/// Shared prompt log so tests can assert on the exact classification prompt
/// even though the sender is moved into the `Arc<dyn CitationLlmSender>`.
type PromptLog = Arc<Mutex<Vec<String>>>;

/// Mock sender: scripted recall hits + classification JSON + claim split,
/// capturing every classification prompt for assertions.
struct ScriptedSender {
    hits: Vec<EmbeddingHit>,
    classification_json: String,
    claim_split_json: String,
    prompts: PromptLog,
}

#[async_trait]
impl CitationLlmSender for ScriptedSender {
    async fn send_classification(
        &self,
        _system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AppError> {
        self.prompts.lock().expect("prompt mutex").push(user_prompt.to_string());
        Ok(self.classification_json.clone())
    }

    async fn send_claim_split(&self, _text: &str) -> Result<String, AppError> {
        Ok(self.claim_split_json.clone())
    }

    async fn recall(
        &self,
        _query: &str,
        _top_k: usize,
        _statuses: &[String],
    ) -> Result<Vec<EmbeddingHit>, AppError> {
        Ok(self.hits.clone())
    }
}

fn seed_article(conn: &Connection, id: &str, title: &str, abstract_text: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, ?2, 'Rogers, J.', ?3, 'included', 'test')",
        rusqlite::params![id, title, abstract_text],
    )
    .expect("seed article");
}

fn seed_chunks(conn: &Connection, id: &str, chunks: &[(&str, &str)]) {
    let owned: Vec<Chunk> = chunks
        .iter()
        .enumerate()
        .map(|(i, (text, section))| Chunk {
            section: if section.is_empty() { None } else { Some(section.to_string()) },
            chunk_index: i,
            text: text.to_string(),
            word_count: text.split_whitespace().count(),
        })
        .collect();
    chunk_repo::replace_chunks_for_article(conn, id, &owned).expect("seed chunks");
}

/// Seed the SDIL article exactly as the user reported it: abstract carrying
/// the thesis sentence + full-text chunks (intro states the thesis, methods
/// fragment, and a consumption-terminology discussion chunk).
fn seed_sdil_article(conn: &Connection) {
    seed_article(conn, "sdil", SDIL_TITLE, SDIL_ABSTRACT);
    seed_chunks(
        conn,
        "sdil",
        &[
            (
                &format!("{SDIL_THESIS} Introduction framing of the levy and study aims follow here with additional context words."),
                "Introduction",
            ),
            (
                "We fitted segmented regression models to monthly purchasing series. Covariates included seasonality and market size.",
                "Methods",
            ),
            (
                "Declines in consumption of soft drinks were observed among adolescents. Dietary intake shifted towards water and milk.",
                "Discussion",
            ),
        ],
    );
}

fn hit(article_id: &str, score: f32, chunk_index: Option<i32>) -> EmbeddingHit {
    EmbeddingHit { article_id: article_id.to_string(), score, chunk_index }
}

/// One JSON object for the classification response (callers wrap in an
/// array, which is what `parse_citation_outputs` expects).
fn validating_output(article_id: &str, claim: &str, quotes: &[&str]) -> String {
    serde_json::json!({
        "article_id": article_id,
        "claim": claim,
        "classification": "validating",
        "relevance_explanation": "The passage reports the levy reducing sugar purchasing.",
        "misrepresents_source": false,
        "justifying_sentences": quotes,
    })
    .to_string()
}

fn new_db() -> DbState {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");
    DbState { conn: Mutex::new(conn) }
}

/// Drive `run_phase_c` (whole-block) with a scripted sender; returns
/// (results, classification prompts, progress events).
async fn run_whole_block(
    db: &DbState,
    text: &str,
    sender: ScriptedSender,
) -> (Vec<CitationResult>, Vec<String>, Vec<CitationFinderProgress>) {
    let prompts: PromptLog = Arc::clone(&sender.prompts);
    let events: Arc<Mutex<Vec<CitationFinderProgress>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_closure = Arc::clone(&events);
    let cancel = Arc::new(AtomicBool::new(false));
    let sender: Arc<dyn CitationLlmSender> = Arc::new(sender);
    let results = run_phase_c(
        db,
        text,
        CitationFinderMode::WholeBlock,
        &["included".to_string()],
        &sender,
        &cancel,
        &move |p: CitationFinderProgress| {
            events_for_closure.lock().expect("events mutex").push(p);
        },
    )
    .await
    .expect("run_phase_c");
    let prompts = prompts.lock().expect("prompt mutex").clone();
    let events = events.lock().expect("events mutex").clone();
    (results, prompts, events)
}

/// The user's exact case: the claim's operative words (`decline`,
/// `consumption`, `lead`) do not appear in the SDIL paper (`reduction`,
/// `purchasing`, `led`), yet the article must surface with the thesis
/// sentence as evidence - via the containment-best chunk (the intro chunk
/// carrying the thesis) plus the abstract context reaching the classifier.
#[tokio::test]
async fn sdil_claim_surfaces_with_thesis_evidence() {
    let db = new_db();
    {
        let conn = db.conn.lock().expect("db");
        seed_sdil_article(&conn);
        seed_article(
            &conn,
            "consumption-paper",
            "Trends in soft drinks consumption among adolescents",
            "Soft drinks consumption declined among adolescents over the study period.",
        );
    }

    let sender = ScriptedSender {
        hits: vec![hit("consumption-paper", 0.62, None), hit("sdil", 0.55, Some(0))],
        classification_json: format!(
            "[{},{}]",
            validating_output("sdil", "", &[SDIL_THESIS]),
            validating_output(
                "consumption-paper",
                "",
                &["Soft drinks consumption declined among adolescents over the study period."]
            ),
        ),
        claim_split_json: "[]".to_string(),
        prompts: Arc::new(Mutex::new(Vec::new())),
    };

    let (results, prompts, events) = run_whole_block(&db, SDIL_CLAIM, sender).await;

    // The SDIL article is in the results.
    let matches = &results[0].matches;
    let sdil = matches.iter().find(|m| m.article_id == "sdil").expect("SDIL article must match");

    // The matched passage is the intro chunk carrying the thesis sentence -
    // the exact "supporting statement" the user wanted surfaced.
    assert!(sdil.matched_passage.contains("significant reduction in household purchasing"));

    // The thesis sentence survives the verbatim grounding gate.
    assert!(
        sdil.highlighted_sentences.iter().any(|s| s.contains("significant reduction")),
        "thesis sentence must be a grounded highlight"
    );

    // The abstract context reached the classifier (P1.5).
    let prompt = prompts.first().expect("classification prompt");
    assert!(
        prompt.contains("- abstract (article summary, extra context"),
        "abstract context must be rendered"
    );
    assert!(prompt.contains(SDIL_THESIS), "abstract text must be in the prompt");

    // Funnel transparency (P1.7): final event carries counts.
    let funnel = events.iter().filter_map(|e| e.funnel).next().expect("funnel event");
    assert_eq!(funnel.recalled, 2);
    assert_eq!(funnel.classified, 2);
    assert_eq!(funnel.dropped_unrelated, 0);
}

/// Heavy paraphrase: no query token matches the article lexically (so the
/// containment gate scores 0), but the recall hit's chunk provenance lets
/// the passage layer fall back to the cosine-best chunk (P0.4) and the
/// article still reaches the classifier.
#[tokio::test]
async fn paraphrased_claim_falls_back_to_cosine_chunk() {
    let db = new_db();
    seed_sdil_article(&db.conn.lock().expect("db"));

    let sender = ScriptedSender {
        hits: vec![hit("sdil", 0.71, Some(2))],
        classification_json: format!(
            "[{}]",
            validating_output(
                "sdil",
                "",
                &["Declines in consumption of soft drinks were observed among adolescents."]
            )
        ),
        claim_split_json: "[]".to_string(),
        prompts: Arc::new(Mutex::new(Vec::new())),
    };

    // Zero lexical overlap with the SDIL text (no stemming: "drink" != "drinks").
    let paraphrase = "Sugary drink taxes cut how much households buy.";
    let (results, prompts, _events) = run_whole_block(&db, paraphrase, sender).await;

    let matches = &results[0].matches;
    assert_eq!(matches.len(), 1, "article must survive via the cosine-chunk fallback");
    assert_eq!(matches[0].article_id, "sdil");
    // The fallback passage is the cosine-best chunk (index 2 = Discussion).
    assert!(matches[0].matched_passage.contains("Declines in consumption"));
    // The abstract context still accompanies the chunk passage.
    let prompt = prompts.first().expect("classification prompt");
    assert!(prompt.contains("- abstract (article summary"), "abstract context attached");
}

/// The pre-fix silent black hole: a strict classifier answering `unrelated`
/// previously made the article vanish without a trace. The funnel must now
/// report the drop (P1.7).
#[tokio::test]
async fn unrelated_drop_is_visible_in_funnel() {
    let db = new_db();
    seed_sdil_article(&db.conn.lock().expect("db"));

    let classification = serde_json::json!([{
        "article_id": "sdil",
        "claim": "",
        "classification": "unrelated",
        "relevance_explanation": "The passage concerns purchasing, not consumption.",
        "misrepresents_source": false,
        "justifying_sentences": [],
    }])
    .to_string();

    let sender = ScriptedSender {
        hits: vec![hit("sdil", 0.55, Some(0))],
        classification_json: classification,
        claim_split_json: "[]".to_string(),
        prompts: Arc::new(Mutex::new(Vec::new())),
    };

    let (results, _prompts, events) = run_whole_block(&db, SDIL_CLAIM, sender).await;
    assert!(results[0].matches.is_empty(), "unrelated stays out of the matches");
    let funnel = events.iter().filter_map(|e| e.funnel).next().expect("funnel event");
    assert_eq!(funnel.dropped_unrelated, 1, "the drop must be counted, not silent");
    assert_eq!(funnel.classified, 0);
    let last = events.last().expect("final event");
    assert!(last.message.contains("not related"), "summary message names the drop");
}

/// Finalist-pool union (P0.3): 15 lexically-stronger fillers fill the
/// containment slots; the semantically-strong article (top cosine, weak
/// containment) must be rescued into the LLM prompt.
#[tokio::test]
async fn cosine_union_keeps_semantically_strong_finalist() {
    let db = new_db();
    {
        let conn = db.conn.lock().expect("db");
        for i in 0..15 {
            seed_article(
                &conn,
                &format!("filler-{i}"),
                &format!("Filler study {i} on sugar taxation"),
                "Sugar tax obesity children evidence review of policy effects.",
            );
        }
        // Weak lexical overlap with the claim but the top cosine.
        seed_article(&conn, "sdil", SDIL_TITLE, SDIL_ABSTRACT);
    }

    let mut hits: Vec<EmbeddingHit> =
        (0..15).map(|i| hit(&format!("filler-{i}"), 0.3, None)).collect();
    hits.push(hit("sdil", 0.99, None));

    // Classify ONLY the sdil article as validating (the fillers are noise).
    let sender = ScriptedSender {
        hits,
        classification_json: format!("[{}]", validating_output("sdil", "", &[SDIL_THESIS])),
        claim_split_json: "[]".to_string(),
        prompts: Arc::new(Mutex::new(Vec::new())),
    };

    let claim = "Sugar tax reduces obesity in children.";
    let (results, prompts, _events) = run_whole_block(&db, claim, sender).await;

    // The SDIL article reached the classifier prompt despite ranking 16th by
    // containment (the pre-union pool would have evicted it).
    let prompt = prompts.first().expect("classification prompt");
    assert!(prompt.contains(SDIL_TITLE), "rescued finalist must be in the prompt");
    assert_eq!(results[0].matches.len(), 1);
    assert_eq!(results[0].matches[0].article_id, "sdil");
}

/// Per-statement mode: the splitter's claim flows through recall, the
/// classification is grouped under the claim, and the funnel is emitted.
#[tokio::test]
async fn per_statement_mode_groups_by_claim() {
    let db = new_db();
    seed_sdil_article(&db.conn.lock().expect("db"));

    let events: Arc<Mutex<Vec<CitationFinderProgress>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_closure = Arc::clone(&events);
    let cancel = Arc::new(AtomicBool::new(false));
    let sender: Arc<dyn CitationLlmSender> = Arc::new(ScriptedSender {
        hits: vec![hit("sdil", 0.55, Some(0))],
        classification_json: format!("[{}]", validating_output("sdil", SDIL_CLAIM, &[SDIL_THESIS])),
        claim_split_json: format!("[{SDIL_CLAIM:?}]"),
        prompts: Arc::new(Mutex::new(Vec::new())),
    });

    let results = run_phase_c(
        &db,
        SDIL_CLAIM,
        CitationFinderMode::PerStatement,
        &["included".to_string()],
        &sender,
        &cancel,
        &move |p: CitationFinderProgress| {
            events_for_closure.lock().expect("events mutex").push(p);
        },
    )
    .await
    .expect("run_phase_c");

    assert_eq!(results.len(), 1, "one result group per claim");
    assert_eq!(results[0].claim.as_deref(), Some(SDIL_CLAIM));
    assert_eq!(results[0].matches.len(), 1);
    assert_eq!(results[0].matches[0].article_id, "sdil");
    assert!(
        events.lock().expect("events mutex").iter().any(|e| e.funnel.is_some()),
        "per-statement path also emits the funnel"
    );
}

/// Empty recall reports a zero funnel instead of returning silently.
#[tokio::test]
async fn empty_recall_reports_zero_funnel() {
    let db = new_db();
    seed_sdil_article(&db.conn.lock().expect("db"));

    let sender = ScriptedSender {
        hits: vec![],
        classification_json: "[]".to_string(),
        claim_split_json: "[]".to_string(),
        prompts: Arc::new(Mutex::new(Vec::new())),
    };

    let (results, _prompts, events) = run_whole_block(&db, SDIL_CLAIM, sender).await;
    assert!(results[0].matches.is_empty());
    let funnel = events.iter().filter_map(|e| e.funnel).next().expect("funnel event");
    assert_eq!(funnel.recalled, 0);
    assert_eq!(funnel.finalists, 0);
}
