use rusqlite::Connection;
use serde::Serialize;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrismaData {
    pub records_identified: usize,
    pub duplicates_removed: usize,
    pub records_screened: usize,
    pub records_excluded: usize,
    pub records_excluded_general: usize,
    pub records_excluded_with_reasons: usize,
    pub records_assessed: usize,
    pub records_in_progress: usize,
    pub studies_included: usize,
    pub exclusion_reasons: Vec<ExclusionReason>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExclusionReason {
    pub criterion_id: String,
    pub criterion_text: String,
    pub count: usize,
}

pub fn compute_prisma_data(conn: &Connection) -> Result<PrismaData, AppError> {
    let records_identified: usize =
        conn.query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0)).unwrap_or(0);

    let duplicates_removed: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE duplicate_of IS NOT NULL", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    let records_screened = records_identified.saturating_sub(duplicates_removed);

    let records_excluded: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'rejected'", [], |row| row.get(0))
        .unwrap_or(0);

    let records_excluded_general: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM articles WHERE status = 'rejected' AND (matched_exclusion_criteria IS NULL OR matched_exclusion_criteria = '[]')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let records_excluded_with_reasons: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM articles WHERE status = 'rejected' AND matched_exclusion_criteria IS NOT NULL AND matched_exclusion_criteria != '[]'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Records actually assessed at full-text = screened minus those generally excluded at screening
    let records_assessed = records_screened.saturating_sub(records_excluded_general);

    let records_in_progress: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'working'", [], |row| row.get(0))
        .unwrap_or(0);

    let studies_included: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'included'", [], |row| row.get(0))
        .unwrap_or(0);

    // Exclusion reasons: count articles per matched exclusion criterion
    let mut stmt = conn.prepare(
        "SELECT matched_exclusion_criteria FROM articles \
         WHERE status = 'rejected' AND matched_exclusion_criteria IS NOT NULL",
    )?;
    let criterion_counts: std::collections::HashMap<String, usize> = {
        let mut counts = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;
        for json_str in rows.flatten() {
            if let Ok(ids) = serde_json::from_str::<Vec<String>>(&json_str) {
                for id in ids {
                    *counts.entry(id).or_insert(0) += 1;
                }
            }
        }
        counts
    };

    let mut exclusion_reasons = Vec::new();
    for (criterion_id, count) in criterion_counts {
        let text: String = conn
            .query_row("SELECT text FROM criteria WHERE id = ?1", [&criterion_id], |row| row.get(0))
            .unwrap_or_else(|_| criterion_id.clone());
        exclusion_reasons.push(ExclusionReason { criterion_id, criterion_text: text, count });
    }

    exclusion_reasons.sort_by_key(|b| std::cmp::Reverse(b.count));

    Ok(PrismaData {
        records_identified,
        duplicates_removed,
        records_screened,
        records_excluded,
        records_excluded_general,
        records_excluded_with_reasons,
        records_assessed,
        records_in_progress,
        studies_included,
        exclusion_reasons,
    })
}
