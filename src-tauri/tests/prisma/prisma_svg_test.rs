//! Coverage for prisma::svg (SVG string generation + XML escaping).
use bango_lib::prisma::data::{ExclusionReason, PrismaData};
use bango_lib::prisma::svg::render_prisma_svg;

fn empty_data() -> PrismaData {
    PrismaData {
        records_identified: 0,
        duplicates_removed: 0,
        records_screened: 0,
        records_excluded: 0,
        records_excluded_general: 0,
        records_excluded_with_reasons: 0,
        records_assessed: 0,
        records_in_progress: 0,
        studies_included: 0,
        exclusion_reasons: vec![],
    }
}

#[test]
fn renders_basic_svg_structure_without_ongoing() {
    let mut data = empty_data();
    data.records_identified = 100;
    data.duplicates_removed = 10;
    data.records_screened = 90;
    data.records_excluded_general = 20;
    data.records_assessed = 70;
    data.studies_included = 50;

    let svg = render_prisma_svg(&data);

    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.ends_with("</svg>"));
    // Phase titles present
    assert!(svg.contains("Identification"));
    assert!(svg.contains("Screening"));
    assert!(svg.contains("Eligibility"));
    assert!(svg.contains("Included"));
    // Counts rendered
    assert!(svg.contains("n = 100"));
    assert!(svg.contains("n = 10"));
    assert!(svg.contains("n = 90"));
    assert!(svg.contains("n = 50"));
    // No ongoing phase when records_in_progress = 0
    assert!(!svg.contains("Ongoing"));
    // Included box is highlighted (stroke-width 2)
    assert!(svg.contains("stroke-width=\"2\""));
}

#[test]
fn renders_ongoing_phase_when_in_progress() {
    let mut data = empty_data();
    data.records_identified = 50;
    data.records_screened = 50;
    data.records_in_progress = 5;
    data.studies_included = 10;

    let svg = render_prisma_svg(&data);
    assert!(svg.contains("Ongoing"), "should render ongoing box");
    assert!(svg.contains("Articles in progress (n = 5)"));
}

#[test]
fn renders_exclusion_reasons() {
    let mut data = empty_data();
    data.records_identified = 10;
    data.records_screened = 10;
    data.records_excluded_with_reasons = 3;
    data.exclusion_reasons = vec![
        ExclusionReason {
            criterion_id: "c1".to_string(),
            criterion_text: "Wrong population".to_string(),
            count: 2,
        },
        ExclusionReason {
            criterion_id: "c2".to_string(),
            criterion_text: "Not a study".to_string(),
            count: 1,
        },
    ];

    let svg = render_prisma_svg(&data);
    assert!(svg.contains("Wrong population"));
    assert!(svg.contains("Not a study"));
    assert!(svg.contains("n=2"));
    assert!(svg.contains("n=1"));
    // Reasons box is taller when reasons exist
    assert!(svg.contains("Records excluded with reasons (n = 3)"));
}

#[test]
fn truncates_more_than_eight_exclusion_reasons() {
    let mut data = empty_data();
    data.records_screened = 100;
    data.exclusion_reasons = (0..10)
        .map(|i| ExclusionReason {
            criterion_id: format!("c{i}"),
            criterion_text: format!("Reason {i}"),
            count: i + 1,
        })
        .collect();

    let svg = render_prisma_svg(&data);
    // First 8 reasons rendered
    assert!(svg.contains("Reason 0"));
    assert!(svg.contains("Reason 7"));
    // 9th and 10th collapsed into "... and 2 more"
    assert!(svg.contains("and 2 more"));
    assert!(!svg.contains("Reason 9"));
}

#[test]
fn escapes_xml_special_characters() {
    let mut data = empty_data();
    data.records_screened = 1;
    data.exclusion_reasons = vec![ExclusionReason {
        criterion_id: "c1".to_string(),
        criterion_text: "A & B <C> \"quoted\"".to_string(),
        count: 1,
    }];

    let svg = render_prisma_svg(&data);
    // Build expected escaped entities from chars to avoid source-mangling by formatters.
    let amp_entity: String = ['&', 'a', 'm', 'p', ';'].iter().collect();
    let lt_entity: String = ['&', 'l', 't', ';'].iter().collect();
    let gt_entity: String = ['&', 'g', 't', ';'].iter().collect();
    let quot_entity: String = ['&', 'q', 'u', 'o', 't', ';'].iter().collect();
    assert!(svg.contains(&amp_entity), "ampersand should be escaped");
    assert!(svg.contains(&lt_entity), "less-than should be escaped");
    assert!(svg.contains(&gt_entity), "greater-than should be escaped");
    assert!(svg.contains(&quot_entity), "quote should be escaped");
    // The raw unescaped criterion text should NOT appear anywhere
    assert!(!svg.contains("A & B <C>"), "raw special chars must be escaped");
}

#[test]
fn empty_data_renders_valid_svg() {
    let data = empty_data();
    let svg = render_prisma_svg(&data);
    assert!(svg.starts_with("<svg"));
    assert!(svg.ends_with("</svg>"));
    // All zeros
    assert!(svg.contains("n = 0"));
}
