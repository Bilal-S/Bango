//! Tauri commands for the Zotero import wizard: connection check, collection
//! listing, and the collection preview (the standard review step's data
//! source). All HTTP goes through `zotero::client`. The `_inner` variants
//! take the base URL so integration tests can point them at a mockito server.

use serde::Serialize;
use tauri::State;

use crate::commands::import::{ImportError, ImportPreview, ImportResult, PreviewArticle};
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::ris::import_pipeline::{parse_and_validate_from_records, ValidationMode};
use crate::ris::types::RisRecord;
use crate::ris::validator::{validate_record, ErrorGroup};
use crate::zotero::client;
use crate::zotero::mapping;
use crate::zotero::{ZoteroChildItem, ZoteroError, ZoteroItem, ZoteroNoteItem, DEFAULT_BASE_URL};

/// User-facing hint shown whenever the Zotero local API is disabled (or any
/// communication error occurs) - the exact preference path from the Zotero
/// docs, reused by both the import wizard and the export panel.
pub const API_DISABLED_HINT: &str = "Enable the local API in Zotero under Settings -> Advanced -> \"Allow other applications on this computer to communicate with Zotero\", then try again.";

/// Connection status payload. `status` is `ok` / `not_running` /
/// `api_disabled` / `error`; `apiVersion` is the `Zotero-API-Version`
/// response header, `zoteroVersion`/`serverId` (from `X-Zotero-Version` and
/// `Zotero-Server-ID` on every response) gate and echo the write flow.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroConnectionStatus {
    pub status: String,
    pub api_version: Option<String>,
    pub zotero_version: Option<String>,
    pub server_id: Option<String>,
    pub hint: Option<String>,
}

/// Inner connection probe (base URL injectable for tests). Never `Err` -
/// every failure mode maps to a status variant with a hint. `zoteroVersion`
/// prefers the connector ping, which answers even while the local API
/// preference is OFF (so a Zotero 9 with the API disabled still gets the
/// "requires Zotero 10" gate instead of the enable-API card).
pub async fn check_connection_inner(base_url: &str) -> ZoteroConnectionStatus {
    // Best-effort ping first: it carries the version on every path below.
    let ping_version = client::connector_ping_version(base_url).await;
    match client::check_connection(base_url).await {
        Ok(info) => ZoteroConnectionStatus {
            status: "ok".to_string(),
            api_version: info.api_version,
            zotero_version: ping_version.or(info.zotero_version),
            server_id: info.server_id,
            hint: None,
        },
        Err(ZoteroError::NotRunning) => ZoteroConnectionStatus {
            status: "not_running".to_string(),
            api_version: None,
            zotero_version: ping_version,
            server_id: None,
            hint: Some("Start Zotero and try again.".to_string()),
        },
        Err(ZoteroError::ApiDisabled) => ZoteroConnectionStatus {
            status: "api_disabled".to_string(),
            api_version: None,
            zotero_version: ping_version,
            server_id: None,
            hint: Some(API_DISABLED_HINT.to_string()),
        },
        // 404 "No endpoint found": Zotero answered but the local API was not
        // reachable at that moment (startup race / preference not yet active).
        // The hint carries the actionable guidance verbatim; both the wizard
        // and the export panel render it with a Retry.
        Err(e @ ZoteroError::ApiEndpointMissing(_)) => ZoteroConnectionStatus {
            status: "api_disabled".to_string(),
            api_version: None,
            zotero_version: ping_version,
            server_id: None,
            hint: Some(e.to_string()),
        },
        Err(other) => ZoteroConnectionStatus {
            status: "error".to_string(),
            api_version: None,
            zotero_version: ping_version,
            server_id: None,
            hint: Some(other.to_string()),
        },
    }
}

/// Probe the local Zotero API. Returns a status payload (never `Err`) so the
/// wizard can render guidance instead of a raw error.
#[tauri::command]
pub async fn check_zotero_connection() -> ZoteroConnectionStatus {
    check_connection_inner(DEFAULT_BASE_URL).await
}

/// Flat collection list entry: `parentKey` is `None` for root collections
/// (`data.parentCollection` false or absent).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroCollectionInfo {
    pub key: String,
    pub name: String,
    pub parent_key: Option<String>,
}

pub async fn get_collections_inner(
    base_url: &str,
) -> Result<Vec<ZoteroCollectionInfo>, ZoteroError> {
    let collections = client::fetch_collections(base_url).await?;
    let mut flat: Vec<ZoteroCollectionInfo> = collections
        .into_iter()
        .map(|c| ZoteroCollectionInfo {
            key: c.key,
            name: c.data.name,
            parent_key: c.data.parent_collection,
        })
        .collect();
    flat.sort_by_key(|c| c.name.to_lowercase());
    Ok(flat)
}

/// List the Zotero collections (one request; sorted by name).
#[tauri::command]
pub async fn get_zotero_collections() -> Result<Vec<ZoteroCollectionInfo>, AppError> {
    Ok(get_collections_inner(DEFAULT_BASE_URL).await?)
}

/// Preview payload: the standard `ImportPreview` (the review step renders it
/// exactly like an RIS preview) plus Zotero-specific data.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroCollectionPreview {
    pub preview: ImportPreview,
    /// Zotero item keys aligned with `preview.preview_articles` (valid only).
    pub article_keys: Vec<String>,
    pub library_version: Option<i64>,
    pub total_items: usize,
    pub mapped_articles: usize,
    pub attachment_count: usize,
    pub tag_count: usize,
    /// Mapped items with at least one non-empty child note.
    pub note_count: usize,
}

/// Items + attachments -> mapped `RisRecord`s with aligned keys + counts.
/// Pure; shared by the preview (Tier 2) and the import (Tier 3) so both walk
/// the same fetch/mapping path.
pub struct ZoteroMappedData {
    pub records: Vec<RisRecord>,
    pub keys: Vec<String>,
    /// 1-based top-item position of each mapped record (error-group indices).
    pub item_positions: Vec<usize>,
    /// Distinct sanitized tags across the mapped items (dedup preserves order).
    pub tags: Vec<String>,
    /// Sanitized tag names per Zotero item key (import linking).
    pub tags_by_key: std::collections::HashMap<String, Vec<String>>,
    pub total_items: usize,
    pub mapped_articles: usize,
    pub attachment_count: usize,
    /// Mapped items with at least one non-empty child note.
    pub note_count: usize,
    /// Merged child-note text per Zotero item key (import -> `user_notes`).
    pub user_notes_by_key: std::collections::HashMap<String, String>,
    /// 1-based positions of unsupported top-level items (error-group indices).
    pub unsupported: Vec<usize>,
}

pub fn build_mapped_data(
    items: &[ZoteroItem],
    attachments: &[ZoteroChildItem],
    notes: &[ZoteroNoteItem],
) -> ZoteroMappedData {
    let grouped = mapping::group_attachments_by_parent(attachments);
    let grouped_notes = mapping::group_notes_by_parent(notes);
    let mut records: Vec<RisRecord> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut item_positions: Vec<usize> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    let mut tags_by_key: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut attachment_count = 0usize;
    let mut note_count = 0usize;
    let mut user_notes_by_key: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut unsupported: Vec<usize> = Vec::new();

    for (position, item) in items.iter().enumerate() {
        let Some(record) = mapping::map_item_to_ris_record(item) else {
            unsupported.push(position + 1);
            continue;
        };
        let mut item_tags: Vec<String> = Vec::new();
        for tag in &item.data.tags {
            if let Some(sanitized) = mapping::sanitize_zotero_tag(&tag.tag) {
                if !tags.contains(&sanitized) {
                    tags.push(sanitized.clone());
                }
                if !item_tags.contains(&sanitized) {
                    item_tags.push(sanitized);
                }
            }
        }
        if !item_tags.is_empty() {
            tags_by_key.insert(item.key.clone(), item_tags);
        }
        keys.push(item.key.clone());
        item_positions.push(position + 1);
        records.push(record);
        if let Some(children) = grouped.get(&item.key) {
            if mapping::first_full_text_candidate(children).is_some() {
                attachment_count += 1;
            }
        }
        if let Some(item_notes) = grouped_notes.get(&item.key) {
            if let Some(merged) = mapping::merge_child_notes(item_notes) {
                user_notes_by_key.insert(item.key.clone(), merged);
                note_count += 1;
            }
        }
    }

    ZoteroMappedData {
        total_items: items.len(),
        mapped_articles: records.len(),
        records,
        keys,
        item_positions,
        tags,
        tags_by_key,
        attachment_count,
        note_count,
        user_notes_by_key,
        unsupported,
    }
}

pub async fn get_collection_preview_inner(
    base_url: &str,
    db: &std::sync::Mutex<rusqlite::Connection>,
    collection_key: &str,
) -> Result<ZoteroCollectionPreview, ZoteroError> {
    let page = client::fetch_collection_items(base_url, collection_key).await?;
    let parent_keys: Vec<String> = page.items.iter().map(|item| item.key.clone()).collect();
    let attachments = client::fetch_all_attachments(base_url, &parent_keys).await?;
    let notes = client::fetch_all_notes(base_url, &parent_keys).await?;
    let mapped = build_mapped_data(&page.items, &attachments, &notes);

    // Strict validation through the existing mechanism (same as RIS/BibTeX).
    let output = parse_and_validate_from_records(&mapped.records, ValidationMode::Strict)
        .map_err(|e| ZoteroError::Parse(e.to_string()))?;

    // Valid-record detection with keys aligned (same predicate as the
    // validator). Error-group indices use top-item positions (the same index
    // space as the unsupported group), via the mapped item positions.
    let mut valid_pairs: Vec<(&RisRecord, &String)> = Vec::new();
    let mut missing_field_indices: Vec<usize> = Vec::new();
    for (index, (record, key)) in mapped.records.iter().zip(mapped.keys.iter()).enumerate() {
        if validate_record(record, index + 1).is_empty() {
            valid_pairs.push((record, key));
        } else {
            missing_field_indices.push(mapped.item_positions[index]);
        }
    }

    // Early duplicate signal: valid records' canonical DOIs vs the current
    // library (one short DB lock; never held across the HTTP fetches above).
    let valid_records: Vec<&RisRecord> = valid_pairs.iter().map(|(record, _)| *record).collect();
    let duplicate_indices = {
        let conn =
            crate::db::connection::lock_conn(db).map_err(|e| ZoteroError::Http(e.to_string()))?;
        super::import::library_duplicate_indices(&conn, &valid_records)
            .map_err(|e| ZoteroError::Http(e.to_string()))?
    };

    // Error groups: one for unsupported item types, one merged for missing
    // fields (the standard `ErrorGroup` mechanism with Zotero labels).
    let mut error_groups: Vec<ErrorGroup> = Vec::new();
    if !mapped.unsupported.is_empty() {
        error_groups.push(ErrorGroup {
            message: "Unsupported Zotero item type".to_string(),
            count: mapped.unsupported.len(),
            record_indices: mapped.unsupported.clone(),
        });
    }
    if !missing_field_indices.is_empty() {
        error_groups.push(ErrorGroup {
            message: "Missing required fields".to_string(),
            count: missing_field_indices.len(),
            record_indices: missing_field_indices.clone(),
        });
    }

    let preview_articles: Vec<PreviewArticle> = valid_pairs
        .iter()
        .take(10)
        .map(|(record, _)| PreviewArticle {
            title: record.title.clone().unwrap_or_default(),
            authors: record.authors.clone(),
            publication_year: record.publication_year,
            journal: record.journal.clone(),
            doi: record.doi.clone(),
        })
        .collect();
    let article_keys: Vec<String> =
        valid_pairs.iter().take(10).map(|(_, key)| (*key).clone()).collect();

    let errors: Vec<ImportError> = output
        .errors
        .iter()
        .map(|e| ImportError { record_index: e.record_index, message: e.message.clone() })
        .collect();

    let preview = ImportPreview {
        total_records: mapped.total_items,
        valid_records: valid_pairs.len(),
        error_count: mapped.unsupported.len() + output.errors.len(),
        duplicate_count: duplicate_indices.len(),
        duplicate_indices,
        errors,
        error_groups,
        preview_articles,
    };

    Ok(ZoteroCollectionPreview {
        preview,
        article_keys,
        library_version: page.library_version,
        total_items: mapped.total_items,
        mapped_articles: mapped.mapped_articles,
        attachment_count: mapped.attachment_count,
        tag_count: mapped.tags.len(),
        note_count: mapped.note_count,
    })
}

/// Fetch a collection recursively, validate it like an RIS import, and return
/// the standard preview plus Zotero keys/version/counts. Nothing is written.
#[tauri::command]
pub async fn get_zotero_collection_preview(
    db_state: State<'_, DbState>,
    collection_key: String,
) -> Result<ZoteroCollectionPreview, AppError> {
    Ok(get_collection_preview_inner(DEFAULT_BASE_URL, &db_state.conn, &collection_key).await?)
}

// ── Tier 3: the import command ─────────────────────────────────────────────

/// `zotero-import:progress` payload (`{ phase, done, total, failed }`, the
/// batch-import progress convention).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroImportProgress {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub failed: usize,
}

/// Import result: the standard `ImportResult` plus attachment tallies and the
/// merged-notes count.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroImportResult {
    pub result: ImportResult,
    pub attached_count: usize,
    pub attachment_failed_count: usize,
    pub attachment_skipped_count: usize,
    /// Articles that received merged Zotero child notes in `user_notes`.
    pub notes_merged_count: usize,
}

/// Pure version guard: the `Last-Modified-Version` captured at preview time
/// must match the fresh fetch. Key-based filtering already makes ordering
/// irrelevant; this guard catches added, removed, or edited items with
/// nothing written.
pub fn check_library_version(actual: Option<i64>, expected: i64) -> Result<(), AppError> {
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(AppError::Import(
            "Zotero library changed since the preview - re-select the collection and try again"
                .to_string(),
        ))
    }
}

/// Zotero preview/import error groups: one for unsupported item types, one
/// merged for missing fields (the standard `ErrorGroup` mechanism).
pub fn build_zotero_error_groups(
    unsupported: &[usize],
    missing_field_indices: &[usize],
) -> Vec<ErrorGroup> {
    let mut groups = Vec::new();
    if !unsupported.is_empty() {
        groups.push(ErrorGroup {
            message: "Unsupported Zotero item type".to_string(),
            count: unsupported.len(),
            record_indices: unsupported.to_vec(),
        });
    }
    if !missing_field_indices.is_empty() {
        groups.push(ErrorGroup {
            message: "Missing required fields".to_string(),
            count: missing_field_indices.len(),
            record_indices: missing_field_indices.to_vec(),
        });
    }
    groups
}

/// DB phase (drivable with a plain `&Connection` in tests): insert (capacity
/// guard + per-article `'import'` audit inherited from
/// `insert_articles_batch`) -> classify -> resolve journal links ->
/// reference-paper linking -> Zotero tags (`ris_keyword` source, linked per
/// article with `changed_at` bumped) -> merged Zotero child notes ->
/// staleness flags. Returns the `ImportResult` payload plus
/// `(article_id, zotero_key, status)` triples for the attachment phase
/// (status re-read post-classify so duplicates are identifiable).
pub struct ZoteroImportDbResult {
    pub import_payload: ImportResult,
    pub article_key_status: Vec<(String, String, String)>,
    /// Articles whose `user_notes` were filled from merged Zotero child notes.
    pub notes_merged_count: usize,
}

/// Grouped db-phase inputs (keeps the function under the clippy arg limit).
pub struct ZoteroImportDbParams<'a> {
    pub records: &'a [RisRecord],
    pub keys: &'a [String],
    pub skipped_by_user: usize,
    pub skipped_validation: usize,
    /// Drop records whose canonical DOI already exists in the library (the
    /// review-step Skip checkbox) before insert; keys stay aligned.
    pub skip_duplicates: bool,
    pub validation_errors: Vec<ImportError>,
    pub error_groups: Vec<ErrorGroup>,
    pub tags_by_key: &'a std::collections::HashMap<String, Vec<String>>,
    /// Merged child-note text per Zotero item key (written to `user_notes`).
    pub user_notes_by_key: &'a std::collections::HashMap<String, String>,
}

pub fn import_zotero_db_phase(
    conn: &rusqlite::Connection,
    params: ZoteroImportDbParams<'_>,
) -> Result<ZoteroImportDbResult, AppError> {
    let ZoteroImportDbParams {
        records,
        keys,
        skipped_by_user,
        skipped_validation,
        skip_duplicates,
        validation_errors,
        error_groups,
        tags_by_key,
        user_notes_by_key,
    } = params;
    use rusqlite::params;

    // Pre-import duplicate skip (review-step Skip checkbox): drop records
    // whose canonical DOI already exists in the library, keeping records and
    // keys aligned. Everything else still flows to the classify phase below.
    let skipped_duplicates;
    let kept: Vec<(&RisRecord, &String)> = if skip_duplicates {
        let dois: Vec<String> = records.iter().filter_map(|r| r.doi.clone()).collect();
        let present = super::import::library_dois_present(conn, &dois)?;
        let kept: Vec<(&RisRecord, &String)> = records
            .iter()
            .zip(keys.iter())
            .filter(|(record, _)| {
                !crate::ris::doi::normalize_doi(record.doi.as_deref())
                    .is_some_and(|d| present.contains(&d))
            })
            .collect();
        skipped_duplicates = records.len() - kept.len();
        kept
    } else {
        skipped_duplicates = 0;
        records.iter().zip(keys.iter()).collect()
    };

    let new_articles: Vec<crate::models::article::NewArticle> =
        kept.iter().map(|(record, _)| super::import::ris_record_to_new_article(record)).collect();
    let imported = crate::db::article_repo::insert_articles_batch(conn, &new_articles, "zotero")?;
    super::dedup::classify_imported_articles(conn, &imported)?;
    crate::db::article_repo::resolve_journal_links(conn, &imported);
    crate::db::reference_repo::link_imported_articles_to_papers(conn, &imported);

    // Zotero tags -> Bango tags. One representation: tags table + article_tags
    // links, `keywords` stays empty (no double counting in keyword networks).
    let mut all_names: Vec<String> = Vec::new();
    for names in tags_by_key.values() {
        for name in names {
            if !all_names.contains(name) {
                all_names.push(name.clone());
            }
        }
    }
    if !all_names.is_empty() {
        crate::db::tag_repo::create_tags_batch(conn, &all_names, "ris_keyword")?;
        for (article, (_, key)) in imported.iter().zip(kept.iter()) {
            let Some(names) = tags_by_key.get(*key) else { continue };
            for name in names {
                let tag_id: Option<String> = conn
                    .query_row("SELECT id FROM tags WHERE LOWER(name) = LOWER(?1)", [name], |row| {
                        row.get(0)
                    })
                    .ok();
                if let Some(tag_id) = tag_id {
                    conn.execute(
                        "INSERT OR IGNORE INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
                        params![article.id, tag_id],
                    )?;
                }
            }
            crate::db::article_repo::bump_changed_at(conn, &article.id)?;
        }
    }

    // Zotero child notes -> the article's editable user notes. The merged
    // text (date-ordered `Title` / `---` / body blocks, spec 3.1.1) is keyed
    // by Zotero item key, aligned with the kept records above. Articles
    // without notes are left untouched.
    let mut notes_merged = 0usize;
    for (article, (_, key)) in imported.iter().zip(kept.iter()) {
        let Some(merged) = user_notes_by_key.get((*key).as_str()) else { continue };
        crate::db::article_repo::update_user_notes(conn, &article.id, merged)?;
        notes_merged += 1;
    }

    crate::db::app_settings_repo::mark_biblio_needs_refresh(conn);
    crate::db::app_settings_repo::mark_wiki_needs_refresh(conn);

    let updated: Vec<crate::models::article::Article> = imported
        .iter()
        .filter_map(|a| crate::db::article_repo::get_article_by_id(conn, &a.id).ok())
        .collect();
    let remaining = crate::db::article_repo::remaining_capacity(conn)?;
    let article_key_status = updated
        .iter()
        .zip(kept.iter())
        .map(|(a, (_, key))| (a.id.clone(), (*key).clone(), a.status.as_str().to_string()))
        .collect();
    let import_payload = ImportResult {
        imported_count: updated.len(),
        skipped_count: skipped_validation,
        skipped_duplicates,
        skipped_by_user,
        articles: updated,
        remaining_capacity: remaining,
        validation_errors,
        error_groups,
    };
    Ok(ZoteroImportDbResult {
        import_payload,
        article_key_status,
        notes_merged_count: notes_merged,
    })
}

/// The full import flow (no Tauri `AppHandle`, so integration tests drive it
/// with a `Mutex<Connection>` + mockito):
///
/// 1. Re-fetch the collection items exactly as in preview and verify the
///    library version (nothing is written on mismatch).
/// 2. Strict validation + key-based exclusion (unknown keys are ignored;
///    `skippedByUser` counts the known excluded keys).
/// 3. DB phase under a short lock, then `on_imported` (the translation
///    enqueue) with the guard dropped.
/// 4. Attachment phase: for each non-duplicate article with a pdf/txt child,
///    resolve the file via the 302 Location (HTTP never under the DB lock;
///    URL-only Locations count as skipped), then attach through the split
///    pipeline (`extract_full_text_data` unlocked, `commit_full_text_to_db`
///    under a short lock) so the mutex is never held across the file copy.
///    Failures are non-fatal (per-article audit errors +
///    `attachment_failed_count`).
// Grouped params keep most call sites under the arg limit; this internal
// core carries two callbacks alongside the request inputs, so the allow
// matches the `screening::article_writer` precedent.
#[allow(clippy::too_many_arguments)]
pub async fn import_zotero_collection_core(
    base_url: &str,
    db: &std::sync::Mutex<rusqlite::Connection>,
    collection_key: &str,
    excluded_keys: &[String],
    expected_library_version: i64,
    skip_duplicates: bool,
    on_progress: &(dyn Fn(&str, usize, usize, usize) + Send + Sync),
    on_imported: &(dyn Fn(&std::sync::Mutex<rusqlite::Connection>, &[String]) + Send + Sync),
) -> Result<ZoteroImportResult, AppError> {
    // 1. Refetch + version guard.
    let page = client::fetch_collection_items(base_url, collection_key).await?;
    check_library_version(page.library_version, expected_library_version)?;
    let parent_keys: Vec<String> = page.items.iter().map(|item| item.key.clone()).collect();
    let attachments = client::fetch_all_attachments(base_url, &parent_keys).await?;
    let notes = client::fetch_all_notes(base_url, &parent_keys).await?;
    let mapped = build_mapped_data(&page.items, &attachments, &notes);
    let grouped = mapping::group_attachments_by_parent(&attachments);

    // 2. Validation (identical to preview) + key-based exclusion.
    let output = parse_and_validate_from_records(&mapped.records, ValidationMode::Strict)
        .map_err(|e| ZoteroError::Parse(e.to_string()))?;
    let excluded_set: std::collections::HashSet<&str> =
        excluded_keys.iter().map(String::as_str).collect();
    let mut to_import: Vec<(&RisRecord, &String)> = Vec::new();
    let mut known_excluded = 0usize;
    let mut missing_field_indices: Vec<usize> = Vec::new();
    for (index, (record, key)) in mapped.records.iter().zip(mapped.keys.iter()).enumerate() {
        if validate_record(record, index + 1).is_empty() {
            if excluded_set.contains(key.as_str()) {
                known_excluded += 1;
            } else {
                to_import.push((record, key));
            }
        } else {
            missing_field_indices.push(mapped.item_positions[index]);
        }
    }
    // Records skipped by validation (unsupported types + missing fields), the
    // same accounting RIS fills into `skipped_count`.
    let skipped_validation =
        mapped.total_items.saturating_sub(to_import.len()).saturating_sub(known_excluded);
    let validation_errors: Vec<ImportError> = output
        .errors
        .iter()
        .map(|e| ImportError { record_index: e.record_index, message: e.message.clone() })
        .collect();
    let error_groups = build_zotero_error_groups(&mapped.unsupported, &missing_field_indices);

    // 3. DB phase, then enqueue with the guard dropped.
    let records: Vec<RisRecord> = to_import.iter().map(|(record, _)| (**record).clone()).collect();
    let keys: Vec<String> = to_import.iter().map(|(_, key)| (*key).clone()).collect();
    let db_result = {
        let conn = crate::db::connection::lock_conn(db)?;
        import_zotero_db_phase(
            &conn,
            ZoteroImportDbParams {
                records: &records,
                keys: &keys,
                skipped_by_user: known_excluded,
                skipped_validation,
                skip_duplicates,
                validation_errors,
                error_groups,
                tags_by_key: &mapped.tags_by_key,
                user_notes_by_key: &mapped.user_notes_by_key,
            },
        )?
    };
    let imported_ids: Vec<String> =
        db_result.article_key_status.iter().map(|(id, _, _)| id.clone()).collect();
    on_imported(db, &imported_ids);
    on_progress("metadata", imported_ids.len(), imported_ids.len(), 0);

    // 4. Attachment phase. Duplicates keep the existing dedup behavior and
    // are skipped; children that are not pdf/txt count as skipped.
    let storage_dir = {
        let conn = crate::db::connection::lock_conn(db)?;
        crate::commands::full_text::compute_storage_dir(&conn)?
    };
    let mut candidates: Vec<(String, String)> = Vec::new();
    let mut skipped = 0usize;
    for (article_id, item_key, status) in &db_result.article_key_status {
        let Some(children) = grouped.get(item_key) else { continue };
        if status == "duplicate" {
            continue;
        }
        // Count EVERY non-candidate child (a pdf + epub pair still counts
        // the epub), then queue the first candidate for attachment.
        skipped +=
            children.iter().filter(|child| !mapping::is_full_text_candidate(&child.data)).count();
        if let Some(child) = mapping::first_full_text_candidate(children) {
            candidates.push((article_id.clone(), child.key.clone()));
        }
    }

    let mut attached = 0usize;
    let mut failed = 0usize;
    let total = candidates.len();
    for (index, (article_id, attachment_key)) in candidates.iter().enumerate() {
        // HTTP + file resolution never under the DB lock. URL-only attachments
        // (a non-file 302 Location) are expected skips, not failures.
        let resolved = client::fetch_attachment_file_path(base_url, attachment_key).await;
        let path = match resolved {
            Ok(path) => path,
            Err(ZoteroError::NonFileScheme(_)) => {
                skipped += 1;
                on_progress("attachments", index + 1, total, failed);
                continue;
            }
            Err(e) => {
                failed += 1;
                if let Ok(conn) = crate::db::connection::lock_conn(db) {
                    let _ = crate::db::audit_repo::create_entry(
                        &conn,
                        article_id,
                        "error",
                        None,
                        None,
                        Some(&format!("Zotero attachment import failed: {e}")),
                        "system",
                    );
                }
                on_progress("attachments", index + 1, total, failed);
                continue;
            }
        };

        // Split pipeline (batch-import Phase-1 pattern): the DOI is read in a
        // short lock, the file copy + text extraction run with NO lock, and
        // only the DB write takes the lock again - the mutex is never held
        // across the file copy.
        let attach_result: Result<(), AppError> = async {
            let article_doi = {
                let conn = crate::db::connection::lock_conn(db)?;
                crate::db::article_repo::get_article_by_id(&conn, article_id)?.doi
            };
            let extracted = crate::commands::full_text::extract_full_text_data(
                &path,
                article_doi.as_deref(),
                article_id,
                &storage_dir,
            )?;
            let conn = crate::db::connection::lock_conn(db)?;
            crate::commands::full_text::commit_full_text_to_db(&conn, article_id, &extracted)
                .map(|_| ())
        }
        .await;
        match attach_result {
            Ok(()) => attached += 1,
            Err(e) => {
                failed += 1;
                // Per-article audit error (OpenAlex pattern) so the failure
                // surfaces in the article's Audit Timeline.
                if let Ok(conn) = crate::db::connection::lock_conn(db) {
                    let _ = crate::db::audit_repo::create_entry(
                        &conn,
                        article_id,
                        "error",
                        None,
                        None,
                        Some(&format!("Zotero attachment import failed: {e}")),
                        "system",
                    );
                }
            }
        }
        on_progress("attachments", index + 1, total, failed);
    }

    Ok(ZoteroImportResult {
        result: db_result.import_payload,
        attached_count: attached,
        attachment_failed_count: failed,
        attachment_skipped_count: skipped,
        notes_merged_count: db_result.notes_merged_count,
    })
}

/// Import a Zotero collection through the canonical pipeline. Progress is
/// emitted as `zotero-import:progress`; translations are enqueued with the DB
/// guard dropped (existing pattern).
#[tauri::command]
pub async fn import_zotero_collection(
    app: tauri::AppHandle,
    db_state: State<'_, DbState>,
    collection_key: String,
    excluded_keys: Vec<String>,
    expected_library_version: i64,
    skip_duplicates: bool,
) -> Result<ZoteroImportResult, AppError> {
    use tauri::Emitter;

    let app_for_events = app.clone();
    let app_for_enqueue = app.clone();
    import_zotero_collection_core(
        DEFAULT_BASE_URL,
        &db_state.conn,
        &collection_key,
        &excluded_keys,
        expected_library_version,
        skip_duplicates,
        &move |phase, done, total, failed| {
            let _ = app_for_events.emit(
                "zotero-import:progress",
                ZoteroImportProgress { phase: phase.to_string(), done, total, failed },
            );
        },
        &move |db, ids| {
            crate::commands::translation::try_enqueue_translations_for_import(
                &app_for_enqueue,
                db,
                ids,
            );
        },
    )
    .await
}

// ── Tier 5: export (write API) ─────────────────────────────────────────────

/// Best-effort connector call (`POST /connector/getSelectedCollection`): the
/// collection currently selected in the Zotero UI, reported by numeric tree
/// id (never an API key) and correlated by name later. Nulls mean "no
/// default"; this never fails the panel. `lastCollectionKey/Name` carry the
/// picker fallback (the collection the last successful export targeted).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroSelectedCollection {
    pub name: Option<String>,
    pub library_name: Option<String>,
    pub editable: bool,
    pub last_collection_key: Option<String>,
    pub last_collection_name: Option<String>,
}

pub async fn get_selected_collection_inner(
    base_url: &str,
    last_collection_key: Option<String>,
    last_collection_name: Option<String>,
) -> Result<Option<ZoteroSelectedCollection>, ZoteroError> {
    let connector_base = base_url.trim_end_matches("/api");
    let url = format!("{connector_base}/connector/getSelectedCollection");
    // The shared 5s-timeout client (a bare client has no total timeout and
    // can hang forever on a filtered port).
    let response = crate::zotero::client::shared_client_5s()?
        .post(&url)
        .header("X-Zotero-Connector-API-Version", "3")
        .send()
        .await
        .map_err(|e| {
            if e.is_connect() || e.is_timeout() {
                ZoteroError::NotRunning
            } else {
                ZoteroError::Http(e.to_string())
            }
        })?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let value: serde_json::Value =
        response.json().await.map_err(|e| ZoteroError::Parse(e.to_string()))?;
    Ok(Some(ZoteroSelectedCollection {
        name: value.get("name").and_then(|v| v.as_str()).map(str::to_string),
        library_name: value.get("libraryName").and_then(|v| v.as_str()).map(str::to_string),
        editable: value.get("editable").and_then(|v| v.as_bool()).unwrap_or(false),
        last_collection_key,
        last_collection_name,
    }))
}

#[tauri::command]
pub async fn get_zotero_selected_collection(
    db_state: State<'_, DbState>,
) -> Result<Option<ZoteroSelectedCollection>, AppError> {
    // Read the last-collection defaults in a short lock scope; the connector
    // call below never holds the DB mutex.
    let (last_collection_key, last_collection_name) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        (
            crate::db::app_settings_repo::get_setting(&conn, "zotero_last_collection_key")
                .unwrap_or(None),
            crate::db::app_settings_repo::get_setting(&conn, "zotero_last_collection_name")
                .unwrap_or(None),
        )
    };
    Ok(get_selected_collection_inner(DEFAULT_BASE_URL, last_collection_key, last_collection_name)
        .await
        .unwrap_or(None))
}

/// Encrypted `zotero_api_key` (same AES-GCM scheme as the LLM/OpenAlex keys).
fn get_zotero_api_key(conn: &rusqlite::Connection) -> Result<Option<String>, AppError> {
    let Some(encrypted) = crate::db::app_settings_repo::get_setting(conn, "zotero_api_key")? else {
        return Ok(None);
    };
    let key = crate::crypto::aes_gcm::derive_key_from_machine();
    let decrypted = crate::crypto::aes_gcm::decrypt(&encrypted, &key)
        .map_err(|_| AppError::Validation("Failed to decrypt the Zotero API key".into()))?;
    let plaintext = String::from_utf8(decrypted)
        .map_err(|_| AppError::Validation("The Zotero API key is not valid UTF-8".into()))?;
    if plaintext.is_empty() {
        Ok(None)
    } else {
        Ok(Some(plaintext))
    }
}

fn set_zotero_api_key(conn: &rusqlite::Connection, key: Option<&str>) -> Result<(), AppError> {
    let value =
        match key {
            Some(k) if !k.is_empty() => {
                let machine_key = crate::crypto::aes_gcm::derive_key_from_machine();
                Some(crate::crypto::aes_gcm::encrypt(k.as_bytes(), &machine_key).map_err(|_| {
                    AppError::Validation("Failed to encrypt the Zotero API key".into())
                })?)
            }
            _ => None,
        };
    crate::db::app_settings_repo::set_setting(conn, "zotero_api_key", value.as_deref())
}

/// `Zotero < 10` gate: major version parsed from `X-Zotero-Version`.
#[must_use]
pub fn zotero_major_version(version: Option<&str>) -> Option<u32> {
    version?.split('.').next()?.parse().ok()
}

/// `zotero-export:progress` payload (`{ phase, done, total, failed }`).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroExportProgress {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub failed: usize,
}

/// Export preview counts - the DOI diff against the collection's top-level
/// items. Nothing is written.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroExportPreview {
    pub total_articles: usize,
    pub missing_count: usize,
    pub already_present_count: usize,
    pub no_doi_count: usize,
    pub file_count: usize,
}

/// Export result counts + the target collection echo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroExportResult {
    pub exported_count: usize,
    pub failed_count: usize,
    /// Items Zotero reported as unchanged (already up to date).
    pub unchanged_count: usize,
    pub already_present_count: usize,
    pub no_doi_count: usize,
    pub file_attached_count: usize,
    pub file_failed_count: usize,
    pub file_skipped_count: usize,
    /// Child-note items created from Bango user notes.
    pub note_exported_count: usize,
    /// Note-item batches that failed (non-fatal; audited per failure).
    pub note_failed_count: usize,
    pub collection_name: String,
    pub library_version: Option<i64>,
}

use crate::zotero::export_mapping::{classify_export_articles, ExportArticleClass};
use crate::zotero::write_client::{self, ZoteroWriteError};

impl From<ZoteroWriteError> for AppError {
    fn from(e: ZoteroWriteError) -> Self {
        AppError::Import(e.to_string())
    }
}

/// Everything the export needs from the DB, read in ONE short lock scope
/// (never held across HTTP calls or file reads).
struct ZoteroExportDbData {
    articles: Vec<crate::models::article::Article>,
    fulltext_dir: std::path::PathBuf,
    stored_key: Option<String>,
    stored_server_id: Option<String>,
}

fn read_export_db_data(
    conn: &rusqlite::Connection,
    status: &str,
    screening_errors_only: bool,
) -> Result<ZoteroExportDbData, AppError> {
    Ok(ZoteroExportDbData {
        articles: crate::db::article_repo::get_articles_for_export(
            conn,
            status,
            screening_errors_only,
        )?,
        fulltext_dir: std::path::PathBuf::from(crate::db::app_settings_repo::get_fulltext_dir(
            conn,
        )?),
        stored_key: get_zotero_api_key(conn)?,
        stored_server_id: crate::db::app_settings_repo::get_setting(conn, "zotero_server_id")?,
    })
}

/// Canonical DOI set of a collection's top-level items.
async fn collection_dois(
    base_url: &str,
    collection_key: &str,
) -> Result<std::collections::HashSet<String>, ZoteroError> {
    let page = client::fetch_collection_top_items(base_url, collection_key).await?;
    Ok(page
        .items
        .iter()
        .filter_map(|item| crate::ris::doi::normalize_doi(item.data.doi.as_deref()))
        .collect())
}

/// The full-text file of an article when it exists with a `.pdf`/`.txt`
/// extension (the only extensions the Zotero upload accepts).
fn attachable_full_text(
    article: &crate::models::article::Article,
    dir: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let name = article.full_text_file_name.as_deref()?;
    let lower = name.to_lowercase();
    if !(lower.ends_with(".pdf") || lower.ends_with(".txt")) {
        return None;
    }
    let path = dir.join(std::path::Path::new(name).file_name()?);
    path.is_file().then_some(path)
}

pub async fn export_zotero_preview_inner(
    base_url: &str,
    db: &std::sync::Mutex<rusqlite::Connection>,
    collection_key: &str,
    status: &str,
    screening_errors_only: bool,
) -> Result<ZoteroExportPreview, AppError> {
    let data = {
        let conn = crate::db::connection::lock_conn(db)?;
        read_export_db_data(&conn, status, screening_errors_only)?
    };
    let dois = collection_dois(base_url, collection_key).await?;
    let classified = classify_export_articles(&data.articles, &dois);

    let missing =
        classified.iter().filter(|(_, class)| *class == ExportArticleClass::Missing).count();
    let file_count = classified
        .iter()
        .filter(|(article, class)| {
            *class == ExportArticleClass::Missing
                && attachable_full_text(article, &data.fulltext_dir).is_some()
        })
        .count();

    Ok(ZoteroExportPreview {
        total_articles: data.articles.len(),
        missing_count: missing,
        already_present_count: classified
            .iter()
            .filter(|(_, class)| *class == ExportArticleClass::AlreadyPresent)
            .count(),
        no_doi_count: classified
            .iter()
            .filter(|(_, class)| *class == ExportArticleClass::NoDoi)
            .count(),
        file_count,
    })
}

#[tauri::command]
pub async fn export_zotero_preview(
    db_state: State<'_, DbState>,
    collection_key: String,
    status: String,
    screening_errors_only: bool,
) -> Result<ZoteroExportPreview, AppError> {
    export_zotero_preview_inner(
        DEFAULT_BASE_URL,
        &db_state.conn,
        &collection_key,
        &status,
        screening_errors_only,
    )
    .await
}

/// The full export flow (testable without a Tauri `AppHandle`):
/// connection + Zotero 10 gate -> DOI diff -> stored-key reuse policy (at
/// most one authorize per run) -> batches of 50 with fresh write tokens ->
/// optional 3-phase file uploads -> last-collection persistence.
pub async fn export_zotero_collection_core(
    base_url: &str,
    db: &std::sync::Mutex<rusqlite::Connection>,
    collection_key: &str,
    status: &str,
    screening_errors_only: bool,
    include_files: bool,
    on_progress: &(dyn Fn(&str, usize, usize, usize) + Send + Sync),
) -> Result<ZoteroExportResult, AppError> {
    // 1. Connection + version gate + live server id (every response carries it).
    let info = client::check_connection(base_url).await.map_err(ZoteroWriteError::from)?;
    // Gate only when the version is KNOWN and < 10 (aligned with the frontend
    // gate); genuinely old Zotero still fails the writes themselves, which
    // classify to NeedsZotero10.
    if zotero_major_version(info.zotero_version.as_deref()).is_some_and(|major| major < 10) {
        return Err(AppError::Import(ZoteroWriteError::NeedsZotero10.to_string()));
    }
    let Some(server_id) = info.server_id.clone() else {
        return Err(AppError::Import(
            "Zotero did not report a Zotero-Server-ID; cannot write.".to_string(),
        ));
    };

    // 2. Collection name + top-level DOI diff baseline.
    let collections = client::fetch_collections(base_url).await.map_err(ZoteroWriteError::from)?;
    let collection_name = collections
        .iter()
        .find(|c| c.key == collection_key)
        .map(|c| c.data.name.clone())
        .ok_or_else(|| {
            AppError::NotFound(format!("Zotero collection {collection_key} not found"))
        })?;
    let top_page = client::fetch_collection_top_items(base_url, collection_key)
        .await
        .map_err(ZoteroWriteError::from)?;
    let dois: std::collections::HashSet<String> = top_page
        .items
        .iter()
        .filter_map(|item| crate::ris::doi::normalize_doi(item.data.doi.as_deref()))
        .collect();

    // 3. One short DB read scope (never held across HTTP or file reads).
    let data = {
        let conn = crate::db::connection::lock_conn(db)?;
        read_export_db_data(&conn, status, screening_errors_only)?
    };
    let classified = classify_export_articles(&data.articles, &dois);
    let missing: Vec<&crate::models::article::Article> = classified
        .iter()
        .filter(|(_, class)| *class == ExportArticleClass::Missing)
        .map(|(article, _)| *article)
        .collect();
    let already_present =
        classified.iter().filter(|(_, class)| *class == ExportArticleClass::AlreadyPresent).count();
    let no_doi = classified.iter().filter(|(_, class)| *class == ExportArticleClass::NoDoi).count();

    // 4. Stored-key reuse policy: authorize ONLY when the key is missing or
    //    the live server id differs (at most one authorize call per run).
    let auth_decision = write_client::decide_write_auth(
        data.stored_key.as_deref(),
        data.stored_server_id.as_deref(),
        Some(server_id.as_str()),
    );
    eprintln!(
        "[zotero] write auth: {}",
        match auth_decision {
            write_client::WriteAuthDecision::UseStored => "reusing stored key",
            write_client::WriteAuthDecision::Authorize =>
                "authorizing (missing key or server-id mismatch)",
        }
    );
    let mut api_key = match auth_decision {
        write_client::WriteAuthDecision::UseStored => data.stored_key.clone().unwrap_or_default(),
        write_client::WriteAuthDecision::Authorize => {
            on_progress("authorize", 0, 0, 0);
            let (key, _remember) = write_client::authorize(base_url, &server_id, "Bango").await?;
            {
                let conn = crate::db::connection::lock_conn(db)?;
                set_zotero_api_key(&conn, Some(&key))?;
                crate::db::app_settings_repo::set_setting(
                    &conn,
                    "zotero_server_id",
                    Some(&server_id),
                )?;
            }
            key
        }
    };

    // 5. Item batches of 50, a fresh write token per batch. Per-item failures
    //    come from the envelope's `failed` map; the run continues.
    let mut exported = 0usize;
    let mut failed_count = 0usize;
    let mut unchanged = 0usize;
    let mut created: Vec<(&crate::models::article::Article, String)> = Vec::new();
    for batch in missing.chunks(50) {
        let items: Vec<serde_json::Value> = batch
            .iter()
            .map(|article| crate::zotero::export_mapping::build_item_json(article, collection_key))
            .collect();
        match write_client::post_items_batch(base_url, &server_id, &api_key, &items).await {
            Ok(envelope) => {
                exported += envelope.successful_keys.len();
                failed_count += envelope.failed.len();
                unchanged += envelope.unchanged_count;
                for (index, key) in &envelope.success_by_index {
                    if let Some(article) = batch.get(*index) {
                        created.push((article, key.clone()));
                    }
                }
            }
            Err(ZoteroWriteError::KeyExpired) => {
                // A remember:false key is single-use: abort with guidance,
                // clear the stale key, re-authorize once on the next attempt.
                if let Ok(conn) = crate::db::connection::lock_conn(db) {
                    let _ = set_zotero_api_key(&conn, None);
                    let _ = crate::db::app_settings_repo::set_setting(
                        &conn,
                        "zotero_server_id",
                        None::<&str>,
                    );
                }
                return Err(AppError::Import(ZoteroWriteError::KeyExpired.to_string()));
            }
            Err(other) => return Err(AppError::Import(other.to_string())),
        }
        on_progress("items", exported, missing.len(), failed_count);
    }

    // 5b. Notes: one Zotero child-note item per title/---/body block of each
    //     CREATED article's user notes (free-form text -> a single note).
    //     Failures are non-fatal (a system audit error per batch, mirroring
    //     the file phase) - notes are auxiliary and must never block items.
    let mut note_exported = 0usize;
    let mut note_failed = 0usize;
    {
        let note_items: Vec<serde_json::Value> = created
            .iter()
            .filter_map(|(article, key)| {
                let text = article.user_notes.as_deref()?.trim();
                if text.is_empty() {
                    return None;
                }
                Some((key, crate::zotero::export_mapping::split_note_blocks(text)))
            })
            .flat_map(|(key, blocks)| {
                blocks.into_iter().map(move |block| {
                    crate::zotero::export_mapping::build_note_item_json(key, &block)
                })
            })
            .collect();
        let total_notes = note_items.len();
        for batch in note_items.chunks(50) {
            match write_client::post_items_batch(base_url, &server_id, &api_key, batch).await {
                Ok(envelope) => {
                    note_exported += envelope.successful_keys.len();
                    note_failed += envelope.failed.len();
                }
                Err(ZoteroWriteError::KeyExpired) => {
                    // A remember:false key is single-use: abort with the same
                    // guidance as the items phase, clear the stale key.
                    if let Ok(conn) = crate::db::connection::lock_conn(db) {
                        let _ = set_zotero_api_key(&conn, None);
                        let _ = crate::db::app_settings_repo::set_setting(
                            &conn,
                            "zotero_server_id",
                            None::<&str>,
                        );
                    }
                    return Err(AppError::Import(ZoteroWriteError::KeyExpired.to_string()));
                }
                Err(e) => {
                    note_failed += batch.len();
                    eprintln!("[zotero] note export failed for {} note item(s): {e}", batch.len());
                    if let Ok(conn) = crate::db::connection::lock_conn(db) {
                        let _ = crate::db::audit_repo::log_error(
                            &conn,
                            &format!(
                                "Zotero note export failed for {} note item(s): {e}",
                                batch.len()
                            ),
                        );
                    }
                }
            }
            on_progress("notes", note_exported + note_failed, total_notes, note_failed);
        }
    }

    // 6. Files: best-effort 3-phase uploads for created items with an
    //    existing .pdf/.txt full text. Non-fatal failures count plus one
    //    system error audit entry each.
    let mut file_attached = 0usize;
    let mut file_failed = 0usize;
    let mut file_skipped = 0usize;
    if include_files {
        let with_files: Vec<Option<std::path::PathBuf>> = created
            .iter()
            .map(|(article, _)| attachable_full_text(article, &data.fulltext_dir))
            .collect();
        for (index, (article, key)) in created.iter().enumerate() {
            let Some(path) = with_files[index].clone() else {
                file_skipped += 1;
                on_progress(
                    "files",
                    file_attached + file_failed + file_skipped,
                    created.len(),
                    file_failed,
                );
                continue;
            };
            // Friendly attachment title/upload filename: first author's last
            // name, a dash, up to 30 title chars cut at a word boundary, and
            // the extension of the local file.
            let ext =
                path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
            let filename = crate::zotero::export_mapping::build_attachment_title(
                &article.authors,
                &article.title,
                &ext,
            );
            let result: Result<bool, ZoteroWriteError> = async {
                let attachment_key = write_client::create_attachment_item(
                    base_url, &server_id, &api_key, key, &filename, &filename,
                )
                .await?;
                let bytes = std::fs::read(&path).map_err(|e| {
                    ZoteroWriteError::Http(format!("Failed to read {}: {e}", path.display()))
                })?;
                let mtime_ms = std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let params = write_client::build_upload_params(
                    &write_client::md5_hex(&bytes),
                    &filename,
                    bytes.len() as u64,
                    mtime_ms,
                );
                write_client::upload_file(
                    base_url,
                    &server_id,
                    &api_key,
                    &attachment_key,
                    &bytes,
                    &params,
                )
                .await?;
                Ok(true)
            }
            .await;
            match result {
                Ok(_) => file_attached += 1,
                Err(e) => {
                    file_failed += 1;
                    eprintln!("[zotero] file upload failed for '{}': {e}", article.title);
                    if let Ok(conn) = crate::db::connection::lock_conn(db) {
                        let _ = crate::db::audit_repo::log_error(
                            &conn,
                            &format!("Zotero file upload failed for '{}': {e}", article.title),
                        );
                    }
                }
            }
            on_progress(
                "files",
                file_attached + file_failed + file_skipped,
                created.len(),
                file_failed,
            );
        }
    }
    api_key.clear();

    // 7. Remember the last-used collection (picker default fallback).
    {
        if let Ok(conn) = crate::db::connection::lock_conn(db) {
            let _ = crate::db::app_settings_repo::set_setting(
                &conn,
                "zotero_last_collection_key",
                Some(collection_key),
            );
            let _ = crate::db::app_settings_repo::set_setting(
                &conn,
                "zotero_last_collection_name",
                Some(&collection_name),
            );
        }
    }

    eprintln!(
        "[zotero] export done: exported {exported}, unchanged {unchanged}, failed {failed_count}; notes {note_exported} exported / {note_failed} failed; files {file_attached} attached / {file_failed} failed / {file_skipped} skipped ('{collection_name}')"
    );

    Ok(ZoteroExportResult {
        exported_count: exported,
        failed_count,
        unchanged_count: unchanged,
        already_present_count: already_present,
        no_doi_count: no_doi,
        file_attached_count: file_attached,
        file_failed_count: file_failed,
        file_skipped_count: file_skipped,
        note_exported_count: note_exported,
        note_failed_count: note_failed,
        collection_name,
        library_version: top_page.library_version,
    })
}

#[tauri::command]
pub async fn export_zotero_collection(
    app: tauri::AppHandle,
    db_state: State<'_, DbState>,
    collection_key: String,
    status: String,
    screening_errors_only: bool,
    include_files: bool,
) -> Result<ZoteroExportResult, AppError> {
    use tauri::Emitter;

    let app_for_events = app.clone();
    export_zotero_collection_core(
        DEFAULT_BASE_URL,
        &db_state.conn,
        &collection_key,
        &status,
        screening_errors_only,
        include_files,
        &move |phase, done, total, failed| {
            let _ = app_for_events.emit(
                "zotero-export:progress",
                ZoteroExportProgress { phase: phase.to_string(), done, total, failed },
            );
        },
    )
    .await
}
