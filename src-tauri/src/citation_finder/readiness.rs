//! Coverage / readiness check for the Citation Finder.
//!
//! Counts articles in filtered statuses vs. articles with ≥1 embedding row
//! of the current model's dimensions. Powers `get_citation_finder_readiness`
//! and the Phase A gate inside `find_citations`.

use std::path::Path;

use rusqlite::{params_from_iter, Connection};

use crate::citation_finder::CitationFinderReadiness;
use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::db::llm_config_repo;
use crate::embedding::backend::EmbeddingBackend;
use crate::embedding::local::manifest::local_manifest;
use crate::embedding::local::paths::resolve_ai_paths;
use crate::embedding::local::state::{assess_installation, LocalEmbeddingState};
use crate::error::AppError;
use crate::llm::embedding::check_embedding_support;

/// Compute the readiness payload for the given status filter.
///
/// `provider_supports_embeddings` is `embedding_status != Disabled`.
/// **Static-override (authoritative, cloud backend only)**: when the
/// configured provider is statically known-unsupported (Anthropic, Z.AI via
/// `check_embedding_support`), the returned `embedding_status` is ALWAYS
/// overridden to `"disabled"` — regardless of the persisted status. This
/// catches un-probed `Unknown`, stale `Enabled` (left over from a previous
/// OpenAI session), and save-debounce timing races. The persisted
/// `app_settings.embedding_status` is NOT mutated here (read-only
/// derivation); the probe + runner read the persisted value directly.
///
/// **Backend-aware (T7)**: the static override applies ONLY to the
/// `configured_provider` backend. With `bango_local` selected, embeddings
/// run on-device, so the chat provider's (lack of an) embedding API is
/// irrelevant; the payload instead carries `local_ready` so the frontend
/// can prompt (Download / Use Configured Provider) when the local
/// components are not installed.
///
/// `coverage_pct` is `embedded_count / total_articles * 100`. Phase A
/// inside `find_citations` runs Phase B when `coverage_pct < 100.0`.
/// Phase B is best-effort — the search proceeds regardless of post-prepare
/// coverage (no 100% gate).
pub fn compute_readiness(
    conn: &Connection,
    status_filter: &[String],
) -> Result<CitationFinderReadiness, AppError> {
    let mut status = app_settings_repo::get_embedding_status(conn)?;
    let dimensions = app_settings_repo::get_embedding_dimensions(conn)?;
    let embedding_model = app_settings_repo::get_embedding_model(conn)?;
    let backend = app_settings_repo::get_embedding_backend(conn)?;

    /* Static-override (authoritative, cloud backend only): when the
    configured provider is statically known-unsupported (Anthropic, Z.AI),
    override the REPORTED status to Disabled regardless of the persisted
    value. The persisted value is NOT mutated — this is a read-only
    derivation for the readiness payload; the probe + runner read the
    persisted value directly. Skipped for `bango_local` (on-device
    embeddings; the chat provider's embedding support is irrelevant). */
    let chat_config = llm_config_repo::get_config(conn)?;
    let chat_provider_supports =
        chat_config.as_ref().is_some_and(|cfg| check_embedding_support(&cfg.provider));
    if backend == EmbeddingBackend::ConfiguredProvider
        && chat_config.is_some()
        && !chat_provider_supports
    {
        status = EmbeddingStatus::Disabled;
    }

    /* L6 (findings-7): for the local backend the Phase A gate is
    authoritative on ACTUAL readiness, not the persisted triple - a fresh
    switch leaves the triple Unknown (the async probe may not have landed
    yet) and a transient past failure can leave it Disabled. `local_ready`
    (healthy components) is the truth for bango_local; the cloud backend
    keeps the static-override + triple semantics. */
    let local_ready = backend == EmbeddingBackend::BangoLocal && local_components_ready(conn);
    let provider_supports = if backend == EmbeddingBackend::BangoLocal {
        local_ready
    } else {
        status != EmbeddingStatus::Disabled
    };

    let total_articles = count_articles_by_status(conn, status_filter)?;
    /* L2 (findings-7): coverage counts rows of the CURRENT model identity
    (when known) so same-dimension models (Google text-embedding-004 vs
    EmbeddingGemma, both 768) never count as covered for each other; a
    backend switch reads as 0% coverage and Phase B regenerates. */
    let model_filter = embedding_model.as_deref().filter(|m| !m.is_empty());
    let embedded_count =
        count_embedded_articles_by_status(conn, dimensions, model_filter, status_filter)?;
    let coverage_pct = coverage_percentage(total_articles, embedded_count);

    Ok(CitationFinderReadiness {
        total_articles,
        embedded_count,
        coverage_pct,
        provider_supports_embeddings: provider_supports,
        statuses: status_filter.to_vec(),
        embedding_status: status.as_str().to_string(),
        embedding_model,
        embedding_backend: backend.as_str().to_string(),
        local_ready,
        chat_provider_supports_embeddings: chat_provider_supports,
    })
}

/// Whether the local embedding components are installed and healthy (model
/// profile `Ready` + runtime library at its pinned size). Cheap filesystem
/// probes; uses the side-effect-free storage-root read so a readiness poll
/// never triggers the lazy migration.
///
/// Deliberate lock-burst exception (findings-6 3.7): these probes run while
/// the caller holds the DB mutex (a few `stat` calls + one ~2 KB manifest
/// parse). Readiness polls on mount, Settings edits, and search submits -
/// never in a loop - so the burst stays brief; restructuring (probing after
/// release) would split the payload construction for no measurable gain.
fn local_components_ready(conn: &Connection) -> bool {
    let storage_root = app_settings_repo::get_setting(conn, app_settings_repo::STORAGE_ROOT_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();
    if storage_root.is_empty() {
        return false;
    }
    let Ok(manifest) = local_manifest() else {
        return false;
    };
    let paths = resolve_ai_paths(Path::new(&storage_root));
    if assess_installation(&paths.model_root, &manifest) != LocalEmbeddingState::Ready {
        return false;
    }
    match (
        crate::embedding::local::download::runtime_lib_file(&paths.runtime_root, &manifest),
        manifest.runtime_file_for_current_target(),
    ) {
        (Some(lib), Some(file)) => {
            crate::embedding::local::download::runtime_library_healthy(&lib, file.lib_size)
        }
        _ => false,
    }
}

/// Pure: `embedded / total * 100`, div-by-zero → 100.0 (empty corpus = full
/// coverage). `#[must_use]`.
#[must_use]
pub fn coverage_percentage(total_articles: i64, embedded_count: i64) -> f64 {
    if total_articles == 0 {
        return 100.0;
    }
    let embedded = embedded_count.clamp(0, total_articles) as f64;
    #[allow(clippy::cast_precision_loss)]
    let total = total_articles as f64;
    (embedded / total) * 100.0
}

/// Count articles matching any of the given statuses. Empty filter = all
/// articles (mirrors `list_for_recall`'s empty-filter contract).
fn count_articles_by_status(conn: &Connection, status_filter: &[String]) -> Result<i64, AppError> {
    let count: i64 = if status_filter.is_empty() {
        conn.query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0))?
    } else {
        let placeholders: Vec<&str> = (0..status_filter.len()).map(|_| "?").collect();
        let in_clause = placeholders.join(", ");
        let sql = format!("SELECT COUNT(*) FROM articles WHERE status IN ({in_clause})");
        let pairs: Vec<&dyn rusqlite::ToSql> =
            status_filter.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        conn.query_row(&sql, params_from_iter(pairs.iter()), |row| row.get(0))?
    };
    Ok(count)
}

/// Count articles matching any of the given statuses that have ≥1 embedding
/// row of the current model's `dimensions`. Empty filter = all statuses.
fn count_embedded_articles_by_status(
    conn: &Connection,
    dimensions: i32,
    model_name: Option<&str>,
    status_filter: &[String],
) -> Result<i64, AppError> {
    let model_filter = model_name.filter(|m| !m.is_empty());
    let count: i64 = if status_filter.is_empty() {
        match model_filter {
            Some(model) => conn.query_row(
                "SELECT COUNT(DISTINCT e.article_id) \
                 FROM article_embeddings e \
                 WHERE e.dimensions = ?1 AND e.model_name = ?2",
                rusqlite::params![dimensions, model],
                |row| row.get(0),
            )?,
            None => conn.query_row(
                "SELECT COUNT(DISTINCT e.article_id) \
                 FROM article_embeddings e \
                 WHERE e.dimensions = ?1",
                rusqlite::params![dimensions],
                |row| row.get(0),
            )?,
        }
    } else {
        // Numbered placeholders + boxed binds (mirrors list_for_recall).
        let mut conditions: Vec<String> = vec!["e.dimensions = ?1".to_string()];
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(dimensions)];
        let mut next_param = 2;
        if let Some(model) = model_filter {
            conditions.push(format!("e.model_name = ?{next_param}"));
            values.push(Box::new(model.to_string()));
            next_param += 1;
        }
        let status_placeholders: Vec<String> = status_filter
            .iter()
            .map(|status| {
                let placeholder = format!("?{next_param}");
                next_param += 1;
                values.push(Box::new(status.clone()));
                placeholder
            })
            .collect();
        conditions.push(format!("a.status IN ({})", status_placeholders.join(", ")));
        let sql = format!(
            "SELECT COUNT(DISTINCT e.article_id) \
             FROM article_embeddings e JOIN articles a ON a.id = e.article_id \
             WHERE {}",
            conditions.join(" AND ")
        );
        conn.query_row(&sql, params_from_iter(values.iter().map(|v| v.as_ref())), |row| row.get(0))?
    };
    Ok(count)
}
// Unit tests live in `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs`
// (extracted per `docs/CLAUDE.md` §Testing). `compute_readiness` is DB-backed
// (covered indirectly via `embedding_recall_multistatus_test`).
