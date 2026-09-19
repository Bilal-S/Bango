//! The Bango Local embedding engine: a single long-lived fastembed session
//! executing on the blocking pool behind a mutex.
//!
//! Concurrency model: `TextEmbedding::embed` takes `&mut self`, so the
//! session lives in a `Mutex<Option<EngineSession>>` held through an `Arc`.
//! Every embed call runs entirely inside ONE `spawn_blocking` closure that
//! locks the mutex, loads the session on first use (seconds), and runs
//! inference - async workers never block and no lock is ever held across an
//! `.await`. Concurrent callers serialize on the session by design (a
//! single ORT intra-op pool is the parallelism axis for this GEMM-bound
//! workload).
//!
//! Runtime loading: the ONNX Runtime dylib is resolved from the component
//! manager's install (`{runtime_root}/onnxruntime/<version>/<lib>`) unless
//! `ORT_DYLIB_PATH` overrides it (dev/live-test escape hatch). ort's
//! process-global environment is committed exactly once via `init_from`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use fastembed::{InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};

use crate::embedding::local::manifest::local_manifest;
use crate::embedding::local::profile::{
    LOCAL_EMBEDDING_DIMENSIONS, LOCAL_MAX_INPUT_TOKENS, LOCAL_PROFILE_DIR,
};
use crate::embedding::local::prompt::{apply_role_prefix, EmbeddingRole};
use crate::embedding::local::state::{assess_installation, LocalEmbeddingState};
use crate::embedding::local::thread_budget::embedding_thread_budget;
use crate::error::AppError;
use crate::local_ai::paths::resolve_ai_paths;

/// Explicit inference batch size (never fastembed's default 256 - a 300M
/// model at ~700-token inputs would spike memory).
const ENGINE_BATCH_SIZE: usize = 8;

/// Resolved artifact roots the engine loads from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnginePaths {
    pub model_root: PathBuf,
    pub runtime_root: PathBuf,
}

impl EnginePaths {
    /// Resolve from a storage root (OneDrive-aware).
    #[must_use]
    pub fn from_storage_root(storage_root: &Path) -> Self {
        let paths = resolve_ai_paths(storage_root);
        Self { model_root: paths.model_root, runtime_root: paths.runtime_root }
    }

    /// The installed profile directory.
    #[must_use]
    pub fn profile_dir(&self) -> PathBuf {
        self.model_root.join(LOCAL_PROFILE_DIR)
    }
}

/// A loaded engine session.
struct EngineSession {
    model: TextEmbedding,
}

/// The shared, lazily-loaded local engine (managed Tauri state).
#[derive(Default)]
pub struct LocalEngine {
    session: Arc<Mutex<Option<EngineSession>>>,
    /// Live `embed` calls, logged at reset to expose switch-vs-embed races.
    embeds_in_flight: Arc<AtomicUsize>,
}

/// Decrements the in-flight embed counter on every exit path (error/panic).
struct InFlight<'a>(&'a AtomicUsize);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl LocalEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Embed `texts` with the profile's role prefixes applied, loading the
    /// session on first use. Returns one vector per input plus the
    /// effective dimensionality (768).
    pub async fn embed(
        &self,
        paths: &EnginePaths,
        texts: &[String],
        role: EmbeddingRole,
    ) -> Result<(Vec<Vec<f32>>, i32), AppError> {
        if texts.is_empty() {
            return Err(AppError::Validation("no texts to embed".to_string()));
        }
        let session = Arc::clone(&self.session);
        let in_flight = Arc::clone(&self.embeds_in_flight);
        let paths = paths.clone();
        let texts = texts.to_vec();
        tokio::task::spawn_blocking(move || -> Result<(Vec<Vec<f32>>, i32), AppError> {
            let text_count = texts.len();
            let before = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            let _in_flight = InFlight(&in_flight);
            let started = Instant::now();
            eprintln!("[embedding] engine embed begin: {text_count} text(s), {before} in flight");
            let result = (|| -> Result<(Vec<Vec<f32>>, i32), AppError> {
                let mut guard = session
                    .lock()
                    .map_err(|e| AppError::LockPoisoned(format!("local engine session: {e}")))?;
                if guard.is_none() {
                    let load_started = Instant::now();
                    eprintln!("[embedding] engine session load begin");
                    *guard = Some(load_session(&paths)?);
                    eprintln!(
                        "[embedding] engine session load end: {} ms",
                        load_started.elapsed().as_millis()
                    );
                }
                let engine = guard
                    .as_mut()
                    .ok_or_else(|| AppError::Import("engine session unavailable".to_string()))?;
                embed_sync(&mut engine.model, &texts, role)
            })();
            eprintln!(
                "[embedding] engine embed end: {} ms, ok={} ({before} in flight before)",
                started.elapsed().as_millis(),
                result.is_ok()
            );
            result
        })
        .await
        .map_err(|e| AppError::Import(format!("engine task panicked: {e}")))?
    }

    /// Drop the loaded session (e.g. after a backend switch, Remove, or a
    /// Repair install; the next call lazy-reloads). `take()` under the brief
    /// lock, then destroy the `EngineSession` (ORT teardown: intra-op thread
    /// pool join + arena free) inside `spawn_blocking`.
    ///
    /// Regression note: this used to be a sync `reset()` that dropped the
    /// session inline under the lock. Called from the then-synchronous
    /// `set_embedding_backend` command it froze the whole app: the command
    /// ran on the main thread holding the DB mutex while waiting out an
    /// in-flight embed batch (the session mutex is held per batch), then
    /// tore the ORT session down on the main thread. Callers must keep both
    /// the wait and the teardown off the main thread and out of any DB-lock
    /// scope.
    pub async fn reset_off_thread(&self) -> Result<(), AppError> {
        let in_flight = self.embeds_in_flight.load(Ordering::SeqCst);
        let started = Instant::now();
        eprintln!("[embedding] engine reset begin: {in_flight} embed(s) in flight");
        let session = self
            .session
            .lock()
            .map_err(|e| AppError::LockPoisoned(format!("local engine session: {e}")))?
            .take();
        eprintln!(
            "[embedding] engine reset locked after {} ms: session_present={}",
            started.elapsed().as_millis(),
            session.is_some()
        );
        if session.is_none() {
            return Ok(());
        }
        let teardown = Instant::now();
        tokio::task::spawn_blocking(move || drop(session))
            .await
            .map_err(|e| AppError::Import(format!("engine reset task panicked: {e}")))?;
        eprintln!("[embedding] engine reset teardown end: {} ms", teardown.elapsed().as_millis());
        Ok(())
    }
}

/// Resolve the ONNX Runtime dylib: `ORT_DYLIB_PATH` when set (dev/live
/// test), else the component-manager install (which must exist and match
/// its pinned size), else None when no runtime is pinned for this target.
fn resolve_dylib(
    env_override: Option<&str>,
    paths: &EnginePaths,
) -> Result<Option<PathBuf>, AppError> {
    if let Some(env) = env_override.filter(|e| !e.trim().is_empty()) {
        return Ok(Some(PathBuf::from(env)));
    }
    let manifest = local_manifest()?;
    // L5 (findings-7): no pinned runtime for this target = unsupported
    // machine. Error with the actionable message instead of Ok(None), which
    // used to make `ensure_ort_environment` latch success and surface a
    // generic fastembed failure on the first real embed.
    let Some(file) = manifest.runtime_file_for_current_target() else {
        return Err(AppError::Validation(
            "Bango Local is not available for this system. You can continue using your \
             configured embedding provider."
                .to_string(),
        ));
    };
    match crate::embedding::local::download::runtime_lib_file(&paths.runtime_root, &manifest) {
        Some(candidate)
            if crate::embedding::local::download::runtime_library_healthy(
                &candidate,
                file.lib_size,
            ) =>
        {
            Ok(Some(candidate))
        }
        // Unreachable in practice (a pinned runtime file implies a path), but
        // treated like a damaged install rather than silently succeeding.
        None => Err(AppError::Validation(
            "Bango Local's ONNX Runtime is not installed. Open Settings - Embeddings to \
             download the local components."
                .to_string(),
        )),
        Some(_) => Err(AppError::Validation(
            "Bango Local's ONNX Runtime is damaged or incomplete. Open Settings - \
             Embeddings to re-install the local components."
                .to_string(),
        )),
    }
}

/// Commit ort's process-global environment exactly once. A committed
/// environment cannot be reconfigured (a failed load stays failed until
/// process restart - surfaced as an error for the repair path).
fn ensure_ort_environment(paths: &EnginePaths) -> Result<(), AppError> {
    static INITIALIZED: OnceLock<()> = OnceLock::new();
    if INITIALIZED.get().is_some() {
        eprintln!("[embedding] ort environment already committed");
        return Ok(());
    }
    let env_override = std::env::var("ORT_DYLIB_PATH").ok();
    let dylib = resolve_dylib(env_override.as_deref(), paths)?;
    eprintln!("[embedding] ort environment resolving dylib: {dylib:?}");
    if let Some(path) = dylib {
        // Under load-dynamic, `commit()` returns a success bool.
        let builder = ort::init_from(&path)
            .map_err(|e| AppError::Import(format!("ONNX Runtime failed to load: {e}")))?;
        if !builder.commit() {
            return Err(AppError::Import(
                "ONNX Runtime environment commit failed (already initialized?)".to_string(),
            ));
        }
        eprintln!("[embedding] ort environment committed");
    }
    let _ = INITIALIZED.set(());
    Ok(())
}

/// Load a session from the installed profile: state gate, ort environment,
/// file bytes (the Q4 data file as an external initializer), thread budget,
/// and a warmup embed as the plan's self-test.
fn load_session(paths: &EnginePaths) -> Result<EngineSession, AppError> {
    let started = Instant::now();
    eprintln!("[embedding] engine session state gate begin: {}", paths.model_root.display());
    let manifest = local_manifest()?;
    match assess_installation(&paths.model_root, &manifest) {
        LocalEmbeddingState::Ready => {}
        state => {
            return Err(AppError::Validation(format!(
                "Bango Local embeddings are not ready ({}). Open Settings - Embeddings \
                 to install or repair the local components.",
                state.not_ready_phrase()
            )));
        }
    }
    eprintln!("[embedding] engine session state gate: ready");
    ensure_ort_environment(paths)?;
    let profile_dir = paths.profile_dir();
    let read = |name: &str| -> Result<Vec<u8>, AppError> {
        std::fs::read(profile_dir.join(name)).map_err(|e| {
            AppError::Import(format!("cannot read installed model file '{name}': {e}"))
        })
    };
    let model = UserDefinedEmbeddingModel::new(
        read("model_q4.onnx")?,
        TokenizerFiles {
            tokenizer_file: read("tokenizer.json")?,
            config_file: read("config.json")?,
            special_tokens_map_file: read("special_tokens_map.json")?,
            tokenizer_config_file: read("tokenizer_config.json")?,
        },
    )
    .with_external_initializer("model_q4.onnx_data".to_string(), read("model_q4.onnx_data")?);
    let threads = embedding_thread_budget(
        std::thread::available_parallelism().map_or(4, std::num::NonZero::get),
    );
    let mut model = TextEmbedding::try_new_from_user_defined(
        model,
        InitOptionsUserDefined::new()
            .with_max_length(LOCAL_MAX_INPUT_TOKENS)
            .with_intra_threads(threads),
    )
    .map_err(|e| AppError::Import(format!("local embedding model failed to load: {e}")))?;
    eprintln!(
        "[embedding] engine session built in {} ms (threads={threads})",
        started.elapsed().as_millis()
    );
    // Self-test (plan §5): a warmup embed proves the session actually runs
    // before the first real query pays the discovery cost.
    let warmup = apply_role_prefix("warmup", EmbeddingRole::Query);
    model
        .embed(vec![warmup], Some(1))
        .map_err(|e| AppError::Import(format!("local embedding self-test failed: {e}")))?;
    eprintln!("[embedding] engine session warmup ok: {} ms total", started.elapsed().as_millis());
    Ok(EngineSession { model })
}

/// Synchronous embed with role prefixes and explicit batching.
fn embed_sync(
    model: &mut TextEmbedding,
    texts: &[String],
    role: EmbeddingRole,
) -> Result<(Vec<Vec<f32>>, i32), AppError> {
    let prefixed: Vec<String> = texts.iter().map(|t| apply_role_prefix(t, role)).collect();
    let vectors = model
        .embed(prefixed, Some(ENGINE_BATCH_SIZE))
        .map_err(|e| AppError::Import(format!("local embedding inference failed: {e}")))?;
    validate_vectors(&vectors, texts.len())?;
    Ok((vectors, LOCAL_EMBEDDING_DIMENSIONS as i32))
}

/// Pure output validation (unit-tested without a model): one vector per
/// input, each at the profile's dimensionality.
pub(crate) fn validate_vectors(
    vectors: &[Vec<f32>],
    expected_count: usize,
) -> Result<(), AppError> {
    if vectors.len() != expected_count {
        return Err(AppError::Import(format!(
            "local embedding returned {} vectors for {} inputs",
            vectors.len(),
            expected_count
        )));
    }
    for vector in vectors {
        if vector.len() != LOCAL_EMBEDDING_DIMENSIONS {
            return Err(AppError::Import(format!(
                "local embedding returned {} dimensions, expected {LOCAL_EMBEDDING_DIMENSIONS}",
                vector.len()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::local::manifest::ComponentManifest;

    #[test]
    fn engine_validate_vectors_accepts_wellformed_output() {
        let ok = vec![vec![0.1; LOCAL_EMBEDDING_DIMENSIONS], vec![0.2; LOCAL_EMBEDDING_DIMENSIONS]];
        assert!(validate_vectors(&ok, 2).is_ok());
        assert!(validate_vectors(&[], 0).is_ok(), "an empty batch is valid");
    }

    #[test]
    fn engine_validate_vectors_rejects_wrong_count_and_dims() {
        let one = vec![vec![0.1; LOCAL_EMBEDDING_DIMENSIONS]];
        let err = validate_vectors(&one, 2).expect_err("count mismatch rejected");
        assert!(err.to_string().contains("2 inputs"), "got: {err}");

        let bad_dims = vec![vec![0.1; 4]];
        let err = validate_vectors(&bad_dims, 1).expect_err("dimension mismatch rejected");
        assert!(err.to_string().contains("dimensions"), "got: {err}");
    }

    #[tokio::test]
    async fn engine_reset_off_thread_without_a_session_is_a_noop() {
        let engine = LocalEngine::new();
        // Nothing loaded: Ok without spawning, and no session appears.
        engine.reset_off_thread().await.expect("empty reset ok");
        let guard = engine.session.lock().expect("session lock");
        assert!(guard.is_none(), "no session appears after an empty reset");
    }

    #[test]
    fn engine_resolve_dylib_env_override_wins() {
        let paths = EnginePaths {
            model_root: PathBuf::from("/nonexistent-model"),
            runtime_root: PathBuf::from("/nonexistent-runtime"),
        };
        let resolved =
            resolve_dylib(Some("/custom/libonnxruntime.so"), &paths).expect("override resolves");
        assert_eq!(resolved, Some(PathBuf::from("/custom/libonnxruntime.so")));
        // A whitespace-only override is treated as unset: resolution falls
        // through to the (missing) component-manager install and errors with
        // the actionable message instead of using the bogus override.
        let err = resolve_dylib(Some("   "), &paths).expect_err("empty override ignored");
        assert!(err.to_string().contains("Settings"), "got: {err}");
    }

    #[test]
    fn engine_resolve_dylib_requires_healthy_install() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = EnginePaths {
            model_root: dir.path().join("model"),
            runtime_root: dir.path().join("runtimes"),
        };
        // Nothing installed: the current target's runtime is pinned, so the
        // engine must refuse with the actionable re-install error.
        if ComponentManifest::current_target().is_some() {
            let err = resolve_dylib(None, &paths).expect_err("missing runtime rejected");
            assert!(err.to_string().contains("Settings"), "got: {err}");
        }

        // A healthy library (pinned size, created sparse) resolves; a
        // truncated one is refused with the actionable repair error.
        let manifest = local_manifest().expect("manifest");
        if let Some(file) = manifest.runtime_file_for_current_target() {
            let lib =
                crate::embedding::local::download::runtime_lib_file(&paths.runtime_root, &manifest)
                    .expect("lib path");
            if let Some(parent) = lib.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::File::create(&lib).unwrap().set_len(file.lib_size).unwrap();
            let resolved = resolve_dylib(None, &paths).expect("healthy runtime resolves");
            assert_eq!(resolved.as_deref(), Some(lib.as_path()));

            std::fs::File::create(&lib).unwrap().set_len(8).unwrap();
            let err = resolve_dylib(None, &paths).expect_err("truncated runtime rejected");
            // Assert the full cleaned message: the damaged-runtime error must
            // read as one sentence (regression: a literal `\n` once rendered
            // as backslash-n plus padding spaces).
            assert!(
                err.to_string()
                    .contains("Open Settings - Embeddings to re-install the local components."),
                "got: {err}"
            );
        }
    }
}
