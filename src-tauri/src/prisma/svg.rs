use super::data::PrismaData;

#[must_use]
pub fn render_prisma_svg(data: &PrismaData) -> String {
    let width = 600;
    let box_w = 280;
    let box_h = 50;
    let x_center = width / 2;
    let x_box = x_center - box_w / 2;

    let mut y = 40;

    // Phase 1: Identification
    let identification_svg = render_box(
        x_box,
        y,
        box_w,
        box_h,
        &format!("Records identified (n = {})", data.records_identified),
    );
    y += box_h + 15;
    let arrow1 = render_arrow(x_center, y - 15, x_center, y);
    let dup_svg = render_side_box(
        x_box + box_w + 20,
        y - box_h - 15,
        200,
        box_h,
        &format!("Duplicates removed (n = {})", data.duplicates_removed),
    );
    y += 15;

    // Phase 2: Screening
    let screening_svg = render_box(
        x_box,
        y,
        box_w,
        box_h,
        &format!("Records screened (n = {})", data.records_screened),
    );
    y += box_h + 15;
    let arrow2 = render_arrow(x_center, y - 15, x_center, y);
    let excluded_svg = render_side_box(
        x_box + box_w + 20,
        y - box_h - 15,
        200,
        box_h,
        &format!("Records excluded (n = {})", data.records_excluded),
    );
    y += 15;

    // Phase 3: Eligibility
    let eligibility_svg = render_box(
        x_box,
        y,
        box_w,
        box_h,
        &format!("Articles assessed (n = {})", data.records_screened),
    );
    y += box_h + 15;
    let arrow3 = render_arrow(x_center, y - 15, x_center, y);
    y += 15;

    // Phase 4: Included
    let included_svg = render_box(
        x_box,
        y,
        box_w,
        box_h,
        &format!("Studies included (n = {})", data.studies_included),
    );

    let height = y + box_h + 40;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" font-family=\"Inter, system-ui, sans-serif\">\n"
    ));
    svg.push_str(&format!("  <rect width=\"{width}\" height=\"{height}\" fill=\"#ffffff\"/>\n"));
    svg.push_str(&identification_svg);
    svg.push_str(&arrow1);
    svg.push_str(&dup_svg);
    svg.push_str(&screening_svg);
    svg.push_str(&arrow2);
    svg.push_str(&excluded_svg);
    svg.push_str(&eligibility_svg);
    svg.push_str(&arrow3);
    svg.push_str(&included_svg);
    svg.push_str("</svg>");
    svg
}

fn render_box(x: i32, y: i32, w: i32, h: i32, text: &str) -> String {
    let text_x = x + w / 2;
    let text_y = y + h / 2 + 5;
    format!(
        "  <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"8\" fill=\"#f0ecf9\" stroke=\"#4f46e5\" stroke-width=\"1.5\"/>\n  <text x=\"{text_x}\" y=\"{text_y}\" text-anchor=\"middle\" font-size=\"13\" font-weight=\"600\" fill=\"#1b1b24\">{text}</text>\n",
        text = escape_xml(text),
    )
}

fn render_side_box(x: i32, y: i32, w: i32, h: i32, text: &str) -> String {
    let text_x = x + w / 2;
    let text_y = y + h / 2 + 5;
    format!(
        "  <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"8\" fill=\"#fee2e2\" stroke=\"#ef4444\" stroke-width=\"1\"/>\n  <text x=\"{text_x}\" y=\"{text_y}\" text-anchor=\"middle\" font-size=\"12\" fill=\"#991b1b\">{text}</text>\n",
        text = escape_xml(text),
    )
}

fn render_arrow(x1: i32, y1: i32, x2: i32, y2: i32) -> String {
    let px_minus = x2 - 4;
    let px_plus = x2 + 4;
    let py = y2 - 8;
    format!(
        "  <line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"#777587\" stroke-width=\"1.5\"/>\n  <polygon points=\"{x2},{y2} {px_minus},{py} {px_plus},{py}\" fill=\"#777587\"/>\n",
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
