use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::reference::{
    ArticleReference, ArticleReferenceLink, LinkedArticleInfo, MatchStatus, NewReferencePaper,
    ReferencePaper, ReferenceType,
};

// ─── Reference Paper operations ────────────────────────────────

/// Insert a reference paper, or find an existing one by DOI or title+authors+year.
/// Returns the (paper, was_created) tuple.
pub fn insert_or_find_paper(
    conn: &Connection,
    new_paper: &NewReferencePaper,
) -> Result<(ReferencePaper, bool), AppError> {
    // Normalize DOI: treat empty string as None
    let doi_normalized = new_paper.doi.as_deref().and_then(|d| {
        if d.is_empty() { None } else { Some(d) }
    });

    // Try to find existing by DOI first
    if let Some(doi) = doi_normalized {
        if let Some(existing) = find_paper_by_doi(conn, doi)? {
            return Ok((existing, false));
        }
    }

    // Try to find by title + authors + year
    if let Some(ref title) = new_paper.title {
        if !title.is_empty() {
            if let Some(existing) = find_paper_by_title_authors_year(
                conn,
                title,
                &new_paper.authors,
                new_paper.publication_year,
            )? {
                return Ok((existing, false));
            }
        }
    }

    // Insert new paper (uses INSERT OR IGNORE for constraint safety)
    match insert_paper(conn, new_paper) {
        Ok(paper) => Ok((paper, true)),
        // If constraint violation, look up existing
        Err(_) => {
            // Try DOI lookup first
            if let Some(doi) = doi_normalized {
                if let Some(existing) = find_paper_by_doi(conn, doi)? {
                    return Ok((existing, false));
                }
            }
            // Then title+authors+year
            if let Some(ref title) = new_paper.title {
                if !title.is_empty() {
                    if let Some(existing) = find_paper_by_title_authors_year(
                        conn, title, &new_paper.authors, new_paper.publication_year,
                    )? {
                        return Ok((existing, false));
                    }
                }
            }
            Err(AppError::Database(rusqlite::Error::InvalidParameterName(
                "Failed to insert or find reference paper".into(),
            )))
        }
    }
}

/// Insert a new reference paper.
/// Normalizes empty DOI to NULL to avoid unique constraint violations.
fn insert_paper(
    conn: &Connection,
    new_paper: &NewReferencePaper,
) -> Result<ReferencePaper, AppError> {
    let id = Uuid::new_v4().to_string();
    let authors_json = serde_json::to_string(&new_paper.authors).unwrap_or_else(|_| "[]".into());
    let keywords_json = serde_json::to_string(&new_paper.keywords).unwrap_or_else(|_| "[]".into());
    let ris_extras_json =
        new_paper.ris_extras.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
    let match_status = new_paper.match_status.as_ref().unwrap_or(&MatchStatus::Unmatched).as_str();

    // Normalize DOI: empty string → NULL (prevents unique constraint violations)
    let doi: Option<String> = new_paper
        .doi
        .as_deref()
        .and_then(|d| if d.is_empty() { None } else { Some(d.to_string()) });

    conn.execute(
        "INSERT INTO reference_papers (
            id, title, abstract_text, authors, publication_year, doi,
            journal, volume, issue, start_page, end_page, keywords, url,
            language, publisher, publisher_city, publisher_address, issn,
            reference_type, date, notes, ris_extras,
            match_status, matched_article_id, import_source
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22,
            ?23, ?24, ?25
        )",
        params![
            id,
            new_paper.title.as_deref().unwrap_or(""),
            new_paper.abstract_text,
            authors_json,
            new_paper.publication_year,
            doi,
            new_paper.journal,
            new_paper.volume,
            new_paper.issue,
            new_paper.start_page,
            new_paper.end_page,
            keywords_json,
            new_paper.url,
            new_paper.language,
            new_paper.publisher,
            new_paper.publisher_city,
            new_paper.publisher_address,
            new_paper.issn,
            new_paper.reference_type,
            new_paper.date,
            new_paper.notes,
            ris_extras_json,
            match_status,
            new_paper.matched_article_id,
            new_paper.import_source,
        ],
    )?;

    get_paper_by_id(conn, &id)
}

/// Get a reference paper by ID.
pub fn get_paper_by_id(conn: &Connection, id: &str) -> Result<ReferencePaper, AppError> {
    conn.query_row("SELECT * FROM reference_papers WHERE id = ?1", [id], row_to_paper).map_err(
        |e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Reference paper {} not found", id))
            }
            other => AppError::Database(other),
        },
    )
}

/// Find a reference paper by DOI.
pub fn find_paper_by_doi(conn: &Connection, doi: &str) -> Result<Option<ReferencePaper>, AppError> {
    let result = conn.query_row(
        "SELECT * FROM reference_papers WHERE doi = ?1 LIMIT 1",
        [doi],
        row_to_paper,
    );
    match result {
        Ok(paper) => Ok(Some(paper)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Find a reference paper by title + authors + year (matches unique constraint).
fn find_paper_by_title_authors_year(
    conn: &Connection,
    title: &str,
    authors: &[String],
    year: Option<i32>,
) -> Result<Option<ReferencePaper>, AppError> {
    let authors_json = serde_json::to_string(authors).unwrap_or_else(|_| "[]".into());

    let result = match year {
        Some(y) => conn.query_row(
            "SELECT * FROM reference_papers WHERE LOWER(title) = LOWER(?1) AND authors = ?2 AND publication_year = ?3 LIMIT 1",
            params![title, authors_json, y],
            row_to_paper,
        ),
        None => conn.query_row(
            "SELECT * FROM reference_papers WHERE LOWER(title) = LOWER(?1) AND authors = ?2 LIMIT 1",
            params![title, authors_json],
            row_to_paper,
        ),
    };

    match result {
        Ok(paper) => Ok(Some(paper)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Try to match a reference paper against the articles table.
/// Matches by DOI first, then by title + journal + year.
pub fn auto_match_paper_to_article(
    conn: &Connection,
    paper: &ReferencePaper,
) -> Result<Option<String>, AppError> {
    // Try DOI match
    if let Some(ref doi) = paper.doi {
        if !doi.is_empty() {
            let result: Option<String> = conn
                .query_row("SELECT id FROM articles WHERE doi = ?1 LIMIT 1", [doi], |row| {
                    row.get(0)
                })
                .ok();
            if let Some(article_id) = result {
                return Ok(Some(article_id));
            }
        }
    }

    // Try title + journal + year match
    if !paper.title.is_empty() {
        let result = match (paper.journal.as_deref(), paper.publication_year) {
            (Some(j), Some(y)) => conn.query_row(
                "SELECT id FROM articles WHERE LOWER(title) = LOWER(?1) AND LOWER(journal) = LOWER(?2) AND publication_year = ?3 LIMIT 1",
                params![paper.title, j, y],
                |row| row.get::<_, String>(0),
            ).ok(),
            (Some(j), None) => conn.query_row(
                "SELECT id FROM articles WHERE LOWER(title) = LOWER(?1) AND LOWER(journal) = LOWER(?2) LIMIT 1",
                params![paper.title, j],
                |row| row.get::<_, String>(0),
            ).ok(),
            (None, Some(y)) => conn.query_row(
                "SELECT id FROM articles WHERE LOWER(title) = LOWER(?1) AND publication_year = ?2 LIMIT 1",
                params![paper.title, y],
                |row| row.get::<_, String>(0),
            ).ok(),
            (None, None) => conn.query_row(
                "SELECT id FROM articles WHERE LOWER(title) = LOWER(?1) LIMIT 1",
                params![paper.title],
                |row| row.get::<_, String>(0),
            ).ok(),
        };
        if let Some(article_id) = result {
            return Ok(Some(article_id));
        }
    }

    Ok(None)
}

/// Promote a reference paper to a full article in the library.
/// Returns the ID of the newly created article.
pub fn promote_to_article(
    conn: &Connection,
    paper_id: &str,
    new_article_id: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE reference_papers SET match_status = 'imported', matched_article_id = ?1, updated_at = datetime('now') WHERE id = ?2",
        params![new_article_id, paper_id],
    )?;
    Ok(())
}

/// Update match status and matched_article_id for a reference paper.
pub fn update_paper_match(
    conn: &Connection,
    paper_id: &str,
    match_status: &MatchStatus,
    matched_article_id: Option<&str>,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE reference_papers SET match_status = ?1, matched_article_id = ?2, updated_at = datetime('now') WHERE id = ?3",
        params![match_status.as_str(), matched_article_id, paper_id],
    )?;
    Ok(())
}

/// Increment citation_count or reference_count for a paper.
pub fn increment_paper_count(
    conn: &Connection,
    paper_id: &str,
    ref_type: &ReferenceType,
) -> Result<(), AppError> {
    let column = match ref_type {
        ReferenceType::Citation => "citation_count",
        ReferenceType::Reference => "reference_count",
    };
    let sql = format!(
        "UPDATE reference_papers SET {} = {} + 1, updated_at = datetime('now') WHERE id = ?1",
        column, column
    );
    conn.execute(&sql, [paper_id])?;
    Ok(())
}

/// Decrement citation_count or reference_count for a paper.
fn decrement_paper_count(
    conn: &Connection,
    paper_id: &str,
    ref_type: &ReferenceType,
) -> Result<(), AppError> {
    let column = match ref_type {
        ReferenceType::Citation => "citation_count",
        ReferenceType::Reference => "reference_count",
    };
    let sql = format!(
        "UPDATE reference_papers SET {} = MAX(0, {} - 1), updated_at = datetime('now') WHERE id = ?1",
        column, column
    );
    conn.execute(&sql, [paper_id])?;
    Ok(())
}

// ─── Article Reference Link operations ─────────────────────────

/// Create a link between an article and a reference paper.
/// Also increments the appropriate counter on the paper.
/// Returns the created link.
pub fn create_link(
    conn: &Connection,
    parent_article_id: &str,
    reference_paper_id: &str,
    ref_type: &ReferenceType,
) -> Result<ArticleReferenceLink, AppError> {
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO article_reference_links (id, parent_article_id, reference_paper_id, type)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(parent_article_id, reference_paper_id, type) DO NOTHING",
        params![id, parent_article_id, reference_paper_id, ref_type.as_int()],
    )?;

    // Check if the insert actually happened
    let changes = conn.changes();
    if changes == 0 {
        // Link already existed, fetch it
        return get_link(conn, parent_article_id, reference_paper_id, ref_type);
    }

    // Increment counter on the paper
    increment_paper_count(conn, reference_paper_id, ref_type)?;

    // Update parent article flags
    update_parent_flags(conn, parent_article_id)?;

    get_link(conn, parent_article_id, reference_paper_id, ref_type)
}

/// Batch-create links for a parent article with multiple reference papers.
pub fn create_links_batch(
    conn: &Connection,
    parent_article_id: &str,
    paper_ids: &[(String, ReferenceType)],
) -> Result<Vec<ArticleReferenceLink>, AppError> {
    if paper_ids.is_empty() {
        return Ok(vec![]);
    }

    let tx = conn.unchecked_transaction()?;
    let mut links = Vec::with_capacity(paper_ids.len());

    for (paper_id, ref_type) in paper_ids {
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO article_reference_links (id, parent_article_id, reference_paper_id, type)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(parent_article_id, reference_paper_id, type) DO NOTHING",
            params![id, parent_article_id, paper_id, ref_type.as_int()],
        )?;

        if tx.changes() > 0 {
            let column = match ref_type {
                ReferenceType::Citation => "citation_count",
                ReferenceType::Reference => "reference_count",
            };
            let sql = format!(
                "UPDATE reference_papers SET {} = {} + 1, updated_at = datetime('now') WHERE id = ?1",
                column, column
            );
            tx.execute(&sql, [paper_id])?;
        }

        if let Ok(link) = tx.query_row(
            "SELECT * FROM article_reference_links WHERE parent_article_id = ?1 AND reference_paper_id = ?2 AND type = ?3",
            params![parent_article_id, paper_id, ref_type.as_int()],
            row_to_link,
        ) {
            links.push(link);
        }
    }

    // Update parent article flags
    update_parent_flags_tx(&tx, parent_article_id)?;

    tx.commit()?;
    Ok(links)
}

/// Get a specific link.
fn get_link(
    conn: &Connection,
    parent_article_id: &str,
    reference_paper_id: &str,
    ref_type: &ReferenceType,
) -> Result<ArticleReferenceLink, AppError> {
    conn.query_row(
        "SELECT * FROM article_reference_links WHERE parent_article_id = ?1 AND reference_paper_id = ?2 AND type = ?3",
        params![parent_article_id, reference_paper_id, ref_type.as_int()],
        row_to_link,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound("Reference link not found".into())
        }
        other => AppError::Database(other),
    })
}

/// Get all reference papers linked to an article, with link context.
pub fn get_references_for_article(
    conn: &Connection,
    parent_article_id: &str,
    ref_type: Option<&ReferenceType>,
) -> Result<Vec<ArticleReference>, AppError> {
    let sql = match ref_type {
        Some(_) => {
            "SELECT l.id as link_id, l.parent_article_id, l.type, l.created_at as link_created_at,
                    p.*
             FROM article_reference_links l
             JOIN reference_papers p ON p.id = l.reference_paper_id
             WHERE l.parent_article_id = ?1 AND l.type = ?2
             ORDER BY p.publication_year DESC NULLS LAST, p.title ASC"
        }
        None => {
            "SELECT l.id as link_id, l.parent_article_id, l.type, l.created_at as link_created_at,
                    p.*
             FROM article_reference_links l
             JOIN reference_papers p ON p.id = l.reference_paper_id
             WHERE l.parent_article_id = ?1
             ORDER BY l.type, p.publication_year DESC NULLS LAST, p.title ASC"
        }
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = match ref_type {
        Some(rt) => {
            stmt.query_map(params![parent_article_id, rt.as_int()], row_to_article_reference)?
        }
        None => stmt.query_map(params![parent_article_id], row_to_article_reference)?,
    };
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Count references for a parent article by type.
pub fn count_references_for_article(
    conn: &Connection,
    parent_article_id: &str,
    ref_type: &ReferenceType,
) -> Result<usize, AppError> {
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM article_reference_links WHERE parent_article_id = ?1 AND type = ?2",
        params![parent_article_id, ref_type.as_int()],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Delete all reference links for a parent article and decrement paper counters.
pub fn delete_references_for_article(
    conn: &Connection,
    parent_article_id: &str,
) -> Result<(), AppError> {
    // Get all links to decrement counters
    let links = {
        let mut stmt = conn.prepare(
            "SELECT reference_paper_id, type FROM article_reference_links WHERE parent_article_id = ?1",
        )?;
        let rows = stmt.query_map([parent_article_id], |row| {
            let paper_id: String = row.get(0)?;
            let type_int: i32 = row.get(1)?;
            Ok((paper_id, type_int))
        })?;
        rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
    };

    for (paper_id, type_int) in &links {
        if let Some(rt) = ReferenceType::from_int(*type_int) {
            decrement_paper_count(conn, paper_id, &rt)?;
        }
    }

    conn.execute(
        "DELETE FROM article_reference_links WHERE parent_article_id = ?1",
        [parent_article_id],
    )?;

    update_parent_flags(conn, parent_article_id)?;
    Ok(())
}

// ─── Row mappers ───────────────────────────────────────────────

fn row_to_paper(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReferencePaper> {
    let status_str: String = row.get("match_status")?;
    let match_status = MatchStatus::from_str(&status_str).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(0, "match_status".into(), rusqlite::types::Type::Text)
    })?;

    let authors_str: Option<String> = row.get("authors")?;
    let authors: Vec<String> =
        authors_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

    let keywords_str: Option<String> = row.get("keywords")?;
    let keywords: Vec<String> =
        keywords_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

    let ris_extras_str: Option<String> = row.get("ris_extras")?;
    let ris_extras: Option<serde_json::Value> =
        ris_extras_str.and_then(|s| serde_json::from_str(&s).ok());

    Ok(ReferencePaper {
        id: row.get("id")?,
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
        reference_type: row.get("reference_type")?,
        date: row.get("date")?,
        notes: row.get("notes")?,
        ris_extras,
        match_status,
        matched_article_id: row.get("matched_article_id")?,
        citation_count: row.get("citation_count")?,
        reference_count: row.get("reference_count")?,
        import_source: row.get("import_source")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn row_to_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArticleReferenceLink> {
    let type_int: i32 = row.get("type")?;
    let reference_type = ReferenceType::from_int(type_int).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(0, "type".into(), rusqlite::types::Type::Integer)
    })?;

    Ok(ArticleReferenceLink {
        id: row.get("id")?,
        parent_article_id: row.get("parent_article_id")?,
        reference_paper_id: row.get("reference_paper_id")?,
        reference_type,
        created_at: row.get("created_at")?,
    })
}

fn row_to_article_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArticleReference> {
    let type_int: i32 = row.get("type")?;
    let reference_type = ReferenceType::from_int(type_int).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(0, "type".into(), rusqlite::types::Type::Integer)
    })?;

    Ok(ArticleReference {
        link_id: row.get("link_id")?,
        parent_article_id: row.get("parent_article_id")?,
        reference_type,
        link_created_at: row.get("link_created_at")?,
        paper: row_to_paper(row)?,
    })
}

// ─── Article flag updates ──────────────────────────────────────

fn update_parent_flags(conn: &Connection, parent_article_id: &str) -> Result<(), AppError> {
    let has_citations =
        count_references_for_article(conn, parent_article_id, &ReferenceType::Citation)? > 0;
    let has_references =
        count_references_for_article(conn, parent_article_id, &ReferenceType::Reference)? > 0;

    conn.execute(
        "UPDATE articles SET has_citation_details = ?1, has_reference_details = ?2, changed_at = datetime('now') WHERE id = ?3",
        params![has_citations as i32, has_references as i32, parent_article_id],
    )?;
    Ok(())
}

fn update_parent_flags_tx(
    tx: &rusqlite::Transaction<'_>,
    parent_article_id: &str,
) -> Result<(), AppError> {
    let citation_count: usize = tx.query_row(
        "SELECT COUNT(*) FROM article_reference_links WHERE parent_article_id = ?1 AND type = 0",
        params![parent_article_id],
        |row| row.get(0),
    )?;
    let reference_count: usize = tx.query_row(
        "SELECT COUNT(*) FROM article_reference_links WHERE parent_article_id = ?1 AND type = 1",
        params![parent_article_id],
        |row| row.get(0),
    )?;

    tx.execute(
        "UPDATE articles SET has_citation_details = ?1, has_reference_details = ?2, changed_at = datetime('now') WHERE id = ?3",
        params![citation_count > 0, reference_count > 0, parent_article_id],
    )?;
    Ok(())
}

// ─── References Tab queries ────────────────────────────────────

/// Search reference papers with pagination.
/// Searches across title, authors, abstract_text, and journal using LIKE.
/// Returns (papers, total_count).
pub fn query_reference_papers(
    conn: &Connection,
    search: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<ReferencePaper>, usize), AppError> {
    let (count_sql, data_sql) = match search {
        Some(term) if !term.is_empty() => {
            (
                "SELECT COUNT(*) FROM reference_papers WHERE title LIKE ?1 OR authors LIKE ?1 OR abstract_text LIKE ?1 OR journal LIKE ?1".to_string(),
                "SELECT * FROM reference_papers WHERE title LIKE ?1 OR authors LIKE ?1 OR abstract_text LIKE ?1 OR journal LIKE ?1 ORDER BY (citation_count + reference_count) DESC, title ASC LIMIT ?2 OFFSET ?3".to_string(),
            )
        }
        _ => (
            "SELECT COUNT(*) FROM reference_papers".to_string(),
            "SELECT * FROM reference_papers ORDER BY (citation_count + reference_count) DESC, title ASC LIMIT ?2 OFFSET ?3".to_string(),
        ),
    };

    let total: usize = match search {
        Some(term) if !term.is_empty() => {
            let pattern = format!("%{}%", term);
            conn.query_row(&count_sql, params![pattern], |row| row.get(0))?
        }
        _ => conn.query_row(&count_sql, [], |row| row.get(0))?,
    };

    let mut stmt = conn.prepare(&data_sql)?;
    let papers: Vec<ReferencePaper> = match search {
        Some(term) if !term.is_empty() => {
            let pattern = format!("%{}%", term);
            stmt.query_map(params![pattern, limit, offset], row_to_paper)?
                .filter_map(|r| r.ok())
                .collect()
        }
        _ => stmt.query_map(params![limit, offset], row_to_paper)?.filter_map(|r| r.ok()).collect(),
    };

    Ok((papers, total))
}

/// Get top 10 "articles of interest": unmatched reference papers with more than 2 total uses
/// (citation_count + reference_count > 2) that are not matched to any article.
pub fn get_articles_of_interest(conn: &Connection) -> Result<Vec<ReferencePaper>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT * FROM reference_papers
         WHERE match_status = 'unmatched'
           AND (citation_count + reference_count) > 2
         ORDER BY (citation_count + reference_count) DESC
         LIMIT 10",
    )?;
    let rows = stmt.query_map([], row_to_paper)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Get all parent articles linked to a reference paper (via article_reference_links).
pub fn get_linked_articles_for_paper(
    conn: &Connection,
    paper_id: &str,
) -> Result<Vec<LinkedArticleInfo>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.title, a.authors, a.publication_year, a.journal, l.type
         FROM article_reference_links l
         JOIN articles a ON a.id = l.parent_article_id
         WHERE l.reference_paper_id = ?1
         ORDER BY a.title ASC",
    )?;
    let rows = stmt.query_map(params![paper_id], |row| {
        // SELECT order: a.id(0), a.title(1), a.authors(2), a.publication_year(3), a.journal(4), l.type(5)
        let type_int: i32 = row.get(5)?;
        let reference_type = ReferenceType::from_int(type_int).ok_or_else(|| {
            rusqlite::Error::InvalidColumnType(5, "type".into(), rusqlite::types::Type::Integer)
        })?;

        let authors_str: Option<String> = row.get(2)?;
        let authors: Vec<String> =
            authors_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

        Ok(LinkedArticleInfo {
            id: row.get(0)?,
            title: row.get(1)?,
            authors,
            publication_year: row.get(3)?,
            journal: row.get(4)?,
            reference_type,
        })
    })?;
    let results: Vec<LinkedArticleInfo> = rows.filter_map(|r| r.ok()).collect();
    Ok(results)
}
