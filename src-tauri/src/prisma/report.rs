//! PRISMA screening reasons report: per-criterion inclusion/exclusion counts
//! and the Markdown rendering consumed by the frontend "Export Report" action.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::Serialize;

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::criteria_repo;
use crate::error::AppError;
use crate::models::criterion::{Criterion, Priority};
use crate::prisma::data;

/// One counted row of a report table. `priority` is `None` for rows whose
/// criterion no longer exists (deleted after screening).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReasonCount {
    pub criterion_id: String,
    pub criterion_text: String,
    pub priority: Option<Priority>,
    pub count: usize,
}

/// The report payload: headline PRISMA numbers + the four tables.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrismaReport {
    pub generated_at: String,
    /// Custom project name (Dashboard title). `None` = default (unset):
    /// the report then opens with the report title as the single h1.
    pub project_name: Option<String>,
    pub records_identified: usize,
    pub duplicates_removed: usize,
    pub records_screened: usize,
    pub records_in_progress: usize,
    pub records_excluded: usize,
    pub records_excluded_general: usize,
    pub records_excluded_with_reasons: usize,
    pub studies_included: usize,
    pub primary_inclusion: Vec<ReasonCount>,
    pub general_inclusion_count: usize,
    pub multi_inclusion: Vec<ReasonCount>,
    pub primary_exclusion: Vec<ReasonCount>,
    pub general_exclusion_count: usize,
    pub multi_exclusion: Vec<ReasonCount>,
}

/// Most significant criterion among `ids`: highest priority wins; ties go to
/// the earliest position in `ids` (first-assigned order). Unresolvable ids
/// (criterion deleted after screening) never win; when no id resolves, the
/// article belongs to the "General" bucket instead.
#[must_use]
pub fn primary_reason<'a>(ids: &[String], criteria: &'a [Criterion]) -> Option<&'a Criterion> {
    let mut best: Option<&Criterion> = None;
    for id in ids {
        let Some(criterion) = criteria.iter().find(|c| c.id == *id) else {
            continue;
        };
        let replace = match best {
            None => true,
            Some(best) => criterion.priority > best.priority,
        };
        if replace {
            best = Some(criterion);
        }
    }
    best
}

/// Primary attribution: one count per article under its most significant
/// criterion. Returns (criterion rows, count of articles with no resolvable
/// matched criterion - the "General" bucket).
#[must_use]
pub fn count_primary(
    article_ids: &[&[String]],
    criteria: &[Criterion],
) -> (Vec<ReasonCount>, usize) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut general = 0_usize;
    for ids in article_ids {
        match primary_reason(ids, criteria) {
            Some(criterion) => *counts.entry(criterion.id.clone()).or_insert(0) += 1,
            None => general += 1,
        }
    }
    (to_reason_counts(&counts, criteria), general)
}

/// Multi-assignment: one count per article per matched criterion id, so an
/// article matching N criteria contributes to N rows.
#[must_use]
pub fn count_multi(article_ids: &[&[String]], criteria: &[Criterion]) -> Vec<ReasonCount> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for ids in article_ids {
        for id in *ids {
            *counts.entry(id.clone()).or_insert(0) += 1;
        }
    }
    to_reason_counts(&counts, criteria)
}

/// Resolve id counts into sorted rows: priority desc (deleted criteria last),
/// count desc, then criterion text asc for a stable order.
#[must_use]
fn to_reason_counts(counts: &HashMap<String, usize>, criteria: &[Criterion]) -> Vec<ReasonCount> {
    let mut rows: Vec<ReasonCount> = counts
        .iter()
        .map(|(id, count)| match criteria.iter().find(|c| c.id == *id) {
            Some(criterion) => ReasonCount {
                criterion_id: id.clone(),
                criterion_text: criterion.text.clone(),
                priority: Some(criterion.priority),
                count: *count,
            },
            None => ReasonCount {
                criterion_id: id.clone(),
                criterion_text: format!("Deleted criterion ({id})"),
                priority: None,
                count: *count,
            },
        })
        .collect();
    rows.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(b.count.cmp(&a.count))
            .then_with(|| a.criterion_text.cmp(&b.criterion_text))
    });
    rows
}

/// Escape user text for a Markdown table cell: pipes, angle brackets, newlines.
#[must_use]
fn md_cell(text: &str) -> String {
    text.replace('|', "\\|")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace(&['\n', '\r'][..], " ")
}

/// Append one GFM table: criterion rows, the General row, and (for the
/// primary tables) a bold total row that equals the segment's article count.
fn push_reason_table(
    out: &mut String,
    column: &str,
    segment: &str,
    rows: &[ReasonCount],
    general_count: usize,
    total: Option<usize>,
) {
    out.push_str(&format!("| Priority | {column} | Articles |\n"));
    out.push_str("| --- | --- | ---: |\n");
    for row in rows {
        let priority = row.priority.map_or_else(|| "-".to_string(), |p| p.as_str().to_string());
        out.push_str(&format!(
            "| {priority} | {} | {} |\n",
            md_cell(&row.criterion_text),
            row.count
        ));
    }
    out.push_str(&format!("| - | General {segment} (no criterion matched) | {general_count} |\n"));
    if let Some(total) = total {
        out.push_str(&format!("| **Total** |  | **{total}** |\n"));
    }
    out.push('\n');
}

/// Single-line sanitize for the project-name heading (already trimmed + capped
/// by `set_project_name`; only interior newlines need flattening).
#[must_use]
fn heading_text(name: &str) -> String {
    name.replace(&['\n', '\r'][..], " ")
}

/// Render the report as GitHub-flavored Markdown (tables render via `marked`
/// for the PDF print path).
#[must_use]
pub fn render_prisma_report_markdown(report: &PrismaReport) -> String {
    let mut md = String::new();

    /* Title block: with a custom project name, the project is the h1 and the
    report title the h2 (sections demoted to h3); otherwise the report title
    is the single h1 with h2 sections. */
    let section = match &report.project_name {
        Some(name) => {
            md.push_str(&format!("# {}\n\n", heading_text(name)));
            md.push_str("## PRISMA Screening Reasons Report\n\n");
            "###"
        }
        None => {
            md.push_str("# PRISMA Screening Reasons Report\n\n");
            "##"
        }
    };
    md.push_str(&format!("Generated: {}\n\n", report.generated_at));

    md.push_str(&format!("{section} Overview\n\n"));
    md.push_str(
        "This report breaks down the screening decisions recorded in this Bango project by criterion, separately for inclusions and exclusions.\n\n",
    );
    md.push_str(&format!(
        "Records identified: {}. Duplicates removed: {}. Records screened: {}. Articles still in progress (not yet decided): {}. Records excluded: {}, of which {} were excluded generally without a matched criterion and {} were excluded with at least one matched criterion. Studies included: {}.\n\n",
        report.records_identified,
        report.duplicates_removed,
        report.records_screened,
        report.records_in_progress,
        report.records_excluded,
        report.records_excluded_general,
        report.records_excluded_with_reasons,
        report.studies_included
    ));
    md.push_str("How to read the tables: Tables 1 and 2 count each article exactly once under its single most significant reason. The most significant reason is the matched criterion with the highest priority (critical > high > standard > low > optional); when several matched criteria share the highest priority, the criterion assigned first wins, that is the earliest entry in the article's matched-criteria list. Tables 3 and 4 count every matched criterion, so an article that matched several criteria contributes to several rows and the row counts intentionally exceed the number of articles.\n\n");

    md.push_str(&format!("{section} Table 1: Primary Inclusion Reasons\n\n"));
    push_reason_table(
        &mut md,
        "Inclusion criterion",
        "inclusion",
        &report.primary_inclusion,
        report.general_inclusion_count,
        Some(report.studies_included),
    );
    md.push_str("Each included article is counted exactly once in this table, under its single most significant inclusion reason. Articles included without any matched inclusion criterion (for example manual inclusions), or whose matched criteria were deleted after screening, are grouped under \"General inclusion (no criterion matched)\". The total therefore equals the number of included studies.\n\n");

    md.push_str(&format!("{section} Table 2: Primary Exclusion Reasons\n\n"));
    push_reason_table(
        &mut md,
        "Exclusion criterion",
        "exclusion",
        &report.primary_exclusion,
        report.general_exclusion_count,
        Some(report.records_excluded),
    );
    md.push_str("Each rejected article is counted exactly once in this table, under its single most significant exclusion reason. Articles rejected without any matched exclusion criterion (for example manual user exclusions), or whose matched criteria were deleted after screening, are grouped under \"General exclusion (no criterion matched)\". The total therefore equals the number of excluded records.\n\n");

    md.push_str(&format!("{section} Table 3: Multi-Assignment Inclusion Counts\n\n"));
    push_reason_table(
        &mut md,
        "Inclusion criterion",
        "inclusion",
        &report.multi_inclusion,
        report.general_inclusion_count,
        None,
    );
    md.push_str("This table counts every inclusion criterion matched by each included article, so an article with several matched criteria appears in several rows. A row's count reads as \"N included articles matched this criterion\". The counts are intentionally not summed: the column total would exceed the number of included studies because articles are counted more than once. The general row counts included articles that matched no criterion.\n\n");

    md.push_str(&format!("{section} Table 4: Multi-Assignment Exclusion Counts\n\n"));
    push_reason_table(
        &mut md,
        "Exclusion criterion",
        "exclusion",
        &report.multi_exclusion,
        report.general_exclusion_count,
        None,
    );
    md.push_str("This table counts every exclusion criterion matched by each rejected article, so an article with several matched criteria appears in several rows. A row's count reads as \"N rejected articles matched this criterion\". The counts are intentionally not summed: the column total would exceed the number of excluded records because articles are counted more than once. The general row counts rejected articles that matched no criterion.\n\n");

    md
}

/// Compute the full report from the database. Included articles feed the
/// inclusion tables (via `matched_inclusion_criteria`), rejected articles the
/// exclusion tables (via `matched_exclusion_criteria`).
pub fn compute_prisma_report(conn: &Connection) -> Result<PrismaReport, AppError> {
    let data = data::compute_prisma_data(conn)?;
    let criteria = criteria_repo::get_all_criteria(conn)?;
    let included = article_repo::get_articles_by_status(conn, "included")?;
    let rejected = article_repo::get_articles_by_status(conn, "rejected")?;

    let inclusion_ids: Vec<&[String]> =
        included.iter().map(|a| a.matched_inclusion_criteria.as_slice()).collect();
    let exclusion_ids: Vec<&[String]> =
        rejected.iter().map(|a| a.matched_exclusion_criteria.as_slice()).collect();

    let (primary_inclusion, general_inclusion_count) = count_primary(&inclusion_ids, &criteria);
    let (primary_exclusion, general_exclusion_count) = count_primary(&exclusion_ids, &criteria);
    let multi_inclusion = count_multi(&inclusion_ids, &criteria);
    let multi_exclusion = count_multi(&exclusion_ids, &criteria);

    Ok(PrismaReport {
        generated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        project_name: app_settings_repo::get_project_name(conn)?,
        records_identified: data.records_identified,
        duplicates_removed: data.duplicates_removed,
        records_screened: data.records_screened,
        records_in_progress: data.records_in_progress,
        records_excluded: data.records_excluded,
        records_excluded_general: data.records_excluded_general,
        records_excluded_with_reasons: data.records_excluded_with_reasons,
        studies_included: data.studies_included,
        primary_inclusion,
        general_inclusion_count,
        multi_inclusion,
        primary_exclusion,
        general_exclusion_count,
        multi_exclusion,
    })
}
