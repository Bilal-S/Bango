//! PRISMA screening reasons report: primary-reason attribution, general
//! buckets, multi-assignment counts, and Markdown rendering.

use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::criterion::{Criterion, CriterionType, Priority};
use bango_lib::prisma::report::{
    compute_prisma_report, count_multi, count_primary, primary_reason,
    render_prisma_report_markdown,
};
use rusqlite::{params, Connection};

fn setup() -> Connection {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

fn criterion(id: &str, t: &str, text: &str, priority: Priority) -> Criterion {
    Criterion {
        id: id.to_string(),
        criterion_type: if t == "inclusion" {
            CriterionType::Inclusion
        } else {
            CriterionType::Exclusion
        },
        text: text.to_string(),
        priority,
        created_at: "2026-01-01T00:00:00+00:00".to_string(),
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

fn insert_criterion(conn: &Connection, id: &str, t: &str, text: &str, priority: &str) {
    conn.execute(
        "INSERT INTO criteria (id, type, text, priority, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, t, text, priority, "2026-01-01T00:00:00+00:00"],
    )
    .unwrap();
}

fn insert_article(conn: &Connection, id: &str, status: &str, inc: &str, exc: &str) {
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, matched_inclusion_criteria, matched_exclusion_criteria) VALUES (?1, ?2, 'T', 'A', '[]', ?3, ?4)",
        params![id, status, inc, exc],
    )
    .unwrap();
}

#[test]
fn primary_reason_prefers_highest_priority_then_first_assigned() {
    let criteria = vec![
        criterion("i1", "inclusion", "Standard study", Priority::Standard),
        criterion("i2", "inclusion", "Critical study", Priority::Critical),
        criterion("i3", "inclusion", "Also critical", Priority::Critical),
    ];
    // Higher priority beats an earlier position.
    let matched = ids(&["i1", "i2"]);
    assert_eq!(primary_reason(&matched, &criteria).map(|c| c.id.as_str()), Some("i2"));
    // Equal priority: earliest position (first assigned) wins.
    let matched = ids(&["i3", "i2"]);
    assert_eq!(primary_reason(&matched, &criteria).map(|c| c.id.as_str()), Some("i3"));
    let matched = ids(&["i2", "i3"]);
    assert_eq!(primary_reason(&matched, &criteria).map(|c| c.id.as_str()), Some("i2"));
    // Unresolvable ids never win; all-unresolvable is General.
    let matched = ids(&["ghost"]);
    assert!(primary_reason(&matched, &criteria).is_none());
    assert!(primary_reason(&[], &criteria).is_none());
}

#[test]
fn count_primary_and_multi_assignment_semantics() {
    let criteria = vec![
        criterion("e1", "exclusion", "Wrong population", Priority::High),
        criterion("e2", "exclusion", "Wrong outcome", Priority::Standard),
    ];
    let articles = [ids(&["e1", "e2"]), ids(&["e1"]), ids(&[]), ids(&["ghost"])];
    let slices: Vec<&[String]> = articles.iter().map(|a| a.as_slice()).collect();

    let (primary, general) = count_primary(&slices, &criteria, false);
    assert_eq!(general, 2); // no criteria + deleted-criterion-only article
    assert_eq!(primary.len(), 1);
    assert_eq!(primary[0].criterion_id, "e1");
    assert_eq!(primary[0].count, 2); // article 1 (e1 high beats e2) + article 2

    let multi = count_multi(&slices, &criteria, false);
    // Multi counts each matched criterion: e1 x2, e2 x1, ghost x1 (kept, dangling).
    let e1 = multi.iter().find(|r| r.criterion_id == "e1").unwrap();
    let e2 = multi.iter().find(|r| r.criterion_id == "e2").unwrap();
    let ghost = multi.iter().find(|r| r.criterion_id == "ghost").unwrap();
    assert_eq!((e1.count, e2.count, ghost.count), (2, 1, 1));
    assert_eq!(ghost.priority, None);
    assert!(ghost.criterion_text.contains("Deleted criterion"));
}

#[test]
fn failed_inclusion_ids_render_as_not_met_in_exclusion_tables() {
    let conn = setup();
    insert_criterion(&conn, "i1", "inclusion", "Must be human study", "critical");
    insert_criterion(&conn, "e1", "exclusion", "Animal study", "high");

    // Rejected article whose exclusion array mixes a violated exclusion
    // criterion and a failed inclusion criterion (implicit cross-type storage).
    insert_article(&conn, "a1", "rejected", "[]", r#"["i1", "e1"]"#);
    // Pure exclusion-criterion rejection for contrast.
    insert_article(&conn, "a2", "rejected", "[]", r#"["e1"]"#);

    let report = compute_prisma_report(&conn).unwrap();
    // The critical failed inclusion outranks the high violated exclusion, so
    // it wins primary attribution for a1 and carries the NOT MET prefix.
    let not_met = report.primary_exclusion.iter().find(|r| r.criterion_id == "i1").unwrap();
    assert_eq!(not_met.criterion_text, "NOT MET: Must be human study");
    let violated = report.primary_exclusion.iter().find(|r| r.criterion_id == "e1").unwrap();
    assert_eq!(violated.criterion_text, "Animal study");

    let not_met_multi = report.multi_exclusion.iter().find(|r| r.criterion_id == "i1").unwrap();
    assert_eq!(not_met_multi.criterion_text, "NOT MET: Must be human study");

    // Inclusion tables never carry the prefix.
    insert_article(&conn, "a3", "included", r#"["i1"]"#, "[]");
    let report = compute_prisma_report(&conn).unwrap();
    let inc_row = report.primary_inclusion.iter().find(|r| r.criterion_id == "i1").unwrap();
    assert_eq!(inc_row.criterion_text, "Must be human study");

    let md = render_prisma_report_markdown(&report);
    assert!(md.contains("NOT MET: Must be human study"));
    assert!(md.contains("Rows prefixed \"NOT MET:\""));
}

#[test]
fn report_totals_match_status_counts_with_general_rows() {
    let conn = setup();
    insert_criterion(&conn, "i1", "inclusion", "UK study", "critical");
    insert_criterion(&conn, "e1", "exclusion", "Not English", "high");

    insert_article(&conn, "a1", "included", r#"["i1"]"#, "[]");
    insert_article(&conn, "a2", "included", "[]", "[]"); // manual inclusion
    insert_article(&conn, "a3", "rejected", "[]", r#"["e1"]"#);
    insert_article(&conn, "a4", "rejected", "[]", "[]"); // manual exclusion
    insert_article(&conn, "a5", "working", "[]", "[]"); // not counted anywhere

    let report = compute_prisma_report(&conn).unwrap();
    assert_eq!(report.studies_included, 2);
    assert_eq!(report.records_excluded, 2);
    assert_eq!(report.general_inclusion_count, 1);
    assert_eq!(report.general_exclusion_count, 1);
    let primary_inc: usize = report.primary_inclusion.iter().map(|r| r.count).sum();
    let primary_exc: usize = report.primary_exclusion.iter().map(|r| r.count).sum();
    assert_eq!(primary_inc + report.general_inclusion_count, 2);
    assert_eq!(primary_exc + report.general_exclusion_count, 2);

    let md = render_prisma_report_markdown(&report);
    assert!(md.contains("| - | General inclusion (no criterion matched) | 1 |"));
    assert!(md.contains("| - | General exclusion (no criterion matched) | 1 |"));
    // Exactly two total rows (primary tables); multi tables carry none.
    assert_eq!(md.matches("**Total**").count(), 2);
    assert!(md.contains("| **Total** |  | **2** |"));
}

#[test]
fn markdown_renders_four_tables_and_escapes_cell_text() {
    let conn = setup();
    insert_criterion(&conn, "i1", "inclusion", "UK | Europe <study>", "standard");
    insert_article(&conn, "a1", "included", r#"["i1"]"#, "[]");

    let report = compute_prisma_report(&conn).unwrap();
    let md = render_prisma_report_markdown(&report);
    for heading in [
        "## Overview",
        "## Table 1: Primary Inclusion Reasons",
        "## Table 2: Primary Exclusion Reasons",
        "## Table 3: Multi-Assignment Inclusion Counts",
        "## Table 4: Multi-Assignment Exclusion Counts",
    ] {
        assert!(md.contains(heading), "missing heading: {heading}");
    }
    assert!(md.contains("UK \\| Europe &lt;study&gt;"), "cell not escaped: {md}");
    assert!(md.contains("| standard |"));
    // Descriptive text under every table.
    assert_eq!(md.matches("counted exactly once").count(), 2);
    assert!(md.contains("intentionally not summed"));
    // No emdash in generated text (project rule).
    assert!(!md.contains('\u{2014}'));
}

#[test]
fn markdown_uses_project_name_heading_when_set() {
    let conn = setup();
    bango_lib::db::app_settings_repo::set_project_name(&conn, "Trust in Blockchain Review")
        .unwrap();
    insert_criterion(&conn, "i1", "inclusion", "UK study", "critical");
    insert_article(&conn, "a1", "included", r#"["i1"]"#, "[]");

    let report = compute_prisma_report(&conn).unwrap();
    assert_eq!(report.project_name.as_deref(), Some("Trust in Blockchain Review"));
    let md = render_prisma_report_markdown(&report);
    assert!(md.starts_with("# Trust in Blockchain Review\n\n## PRISMA Screening Reasons Report\n"));
    // Sections nest under the report title (h3), not beside it.
    assert!(md.contains("### Overview"));
    assert!(md.contains("### Table 1: Primary Inclusion Reasons"));
    assert!(md.contains("### Table 4: Multi-Assignment Exclusion Counts"));
    assert!(!md.contains("\n## Table"));
}

#[test]
fn markdown_default_title_without_project_name() {
    let conn = setup();
    insert_article(&conn, "a1", "included", "[]", "[]");

    let report = compute_prisma_report(&conn).unwrap();
    assert_eq!(report.project_name, None);
    let md = render_prisma_report_markdown(&report);
    assert!(md.starts_with("# PRISMA Screening Reasons Report\n"));
    assert!(md.contains("## Overview"));
    assert!(md.contains("## Table 1: Primary Inclusion Reasons"));
    assert!(!md.contains("###"));
}
