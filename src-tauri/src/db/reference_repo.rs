use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::reference::{MatchStatus, NewReference, Reference, ReferenceType};

/// Insert a single reference record.
pub fn insert_reference(conn: &Connection, reference: &NewReference) -> Result<Reference, AppError> {
    let id = Uuid::new_v4().to_string();
    let authors_json = serde_json::to_string(&reference.authors)?;
    let keywords_json = serde_json::to_string(&reference.keywords)?;
    let ris_extras_json =
        reference.ris_extras.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        "INSERT INTO article_references (
            id, type, parent_id, match_status,
            title, abstract_text, authors, publication_year, doi,
            journal, volume, issue, start_page, end_page, keywords, url,
            language, publisher, publisher_city, publisher_address, issn,
            reference_type, date, author_address, accession_number,
            custom_field3, journal_abbreviation, journal_iso_abbreviation,
            notes, web_of_science_db, ris_extras,
            num_cited, num_references,
            has_full_text, full_text_file_name,
            import_source
        ) VALUES (
            ?1, ?2, ?3, ?4,
            ?5, ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13, ?14, ?15, ?16,
            ?17, ?18, ?19, ?20, ?21,
            ?22, ?23, ?24, ?25,
            ?26, ?27, ?28,
            ?29, ?30, ?31,
            ?32, ?33,
            ?34, ?35,
            ?36
        )",
        params![
            id,
            reference.reference_type.as_int(),
            reference.parent_id,
            reference.match_status.as_str(),
            reference.title,
            reference.abstract_text,
            authors_json,
            reference.publication_year,
            reference.doi,
            reference.journal,
            reference.volume,
            reference.issue,
            reference.start_page,
            reference.end_page,
            keywords_json,
            reference.url,
            reference.language,
            reference.publisher,
            reference.publisher_city,
            reference.publisher_address,
            reference.issn,
            reference.reference_type_field,
            reference.date,
            reference.author_address,
            reference.accession_number,
            reference.custom_field3,
            reference.journal_abbreviation,
            reference.journal_iso_abbreviation,
            reference.notes,
            reference.web_of_science_db,
            ris_extras_json,
            reference.num_cited,
            reference.num_references,
            reference.has_full_text as i32,
            reference.full_text_file_name,
            reference.import_source,
        ],
    )?;

    // Update parent article flags
    update_parent_flags(conn, &reference.parent_id)?;

    get_reference_by_id(conn, &id)
}

/// Batch insert reference records and update parent article flags.
pub fn insert_references_batch(
    conn: &Connection,
    references: &[NewReference],
) -> Result<Vec<Reference>, AppError> {
    if references.is_empty() {
        return Ok(vec![]);
    }

    let tx = conn.unchecked_transaction()?;
    let mut inserted = Vec::with_capacity(references.len());

    for reference in references {
        let id = Uuid::new_v4().to_string();
        let authors_json = serde_json::to_string(&reference.authors)?;
        let keywords_json = serde_json::to_string(&reference.keywords)?;
        let ris_extras_json =
            reference.ris_extras.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

        tx.execute(
            "INSERT INTO article_references (
                id, type, parent_id, match_status,
                title, abstract_text, authors, publication_year, doi,
                journal, volume, issue, start_page, end_page, keywords, url,
                language, publisher, publisher_city, publisher_address, issn,
                reference_type, date, author_address, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation,
                notes, web_of_science_db, ris_extras,
                num_cited, num_references,
                has_full_text, full_text_file_name,
                import_source
            ) VALUES (
                ?1, ?2, ?3, ?4,
                ?5, ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                ?17, ?18, ?19, ?20, ?21,
                ?22, ?23, ?24, ?25,
                ?26, ?27, ?28,
                ?29, ?30, ?31,
                ?32, ?33,
                ?34, ?35,
                ?36
            )",
            params![
                id,
                reference.reference_type.as_int(),
                reference.parent_id,
                reference.match_status.as_str(),
                reference.title,
                reference.abstract_text,
                authors_json,
                reference.publication_year,
                reference.doi,
                reference.journal,
                reference.volume,
                reference.issue,
                reference.start_page,
                reference.end_page,
                keywords_json,
                reference.url,
                reference.language,
                reference.publisher,
                reference.publisher_city,
                reference.publisher_address,
                reference.issn,
                reference.reference_type_field,
                reference.date,
                reference.author_address,
                reference.accession_number,
                reference.custom_field3,
                reference.journal_abbreviation,
                reference.journal_iso_abbreviation,
                reference.notes,
                reference.web_of_science_db,
                ris_extras_json,
                reference.num_cited,
                reference.num_references,
                reference.has_full_text as i32,
                reference.full_text_file_name,
                reference.import_source,
            ],
        )?;

        inserted.push(get_reference_by_id_tx(&tx, &id)?);
    }

    // Collect unique parent IDs and update their flags
    let parent_ids: std::collections::HashSet<&str> =
        references.iter().map(|r| r.parent_id.as_str()).collect();
    for parent_id in parent_ids {
        update_parent_flags_tx(&tx, parent_id)?;
    }

    tx.commit()?;
    Ok(inserted)
}

/// Get a single reference by ID.
pub fn get_reference_by_id(conn: &Connection, id: &str) -> Result<Reference, AppError> {
    conn.query_row(
        "SELECT * FROM article_references WHERE id = ?1",
        [id],
        row_to_reference,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound(format!("Reference {} not found", id))
        }
        other => AppError::Database(other),
    })
}

fn get_reference_by_id_tx(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
) -> Result<Reference, AppError> {
    tx.query_row("SELECT * FROM article_references WHERE id = ?1", [id], row_to_reference)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Reference {} not found", id))
            }
            other => AppError::Database(other),
        })
}

/// Get all references for a parent article, optionally filtered by type.
pub fn get_references_for_article(
    conn: &Connection,
    parent_id: &str,
    reference_type: Option<&ReferenceType>,
) -> Result<Vec<Reference>, AppError> {
    let sql = match reference_type {
        Some(_rt) => "SELECT * FROM article_references WHERE parent_id = ?1 AND type = ?2 ORDER BY imported_at ASC",
        None => "SELECT * FROM article_references WHERE parent_id = ?1 ORDER BY type, imported_at ASC",
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = match reference_type {
        Some(rt) => stmt.query_map(params![parent_id, rt.as_int()], row_to_reference)?,
        None => stmt.query_map(params![parent_id], row_to_reference)?,
    };
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Count references for a parent article by type.
pub fn count_references_for_article(
    conn: &Connection,
    parent_id: &str,
    reference_type: &ReferenceType,
) -> Result<usize, AppError> {
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM article_references WHERE parent_id = ?1 AND type = ?2",
        params![parent_id, reference_type.as_int()],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Update match status for a reference.
pub fn update_match_status(
    conn: &Connection,
    reference_id: &str,
    new_status: &MatchStatus,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE article_references SET match_status = ?1 WHERE id = ?2",
        params![new_status.as_str(), reference_id],
    )?;
    Ok(())
}

/// Find references matching a given DOI (for auto-matching).
pub fn find_by_doi(conn: &Connection, doi: &str) -> Result<Vec<Reference>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM article_references WHERE doi = ?1")?;
    let rows = stmt.query_map(params![doi], row_to_reference)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Delete all references for a parent article.
pub fn delete_references_for_article(conn: &Connection, parent_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM article_references WHERE parent_id = ?1", params![parent_id])?;
    update_parent_flags(conn, parent_id)?;
    Ok(())
}

/// Update the parent article's `has_citation_details` and `has_reference_details` flags.
fn update_parent_flags(conn: &Connection, parent_id: &str) -> Result<(), AppError> {
    let has_citations = count_references_for_article(conn, parent_id, &ReferenceType::Citation)? > 0;
    let has_references = count_references_for_article(conn, parent_id, &ReferenceType::Reference)? > 0;

    conn.execute(
        "UPDATE articles SET has_citation_details = ?1, has_reference_details = ?2, changed_at = datetime('now') WHERE id = ?3",
        params![has_citations as i32, has_references as i32, parent_id],
    )?;
    Ok(())
}

fn update_parent_flags_tx(tx: &rusqlite::Transaction<'_>, parent_id: &str) -> Result<(), AppError> {
    let citation_count: usize = tx.query_row(
        "SELECT COUNT(*) FROM article_references WHERE parent_id = ?1 AND type = 0",
        params![parent_id],
        |row| row.get(0),
    )?;
    let reference_count: usize = tx.query_row(
        "SELECT COUNT(*) FROM article_references WHERE parent_id = ?1 AND type = 1",
        params![parent_id],
        |row| row.get(0),
    )?;

    tx.execute(
        "UPDATE articles SET has_citation_details = ?1, has_reference_details = ?2, changed_at = datetime('now') WHERE id = ?3",
        params![citation_count > 0, reference_count > 0, parent_id],
    )?;
    Ok(())
}

fn row_to_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<Reference> {
    let type_int: i32 = row.get("type")?;
    let reference_type = ReferenceType::from_int(type_int)
        .ok_or_else(|| rusqlite::Error::InvalidColumnType(1, "type".into(), rusqlite::types::Type::Integer))?;

    let status_str: String = row.get("match_status")?;
    let match_status = MatchStatus::from_str(&status_str)
        .ok_or_else(|| rusqlite::Error::InvalidColumnType(3, "match_status".into(), rusqlite::types::Type::Text))?;

    let authors_str: Option<String> = row.get("authors")?;
    let authors: Vec<String> = authors_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let keywords_str: Option<String> = row.get("keywords")?;
    let keywords: Vec<String> = keywords_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let ris_extras_str: Option<String> = row.get("ris_extras")?;
    let ris_extras: Option<serde_json::Value> =
        ris_extras_str.and_then(|s| serde_json::from_str(&s).ok());

    let has_full_text_int: i32 = row.get("has_full_text")?;

    Ok(Reference {
        id: row.get("id")?,
        reference_type,
        parent_id: row.get("parent_id")?,
        match_status,
        title: row.get("title")?,
        abstract_text: row.get("abstract_text")?,
        authors,
        publication_year: row.get("publication_year")?,
        doi: row.get("doi")?,
        journal: row.get("journal")?,
        volume: row.get("volume")?,
        issue: row.get("issue")?,
        start_page: row.get("start_page")?,
        end_page: row.get("end_page")?,
        keywords,
        url: row.get("url")?,
        language: row.get("language")?,
        publisher: row.get("publisher")?,
        publisher_city: row.get("publisher_city")?,
        publisher_address: row.get("publisher_address")?,
        issn: row.get("issn")?,
        reference_type_field: row.get("reference_type")?,
        date: row.get("date")?,
        author_address: row.get("author_address")?,
        accession_number: row.get("accession_number")?,
        custom_field3: row.get("custom_field3")?,
        journal_abbreviation: row.get("journal_abbreviation")?,
        journal_iso_abbreviation: row.get("journal_iso_abbreviation")?,
        notes: row.get("notes")?,
        web_of_science_db: row.get("web_of_science_db")?,
        user_notes: None,
        ris_extras,
        num_cited: row.get("num_cited")?,
        num_references: row.get("num_references")?,
        has_full_text: has_full_text_int != 0,
        full_text_file_name: row.get("full_text_file_name")?,
        import_source: row.get("import_source")?,
        imported_at: row.get("imported_at")?,
    })
}