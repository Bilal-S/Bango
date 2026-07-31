//! Coverage / readiness check for the Citation Finder (`citation_finder/AGENTS.md`).
//!
//! Counts articles in the filtered statuses vs. articles with ≥1 embedding row
//! of the current model's dimensions. Powers:
//! - `get_citation_finder_readiness` command (toggle visibility + tooltip hint)
//! - The Phase A gate inside `find_citations` (decides whether Phase B runs)

use rusqlite::{params_from_iter, Connection};

use crate::citation_finder::CitationFinderReadiness;
use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::error::AppError;

/// Compute the readiness payload for the given status filter.
///
/// `provider_supports_embeddings` is `embedding_status != Disabled`
/// (cf2.md §2.1): the toggle is hidden only on a *known-unsupported* provider
/// (Anthropic, Z.AI). `Unknown` shows the toggle — Phase B's first run probes
/// via `generate_embeddings_inner` and resolves it to `Enabled`/`Disabled`.
/// `Enabled` shows the toggle (unchanged). `dimensions` is still loaded so
/// `coverage_pct` can filter same-dimension rows; it is NOT part of the
/// toggle-visibility gate (a probe has not necessarily run yet when the toggle
/// is first shown).
///
/// `coverage_pct` is `embedded_count / total_articles * 100`. The Phase A
/// check inside `find_citations` runs Phase B (prepare) when `coverage_pct <
/// 100.0`. Phase B is best-effort — the search proceeds regardless of the
/// post-prepare coverage (no 100% gate); see `search.rs` + the module
/// `AGENTS.md` for why partial coverage is tolerated.
pub fn compute_readiness(
    conn: &Connection,
    status_filter: &[String],
) -> Result<CitationFinderReadiness, AppError> {
    let status = app_settings_repo::get_embedding_status(conn)?;
    let dimensions = app_settings_repo::get_embedding_dimensions(conn)?;
    let provider_supports = status != EmbeddingStatus::Disabled;

    let total_articles = count_articles_by_status(conn, status_filter)?;
    let embedded_count = count_embedded_articles_by_status(conn, dimensions, status_filter)?;
    let coverage_pct = coverage_percentage(total_articles, embedded_count);

    Ok(CitationFinderReadiness {
        total_articles,
        embedded_count,
        coverage_pct,
        provider_supports_embeddings: provider_supports,
        statuses: status_filter.to_vec(),
    })
}

/// Pure percentage helper: `embedded / total * 100`, with division-by-zero →
/// 100.0 (an empty corpus trivially has full coverage).
///
/// `#[must_use]` so the boundary cases (empty corpus, partial, full) are
/// unit-testable in isolation.
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
    status_filter: &[String],
) -> Result<i64, AppError> {
    let count: i64 = if status_filter.is_empty() {
        conn.query_row(
            "SELECT COUNT(DISTINCT e.article_id) \
             FROM article_embeddings e \
             WHERE e.dimensions = ?1",
            rusqlite::params![dimensions],
            |row| row.get(0),
        )?
    } else {
        let placeholders: Vec<&str> = (0..status_filter.len()).map(|_| "?").collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT COUNT(DISTINCT e.article_id) \
             FROM article_embeddings e JOIN articles a ON a.id = e.article_id \
             WHERE e.dimensions = ?1 AND a.status IN ({in_clause})"
        );
        let mut pairs: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(1 + status_filter.len());
        pairs.push(Box::new(dimensions));
        for s in status_filter {
            pairs.push(Box::new(s.clone()));
        }
        conn.query_row(&sql, params_from_iter(pairs.iter()), |row| row.get(0))?
    };
    Ok(count)
}
// Unit tests live in `src-tauri/tests/citation_finder_readiness_test.rs`
// (extracted per `docs/CLAUDE.md` §Testing). `compute_readiness` is DB-backed
// (covered indirectly via `embedding_recall_multistatus_test`).
