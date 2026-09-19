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

use std::path::PathBuf;

use bango_lib::embedding::local::download::{
    install_profile, install_runtime, verify_installed, verify_runtime,
};
use bango_lib::embedding::local::engine::{EnginePaths, LocalEngine};
use bango_lib::embedding::local::manifest::local_manifest;
use bango_lib::embedding::local::profile::{
    LOCAL_EMBEDDING_DIMENSIONS, LOCAL_MAX_INPUT_TOKENS, LOCAL_PROFILE_DIR,
};
use bango_lib::embedding::local::prompt::{DOCUMENT_PREFIX, QUERY_PREFIX};
use bango_lib::embedding::local::state::INSTALL_MANIFEST_NAME;
use bango_lib::embedding::local::thread_budget::embedding_thread_budget;
use bango_lib::embedding::text::cosine_similarity;
use fastembed::{InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};
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
