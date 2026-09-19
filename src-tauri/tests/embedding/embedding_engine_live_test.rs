//! T4/T5 live spike: end-to-end validation of the Bango Local embedding
//! engine (fastembed + ort load-dynamic + EmbeddingGemma 300M Q4 on CPU).
//!
//! Ignored by default - it needs the network + prepared model artifacts
//! (the ONNX Runtime archive downloads itself via the pinned manifest):
//! - `BANGO_EMBED_MODEL_DIR`: directory holding the six pinned model files.
//!
//! Artifact preparation (linux x64 example; every URL is commit-pinned and
//! hash-verified by the test):
//!
//! ```text
//! B=https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/5090578d9565bb06545b4552f76e6bc2c93e4a66
//! mkdir model && cd model
//! curl -sL -o model_q4.onnx           $B/onnx/model_q4.onnx
//! curl -sL -o model_q4.onnx_data      $B/onnx/model_q4.onnx_data
//! curl -sL -o tokenizer.json          $B/tokenizer.json
//! curl -sL -o tokenizer_config.json   $B/tokenizer_config.json
//! curl -sL -o config.json             $B/config.json
//! curl -sL -o special_tokens_map.json $B/special_tokens_map.json
//!
//! BANGO_EMBED_MODEL_DIR=$PWD \
//!   cargo test --test embedding live_embeddinggemma_q4_end_to_end -- --ignored
//! ```
//!
//! The test verifies every file against the embedded manifest's SHA-256
//! pins, then runs the T5 production path (pinned runtime archive download +
//! single-member extraction + `LocalEngine.embed` with NO dylib override),
//! then initializes fastembed directly with the production thread budget and
//! the EmbeddingGemma context, embeds role-prefixed texts, and asserts the
//! 768-dim output plus a retrieval-sanity ordering.
//!
//! Operational app-install check (no network, no temp staging): an ignored
//! fixture generator turns `tests/assets/pone-0285956.pdf` into
//! `tests/assets/pone-0285956-chunks.json` via the production
//! section + chunk pipeline, and a slow-ignored test embeds those
//! chunks through the components the APP downloaded (storage root from
//! `BANGO_STORAGE_ROOT` / the app DB, runtime resolved by the engine) and
//! prints the time each chunk embedding takes.

use std::path::{Path, PathBuf};
use std::time::Instant;

use bango_lib::db::app_settings_repo::{
    get_setting, EMBEDDING_BACKEND_KEY, EMBEDDING_DIMENSIONS_KEY, EMBEDDING_MODEL_KEY,
    EMBEDDING_STATUS_KEY, STORAGE_ROOT_KEY,
};
use bango_lib::embedding::local::download::{
    install_profile, install_runtime, verify_installed, verify_runtime,
};
use bango_lib::embedding::local::engine::{EnginePaths, LocalEngine};
use bango_lib::embedding::local::manifest::local_manifest;
use bango_lib::embedding::local::profile::{
    LOCAL_EMBEDDING_DIMENSIONS, LOCAL_MAX_INPUT_TOKENS, LOCAL_PROFILE_DIR, LOCAL_PROFILE_ID,
};
use bango_lib::embedding::local::prompt::{EmbeddingRole, DOCUMENT_PREFIX, QUERY_PREFIX};
use bango_lib::embedding::local::state::{
    assess_installation, LocalEmbeddingState, INSTALL_MANIFEST_NAME,
};
use bango_lib::embedding::local::thread_budget::embedding_thread_budget;
use bango_lib::embedding::service::probe_local;
use bango_lib::embedding::text::cosine_similarity;
use bango_lib::utils::chunking::{chunk_sections, DEFAULT_CHUNK_WORDS};
use bango_lib::utils::sections::extract_sections;
use fastembed::{InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

fn read(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// ── T8: model acceptance smoke (plan §9) ────────────────────────────────────
//
// One-pass acceptance for `EmbeddingGemma 300M Q4` - NO comparative metrics.
// Fully self-sufficient: downloads BOTH pinned components from the embedded
// manifest (model ~219 MB + runtime archive), verifies them, then embeds a
// synthetic-but-realistic corpus through the PRODUCTION engine
// (`LocalEngine::embed`, no dylib override) and observes, once: install
// time, cold first-call latency (session load + self-test), document
// throughput, warm query latency, and (linux) the peak-RSS delta across the
// load (expected ~= 2x the ~188 MiB Q4 data file: fastembed 7.0.1 external
// initializers are buffer-only).
//
// Assertions are CORRECTNESS only (dimensions, counts, retrieval sanity, and
// a loose lower bound proving the weights are resident); the observations
// print as a summary block for the acceptance record. Cancel/resume/shutdown
// during a large import are exercised on the pending win-x64/osx-arm64
// machines (see the plan's T10 machine-validation checklist).

/// Deterministic synthetic corpus: `count` title+abstract-style documents,
/// each ~120 words, from a fixed research-flavored word pool (LCG-seeded so
/// runs are comparable).
fn synthetic_corpus(count: usize) -> Vec<String> {
    const WORDS: &[&str] = &[
        "sugar",
        "tax",
        "study",
        "children",
        "consumption",
        "policy",
        "effect",
        "analysis",
        "public",
        "health",
        "obesity",
        "reformulation",
        "beverage",
        "industry",
        "evidence",
        "longitudinal",
        "cohort",
        "difference-in-differences",
        "evaluation",
        "outcomes",
        "socioeconomic",
        "gradient",
        "household",
        "panel",
        "data",
        "estimates",
        "model",
        "results",
        "suggest",
        "significant",
        "reduction",
        "purchase",
        "price",
        "elasticity",
        "substitution",
        "untaxed",
        "foods",
        "distribution",
        "impact",
        "manufacturers",
        "report",
        "findings",
        "contribute",
        "literature",
        "natural",
        "experiment",
        "levy",
    ];
    let mut state: u64 = 0x5172_6965_7233_3003;
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 33) as usize
    };
    (0..count)
        .map(|i| {
            let words: Vec<&str> = (0..120).map(|_| WORDS[next() % WORDS.len()]).collect();
            format!("Study {}: {}", i + 1, words.join(" "))
        })
        .collect()
}

/// Peak resident set in bytes (linux `VmHWM`); `None` elsewhere.
fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: u64 = rest.trim().trim_end_matches("kB").trim().parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[test]
#[ignore = "slow"]
fn embeddinggemma_q4_acceptance_smoke() {
    let manifest = local_manifest().expect("embedded manifest");
    let tmp = tempfile::tempdir().expect("tempdir");
    let paths = EnginePaths {
        model_root: tmp.path().join("model"),
        runtime_root: tmp.path().join("runtimes"),
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let progress = |_p| {};

    // 1. End-to-end install of BOTH pinned components (network, ~230 MB).
    let rss_before_load = peak_rss_bytes();
    let t0 = std::time::Instant::now();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime
        .block_on(install_profile(&paths.model_root, &manifest, &progress, &cancel))
        .expect("model profile installs from the pinned manifest");
    runtime
        .block_on(install_runtime(&paths.runtime_root, &manifest, &progress, &cancel))
        .expect("runtime archive installs + extracts");
    let install_secs = t0.elapsed().as_secs_f64();
    assert!(
        verify_installed(&paths.model_root, &manifest).unwrap_or_default().is_empty()
            && verify_runtime(&paths.runtime_root, &manifest).is_ok(),
        "both components verify after the fresh install"
    );

    // 2. Real-corpus embed through the production engine. The FIRST call
    //    pays the session load (file bytes + ort init + warmup self-test).
    let corpus = synthetic_corpus(200);
    let engine = LocalEngine::new();
    let t_cold = std::time::Instant::now();
    let (vectors, dims) = runtime
        .block_on(engine.embed(
            &paths,
            &corpus,
            bango_lib::embedding::local::prompt::EmbeddingRole::Document,
        ))
        .expect("corpus embeds on-device");
    let cold_secs = t_cold.elapsed().as_secs_f64();
    assert_eq!(dims, LOCAL_EMBEDDING_DIMENSIONS as i32);
    assert_eq!(vectors.len(), corpus.len(), "one vector per document");
    assert!(vectors.iter().all(|v| v.len() == LOCAL_EMBEDDING_DIMENSIONS));
    let rss_after_load = peak_rss_bytes();

    let docs_per_sec = corpus.len() as f64 / cold_secs;

    // 3. Warm query latency: 10 single-query calls (retrieval-shaped).
    let queries: Vec<String> = (0..10)
        .map(|i| format!("sugar tax effect on child beverage consumption, study {}", i + 1))
        .collect();
    let mut warm_secs = 0.0f64;
    let mut query_vectors = Vec::with_capacity(queries.len());
    for query in &queries {
        let t = std::time::Instant::now();
        let (v, _) = runtime
            .block_on(engine.embed(
                &paths,
                std::slice::from_ref(query),
                bango_lib::embedding::local::prompt::EmbeddingRole::Query,
            ))
            .expect("query embeds");
        warm_secs += t.elapsed().as_secs_f64();
        query_vectors.push(v.into_iter().next().expect("one vector"));
    }
    let warm_mean_ms = warm_secs / queries.len() as f64 * 1000.0;

    // 4. Retrieval sanity on the corpus (correctness gate).
    let target_sim = cosine_similarity(&query_vectors[0], &vectors[0]);
    let unrelated_sim = cosine_similarity(&query_vectors[0], &vectors[100]);
    assert!(
        target_sim > unrelated_sim || target_sim > 0.5,
        "retrieval sanity: target {target_sim:.4} vs unrelated {unrelated_sim:.4}"
    );

    // 5. Observations (acceptance record - correctness is asserted above;
    //    perf numbers are observed, not gated).
    let load_delta_mb = rss_before_load
        .zip(rss_after_load)
        .map(|(a, b)| (b.saturating_sub(a)) as f64 / 1_048_576.0);
    eprintln!("\n=== EmbeddingGemma 300M Q4 acceptance smoke (plan s9) ===");
    eprintln!("install (model + runtime, network): {install_secs:.1}s");
    eprintln!("cold first call (session load + self-test + 200 docs): {cold_secs:.1}s");
    eprintln!("document throughput (cold, incl. load): {docs_per_sec:.1} docs/s");
    eprintln!("warm single-query mean latency: {warm_mean_ms:.0} ms");
    if let Some(delta) = load_delta_mb {
        eprintln!(
            "peak RSS delta across load: {delta:.0} MiB (expect ~= 2x the ~188 MiB data file)"
        );
        assert!(delta > 150.0, "the Q4 weights must be resident after load (delta {delta:.0} MiB)");
    } else {
        eprintln!("peak RSS delta: not measurable on this platform (linux-only probe)");
    }
    eprintln!("=============================================================\n");
}

#[test]
#[ignore = "slow"]
fn live_embeddinggemma_q4_end_to_end() {
    let dir = PathBuf::from(
        std::env::var("BANGO_EMBED_MODEL_DIR").expect("set BANGO_EMBED_MODEL_DIR to the model dir"),
    );

    // 0. T5 production path: stage the artifacts as a real install, download
    //    the pinned runtime archive via `install_runtime`, and run
    //    `LocalEngine.embed` with NO ORT_DYLIB_PATH override (the engine must
    //    resolve the component-manager library itself). This phase commits
    //    ort's process environment; the direct fastembed phase below reuses
    //    it (same pinned 1.30.0 build).
    {
        let manifest = local_manifest().expect("embedded manifest");
        let tmp = tempfile::tempdir().expect("tempdir");
        let model_root = tmp.path().join("model");
        let profile_dir = model_root.join(LOCAL_PROFILE_DIR);
        std::fs::create_dir_all(&profile_dir).expect("profile dir");
        for file in &manifest.files {
            std::fs::copy(dir.join(&file.name), profile_dir.join(&file.name))
                .unwrap_or_else(|e| panic!("stage {}: {e}", file.name));
        }
        std::fs::write(
            profile_dir.join(INSTALL_MANIFEST_NAME),
            serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
        )
        .expect("write install manifest");
        let runtime_root = tmp.path().join("runtimes");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let report = tokio::runtime::Runtime::new()
            .expect("tokio runtime")
            .block_on(install_runtime(&runtime_root, &manifest, &|_| {}, &cancel))
            .expect("pinned runtime archive downloads + extracts");
        assert!(report.downloaded + report.skipped >= 1, "runtime install ran");

        std::env::remove_var("ORT_DYLIB_PATH");
        let engine = LocalEngine::new();
        let paths = EnginePaths { model_root, runtime_root };
        let (vectors, dims) = tokio::runtime::Runtime::new()
            .expect("tokio runtime")
            .block_on(
                engine.embed(
                    &paths,
                    &[
                        "Which planet is known as the Red Planet?".to_string(),
                        "Mars is often referred to as the Red Planet due to its reddish \
                     appearance caused by iron oxide."
                            .to_string(),
                    ],
                    bango_lib::embedding::local::prompt::EmbeddingRole::Query,
                ),
            )
            .expect("engine embeds via the component-manager runtime");
        assert_eq!(dims, LOCAL_EMBEDDING_DIMENSIONS as i32, "engine dims");
        assert_eq!(vectors.len(), 2, "one vector per input");
        assert_eq!(vectors[0].len(), LOCAL_EMBEDDING_DIMENSIONS, "768-dim output");
    }

    // 1. Every artifact must match the embedded pins before the engine runs.
    let manifest = local_manifest().expect("embedded manifest");
    for file in &manifest.files {
        let bytes = read(&dir.join(&file.name));
        assert_eq!(bytes.len() as u64, file.size, "size pin failed for {}", file.name);
        if let Some(expected) = &file.sha256 {
            let actual = format!("{:x}", Sha256::digest(&bytes));
            assert_eq!(&actual, expected, "sha256 pin failed for {}", file.name);
        }
    }

    // 2. Engine init: dynamic runtime, production thread budget, model ctx.
    //    ort's environment was already committed by the T5 phase above (same
    //    pinned 1.30.0 build), so this direct fastembed section reuses it.
    let threads = embedding_thread_budget(
        std::thread::available_parallelism().map_or(4, std::num::NonZero::get),
    );
    let model = UserDefinedEmbeddingModel::new(
        read(&dir.join("model_q4.onnx")),
        TokenizerFiles {
            tokenizer_file: read(&dir.join("tokenizer.json")),
            config_file: read(&dir.join("config.json")),
            special_tokens_map_file: read(&dir.join("special_tokens_map.json")),
            tokenizer_config_file: read(&dir.join("tokenizer_config.json")),
        },
    )
    .with_external_initializer(
        "model_q4.onnx_data".to_string(),
        read(&dir.join("model_q4.onnx_data")),
    );
    let mut model = TextEmbedding::try_new_from_user_defined(
        model,
        InitOptionsUserDefined::new()
            .with_max_length(LOCAL_MAX_INPUT_TOKENS)
            .with_intra_threads(threads),
    )
    .expect("engine initializes with the Q4 graph (contrib ops) via load-dynamic");

    // 3. Role-prefixed inference + retrieval sanity.
    let query = format!("{QUERY_PREFIX}Which planet is known as the Red Planet?");
    let relevant = format!(
        "{DOCUMENT_PREFIX}Mars is often referred to as the Red Planet due to its reddish \
         appearance caused by iron oxide."
    );
    let unrelated =
        format!("{DOCUMENT_PREFIX}Venus is similar in size to Earth and covered by thick clouds.");
    let out = model.embed(vec![query, relevant, unrelated], Some(2)).expect("inference succeeds");
    assert_eq!(out.len(), 3);
    for vector in &out {
        assert_eq!(vector.len(), LOCAL_EMBEDDING_DIMENSIONS, "768-dim output");
    }
    let score_relevant = cosine_similarity(&out[0], &out[1]);
    let score_unrelated = cosine_similarity(&out[0], &out[2]);
    assert!(
        score_relevant > score_unrelated,
        "retrieval sanity: relevant {score_relevant:.4} must beat unrelated {score_unrelated:.4}"
    );
    assert!(
        score_relevant > 0.3,
        "prefixed query/document similarity should be well above zero: {score_relevant:.4}"
    );
}

// ── Operational check: embed a committed PDF chunk fixture on the app install ──
//
// These two helpers use the REAL app state (installed model + runtime, app DB
// storage root) and no network. The fixture keeps the sample stable across
// runs while the generator keeps it reproducible from the committed PDF.

/// Committed open-access PDF sample (Oakland SSB tax paper, PLOS ONE).
const PDF_ASSET: &str = "../tests/assets/pone-0285956.pdf";

/// Committed chunk fixture derived from [`PDF_ASSET`] via the production
/// section + chunk pipeline.
const CHUNKS_FIXTURE: &str = "../tests/assets/pone-0285956-chunks.json";

/// Minimal `serde` mirror of `utils::chunking::Chunk` (which stays serde-free
/// in production).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureChunk {
    /// Parent section label (`Some("Methods")`); `None` for unlabeled text.
    section: Option<String>,
    chunk_index: usize,
    word_count: usize,
    text: String,
}

/// Fixture envelope: provenance header + the chunk list.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChunkFixture {
    source: String,
    generator: String,
    chunks: Vec<FixtureChunk>,
}

/// Read-only snapshot of the app DB's embedding-relevant settings.
struct AppDbSnapshot {
    path: PathBuf,
    storage_root: Option<String>,
    backend: Option<String>,
    model: Option<String>,
    status: Option<String>,
    dimensions: Option<String>,
}

/// Open the real app DB (`{data_dir}/BonCode.Bango/bango.db`) READ-ONLY and
/// read the settings the test reports. Never migrates or writes (unlike
/// `get_storage_root`); `None` when the DB is absent/unreadable.
fn read_app_db_snapshot() -> Option<AppDbSnapshot> {
    let path = dirs::data_dir()?.join("BonCode.Bango").join("bango.db");
    if !path.is_file() {
        return None;
    }
    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    let get = |key: &str| get_setting(&conn, key).ok().flatten();
    Some(AppDbSnapshot {
        path,
        storage_root: get(STORAGE_ROOT_KEY),
        backend: get(EMBEDDING_BACKEND_KEY),
        model: get(EMBEDDING_MODEL_KEY),
        status: get(EMBEDDING_STATUS_KEY),
        dimensions: get(EMBEDDING_DIMENSIONS_KEY),
    })
}

/// Resolve the storage root the app uses: `BANGO_STORAGE_ROOT` override, else
/// the app DB setting, else the platform default `~/Documents/Bango`
/// (mirrors `app_settings_repo::compute_default_storage_root`).
fn resolve_storage_root(snapshot: Option<&AppDbSnapshot>) -> (PathBuf, String) {
    if let Ok(root) = std::env::var("BANGO_STORAGE_ROOT") {
        if !root.trim().is_empty() {
            return (PathBuf::from(root), "BANGO_STORAGE_ROOT".to_string());
        }
    }
    if let Some(root) =
        snapshot.and_then(|s| s.storage_root.as_deref()).filter(|r| !r.trim().is_empty())
    {
        return (PathBuf::from(root), "app database setting".to_string());
    }
    let docs = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."));
    (docs.join("Bango"), "platform default".to_string())
}

/// Regenerate `tests/assets/pone-0285956-chunks.json` from the committed PDF
/// through the production pipeline (`extract_sections` -> `chunk_sections`).
/// Run when the PDF or the chunker changes:
///
/// ```text
/// cargo test --test embedding generate_pone_chunks_fixture -- --ignored --nocapture
/// ```
#[test]
#[ignore = "fixture generator (rewrites tests/assets/pone-0285956-chunks.json)"]
fn generate_pone_chunks_fixture() {
    let sections = extract_sections(Path::new(PDF_ASSET)).expect("extract PDF sections");
    let chunks = chunk_sections(&sections, DEFAULT_CHUNK_WORDS);
    assert!(!chunks.is_empty(), "the PDF must produce chunks");
    let fixture = ChunkFixture {
        source: "pone-0285956.pdf".to_string(),
        generator: "utils::sections::extract_sections + \
                    utils::chunking::chunk_sections(DEFAULT_CHUNK_WORDS)"
            .to_string(),
        chunks: chunks
            .into_iter()
            .map(|c| FixtureChunk {
                section: c.section,
                chunk_index: c.chunk_index,
                word_count: c.word_count,
                text: c.text,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&fixture).expect("serialize chunk fixture");
    std::fs::write(CHUNKS_FIXTURE, format!("{json}\n")).expect("write chunk fixture");
    eprintln!(
        "wrote {} chunks from {PDF_ASSET} to {CHUNKS_FIXTURE} ({} words)",
        fixture.chunks.len(),
        fixture.chunks.iter().map(|c| c.word_count).sum::<usize>()
    );
}

/// Embed the committed chunk fixture through the installed app components:
/// no network, no temp staging, no `ORT_DYLIB_PATH` (the engine resolves the
/// component-manager runtime itself). Prints the time each chunk embedding
/// takes plus a summary block.
///
/// Requires a healthy Bango Local install (Settings - Embeddings) and exits
/// with an actionable assert otherwise. Storage root resolution: see
/// [`resolve_storage_root`].
///
/// ```text
/// cargo test --test embedding live_embed_chunk_fixture_against_installed_app -- --ignored --nocapture
/// ```
#[test]
#[ignore = "slow"]
fn live_embed_chunk_fixture_against_installed_app() {
    let snapshot = read_app_db_snapshot();
    let (storage_root, root_source) = resolve_storage_root(snapshot.as_ref());
    let paths = EnginePaths::from_storage_root(&storage_root);
    let manifest = local_manifest().expect("embedded manifest");

    eprintln!("\n=== Bango Local: chunk-embedding timings on the app install ===");
    eprintln!("storage root: {} ({root_source})", storage_root.display());
    eprintln!("model root:   {}", paths.model_root.display());
    eprintln!("runtime root: {}", paths.runtime_root.display());
    match &snapshot {
        Some(s) => eprintln!(
            "app DB {}: backend={} model={} status={} dimensions={}",
            s.path.display(),
            s.backend.as_deref().unwrap_or("<unset>"),
            s.model.as_deref().unwrap_or("<unset>"),
            s.status.as_deref().unwrap_or("<unset>"),
            s.dimensions.as_deref().unwrap_or("<unset>")
        ),
        None => eprintln!("app DB not found/unreadable - using the fallback storage root"),
    }

    // No dylib override: the engine must resolve the installed runtime.
    std::env::remove_var("ORT_DYLIB_PATH");
    let state = assess_installation(&paths.model_root, &manifest);
    assert_eq!(
        state,
        LocalEmbeddingState::Ready,
        "local components are not ready ({state:?}) at {} - install/repair in Settings - \
         Embeddings, or set BANGO_STORAGE_ROOT to the right root",
        paths.model_root.display()
    );

    let fixture: ChunkFixture = serde_json::from_str(
        &std::fs::read_to_string(CHUNKS_FIXTURE).expect("read committed chunk fixture"),
    )
    .expect("parse committed chunk fixture");
    assert_eq!(fixture.source, "pone-0285956.pdf");
    assert!(
        fixture.chunks.len() >= 10,
        "fixture should stay a representative sample: {} chunks",
        fixture.chunks.len()
    );
    for (i, chunk) in fixture.chunks.iter().enumerate() {
        assert_eq!(chunk.chunk_index, i, "fixture chunk_index must be contiguous");
        assert_eq!(
            chunk.word_count,
            chunk.text.split_whitespace().count(),
            "fixture chunk {i} word_count must match its text"
        );
        assert!(!chunk.text.trim().is_empty(), "fixture chunk {i} text is empty");
    }
    let total_words: usize = fixture.chunks.iter().map(|c| c.word_count).sum();

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let engine = LocalEngine::new();

    // The app's offline probe: session load + warmup self-test + one embed.
    let probe_start = Instant::now();
    let probe = runtime.block_on(probe_local(&engine, &storage_root));
    let probe_ms = probe_start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(probe.status, "enabled", "local probe: {}", probe.reason);
    assert_eq!(probe.model, LOCAL_PROFILE_ID, "probe model");
    assert_eq!(probe.dimensions, LOCAL_EMBEDDING_DIMENSIONS as i32, "probe dims");
    eprintln!("session load + self-test probe: {probe_ms:.0} ms ({})", probe.reason);

    // Per-chunk timings through the production engine (Document role).
    let mut timings = Vec::with_capacity(fixture.chunks.len());
    eprintln!("\n{:>5} | {:<14} | {:>6} | {:>9} | norm", "chunk", "section", "words", "ms");
    for chunk in &fixture.chunks {
        let start = Instant::now();
        let (vectors, dims) = runtime
            .block_on(engine.embed(
                &paths,
                std::slice::from_ref(&chunk.text),
                EmbeddingRole::Document,
            ))
            .unwrap_or_else(|e| panic!("chunk {} failed to embed: {e}", chunk.chunk_index));
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(dims, LOCAL_EMBEDDING_DIMENSIONS as i32, "engine dims");
        assert_eq!(vectors.len(), 1, "one vector per chunk");
        let vector = &vectors[0];
        assert_eq!(vector.len(), LOCAL_EMBEDDING_DIMENSIONS, "768-dim output");
        assert!(vector.iter().all(|v| v.is_finite()), "finite components");
        let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(norm > 0.0, "non-zero vector");
        eprintln!(
            "{:>5} | {:<14} | {:>6} | {:>9.1} | {norm:.4}",
            chunk.chunk_index,
            chunk.section.as_deref().unwrap_or("-"),
            chunk.word_count,
            ms
        );
        timings.push(ms);
    }

    let total_ms: f64 = timings.iter().sum();
    let mean_ms = total_ms / timings.len() as f64;
    let min_ms = timings.iter().copied().fold(f64::INFINITY, f64::min);
    let max_ms = timings.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let total_secs = total_ms / 1000.0;
    eprintln!("\n--- summary ---");
    eprintln!("chunks embedded: {}", fixture.chunks.len());
    eprintln!("source words:    {total_words}");
    eprintln!("per-chunk:       mean {mean_ms:.1} ms | min {min_ms:.1} ms | max {max_ms:.1} ms");
    eprintln!(
        "total:           {total_ms:.0} ms ({:.1} chunks/s)",
        fixture.chunks.len() as f64 / total_secs
    );
    eprintln!("==============================================================\n");
}

/// App-install regression for the backend-switch session reset
/// (`set_embedding_backend` -> `LocalEngine::reset_off_thread`): the offline
/// probe loads the session, the off-thread reset drops it without blocking
/// the caller, and the next probe lazy-reloads and reports enabled again.
/// Pins the app-freeze regression: the old sync `reset()` under a held DB
/// lock blocked the main thread until an in-flight batch finished.
///
/// Requires a healthy Bango Local install (Settings - Embeddings); storage
/// root resolution: see [`resolve_storage_root`].
///
/// ```text
/// cargo test --test embedding local_engine_reset_off_thread_after_probe -- --ignored --nocapture
/// ```
#[test]
#[ignore = "slow"]
fn local_engine_reset_off_thread_after_probe() {
    let snapshot = read_app_db_snapshot();
    let (storage_root, root_source) = resolve_storage_root(snapshot.as_ref());

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let engine = LocalEngine::new();

    let before = runtime.block_on(probe_local(&engine, &storage_root));
    assert_eq!(before.status, "enabled", "probe before reset: {}", before.reason);
    eprintln!("probe before reset: enabled via {} ({root_source})", storage_root.display());

    let start = Instant::now();
    runtime.block_on(engine.reset_off_thread()).expect("off-thread session reset");
    eprintln!("off-thread reset completed in {:.0} ms", start.elapsed().as_secs_f64() * 1000.0);

    // The next use lazy-reloads the session from the installed profile.
    let after = runtime.block_on(probe_local(&engine, &storage_root));
    assert_eq!(after.status, "enabled", "probe after reset: {}", after.reason);
}
