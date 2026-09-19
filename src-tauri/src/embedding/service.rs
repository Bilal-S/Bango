//! Embedding service: the backend router behind embedding generation and
//! recall queries.
//!
//! Two backends: `ConfiguredProvider` delegates to the existing
//! `LlmOrchestrator` path (semaphore + rate limiting + timeout - all LLM
//! calls flow through the orchestrator per `docs/CLAUDE.md`), while
//! `BangoLocal` runs on-device inference through the shared local engine
//! (role prefixes, thread budget, and session lifetime live in
//! `local::engine`). The default backend keeps byte-identical cloud
//! behavior.

use std::path::Path;
use std::sync::Arc;

use crate::embedding::backend::EmbeddingBackend;
use crate::embedding::local::engine::{EnginePaths, LocalEngine};
use crate::embedding::local::manifest::local_manifest;
use crate::embedding::local::profile::{LOCAL_EMBEDDING_DIMENSIONS, LOCAL_PROFILE_ID};
use crate::embedding::local::prompt::EmbeddingRole;
use crate::embedding::local::state::{
    assess_installation, probe_installation_state, LocalEmbeddingState,
};
use crate::error::AppError;
use crate::llm::embedding::ProbeOutcome;
use crate::llm::orchestrator::LlmOrchestrator;
use crate::models::llm_config::LlmConfig;

/// Cloud backend: the configured provider's embedding API via the
/// orchestrator (the pre-local-embeddings behavior, unchanged). A plain
/// struct, not a trait impl - the backend split lives in `EmbeddingService`
/// (query routing) and `EmbeddingBatchSender` (generation + probing), so an
/// `EmbeddingProvider` trait would be a shim nothing consumes.
pub struct CloudEmbeddingProvider {
    orchestrator: Arc<LlmOrchestrator>,
}

impl CloudEmbeddingProvider {
    #[must_use]
    pub fn new(orchestrator: Arc<LlmOrchestrator>) -> Self {
        Self { orchestrator }
    }

    /// Embed retrieval queries or indexed documents (cloud models are
    /// role-symmetric; one method serves both roles).
    pub async fn embed(
        &self,
        config: &LlmConfig,
        texts: &[String],
        model: &str,
    ) -> Result<(Vec<Vec<f32>>, i32), AppError> {
        self.orchestrator.send_embedding(config, texts, model).await
    }
}

/// Router over the two embedding backends.
///
/// Stateless beyond the shared orchestrator + local engine handles: callers
/// resolve `backend` and `storage_root` inside their own brief DB lock
/// burst, and the service never touches the database (lock discipline: no
/// DB lock is held across inference or HTTP).
pub struct EmbeddingService {
    cloud: CloudEmbeddingProvider,
    engine: Arc<LocalEngine>,
}

impl EmbeddingService {
    #[must_use]
    pub fn new(orchestrator: Arc<LlmOrchestrator>, engine: Arc<LocalEngine>) -> Self {
        Self { cloud: CloudEmbeddingProvider::new(orchestrator), engine }
    }

    /// Route one embedding call by backend selection and retrieval role.
    ///
    /// The cloud branch goes through the orchestrator for both roles (cloud
    /// models are role-symmetric). The local branch runs a cheap
    /// state pre-check (fast actionable error without touching the session)
    /// and then the engine.
    pub async fn embed(
        &self,
        backend: EmbeddingBackend,
        storage_root: &Path,
        config: &LlmConfig,
        texts: &[String],
        model: &str,
        role: EmbeddingRole,
    ) -> Result<(Vec<Vec<f32>>, i32), AppError> {
        match backend {
            EmbeddingBackend::ConfiguredProvider => self.cloud.embed(config, texts, model).await,
            EmbeddingBackend::BangoLocal => {
                let paths = local_paths(storage_root)?;
                self.engine.embed(&paths, texts, role).await
            }
        }
    }
}

/// Offline capability probe for the local backend (plan §7): state check,
/// session load (the self-test), one probe embed, then the same outcome
/// shape as the cloud probe. Never touches the cloud provider - the runner,
/// the Test Connection probe, and the generation probe all route here when
/// `bango_local` is selected.
pub async fn probe_local(engine: &LocalEngine, storage_root: &Path) -> ProbeOutcome {
    let disabled = |reason: String| ProbeOutcome {
        status: "disabled".to_string(),
        model: String::new(),
        dimensions: 0,
        reason,
    };
    let paths = match local_paths(storage_root) {
        Ok(paths) => paths,
        Err(_) => {
            return disabled(
                "Bango Local embeddings are not installed. Open Settings - \
                 Embeddings to download the local model."
                    .to_string(),
            );
        }
    };
    let manifest = match local_manifest() {
        Ok(manifest) => manifest,
        Err(e) => return disabled(format!("Bango Local manifest error: {e}")),
    };
    match assess_installation(&paths.model_root, &manifest) {
        LocalEmbeddingState::Ready => {}
        state => {
            return disabled(format!(
                "Bango Local embeddings need repair ({}). Open Settings - Embeddings to \
                 repair the local components.",
                state.not_ready_phrase()
            ));
        }
    }
    match engine.embed(&paths, &["probe".to_string()], EmbeddingRole::Query).await {
        Ok((_, dims)) if dims == LOCAL_EMBEDDING_DIMENSIONS as i32 => ProbeOutcome {
            status: "enabled".to_string(),
            model: LOCAL_PROFILE_ID.to_string(),
            dimensions: dims,
            reason: format!("Embeddings enabled: using {LOCAL_PROFILE_ID} (on-device)."),
        },
        Ok((_, dims)) => disabled(format!(
            "Bango Local self-test returned {dims} dimensions (expected \
             {LOCAL_EMBEDDING_DIMENSIONS})."
        )),
        Err(e) => disabled(format!("Bango Local self-test failed: {e}")),
    }
}

/// Resolve engine paths from the caller-provided storage root, mapping an
/// empty root (side-effect-free read found nothing) to the actionable
/// not-installed error.
fn local_paths(storage_root: &Path) -> Result<EnginePaths, AppError> {
    if storage_root.as_os_str().is_empty() {
        return Err(local_not_installed_error());
    }
    let paths = EnginePaths::from_storage_root(storage_root);
    if probe_installation_state(&paths.model_root)
        != crate::embedding::local::state::LocalEmbeddingState::Ready
    {
        return Err(local_not_installed_error());
    }
    Ok(paths)
}

/// The actionable not-installed error (points the user at Settings or the
/// configured provider instead of presenting as "no results").
fn local_not_installed_error() -> AppError {
    AppError::Validation(
        "Bango Local embeddings are not installed. Open Settings - Embeddings \
         to download the local model, or use your configured provider."
            .to_string(),
    )
}
