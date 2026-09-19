//! Backend-aware generation readiness (T6).
//!
//! `has_usable_llm` is the single gate every generation feature (and the
//! embedding director) consults: the stored cloud config under
//! `configured_provider`, or installed Bango AI components under `bango_ai`.
//! The embedding path is deliberately path-aware: chat-backend usability must
//! never green-light the cloud embedding branch when no cloud row exists.

use std::path::Path;

use crate::db::app_settings_repo;
use crate::db::llm_config_repo;
use crate::embedding::backend::EmbeddingBackend;
use crate::error::AppError;
use crate::llm::backend::LlmBackend;
use crate::llm::local::install::{assess_model, assess_runtime};
use crate::llm::local::manifest::local_manifest;
use crate::local_ai::manifest::current_target;
use crate::local_ai::paths::resolve_ai_paths;
use crate::local_ai::state::Assessment;

/// Side-effect-free storage-root read (no migration, no mkdir) for hot paths.
fn storage_root(conn: &rusqlite::Connection) -> Option<String> {
    app_settings_repo::get_setting(conn, app_settings_repo::STORAGE_ROOT_KEY)
        .ok()
        .flatten()
        .filter(|root| !root.trim().is_empty())
}

/// Path-based component check (shared by the conn-based gate and tests).
#[must_use]
pub fn local_components_ready_at(
    paths: &crate::local_ai::paths::AiPaths,
    manifest: &crate::llm::local::manifest::BangoAiManifest,
) -> bool {
    assess_runtime(&paths.runtime_root, manifest) == Assessment::Ready
        && assess_model(&paths.model_root, manifest) == Assessment::Ready
}

/// Whether the Bango AI components are installed on a supported target.
pub fn local_components_ready(conn: &rusqlite::Connection) -> Result<bool, AppError> {
    if current_target().is_none() {
        return Ok(false);
    }
    let Some(root) = storage_root(conn) else {
        return Ok(false);
    };
    let manifest = local_manifest()?;
    let paths = resolve_ai_paths(Path::new(&root));
    Ok(local_components_ready_at(&paths, &manifest))
}

/// Generation usability for the active backend.
pub fn has_usable_llm(conn: &rusqlite::Connection) -> Result<bool, AppError> {
    match app_settings_repo::get_llm_backend(conn)? {
        LlmBackend::BangoAi => local_components_ready(conn),
        LlmBackend::ConfiguredProvider => Ok(llm_config_repo::has_config(conn)?),
    }
}

/// Embedding-generation usability (path-aware; see the module docs).
pub fn embedding_generation_ready(conn: &rusqlite::Connection) -> Result<bool, AppError> {
    match app_settings_repo::get_embedding_backend(conn)? {
        EmbeddingBackend::BangoLocal => {
            Ok(crate::citation_finder::readiness::local_components_ready(conn))
        }
        EmbeddingBackend::ConfiguredProvider => Ok(llm_config_repo::has_config(conn)?),
    }
}
