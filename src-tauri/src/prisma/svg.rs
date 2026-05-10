use super::data::PrismaData;

const WIDTH: i32 = 800;
const BOX_W: i32 = 280;
const BOX_H: i32 = 56;
const SIDE_W: i32 = 220;
const SIDE_H: i32 = 50;
const GAP: i32 = 16;

/// Design-system-aligned colors matching the Tailwind `@theme` tokens in `base.css`.
mod colors {
    pub const MAIN_FILL: &str = "#ffffff"; // surface-container-lowest
    pub const MAIN_STROKE: &str = "#c7c4d8"; // outline-variant
    pub const SIDE_FILL: &str = "#f5f2ff"; // surface-container-low
    pub const SIDE_STROKE: &str = "#c7c4d8"; // outline-variant
    pub const INCLUDED_FILL: &str = "#e2dfff"; // primary-fixed
    pub const INCLUDED_STROKE: &str = "#c3c0ff"; // primary-fixed-dim
    pub const CONNECTOR: &str = "#c7c4d8"; // outline-variant
    pub const TEXT_PRIMARY: &str = "#1b1b24"; // on-surface
    pub const TEXT_SECONDARY: &str = "#464555"; // on-surface-variant
    pub const TEXT_INCLUDED: &str = "#0f0069"; // on-primary-fixed
    pub const TEXT_INCLUDED_DESC: &str = "#3323cc"; // on-primary-fixed-variant
}

/// Style parameters for a phase box, grouped to avoid too-many-arguments.
struct PhaseStyle {
    fill: &'static str,
    stroke: &'static str,
    title_color: &'static str,
    desc_color: &'static str,
}

const MAIN_STYLE: PhaseStyle = PhaseStyle {
    fill: colors::MAIN_FILL,
    stroke: colors::MAIN_STROKE,
    title_color: colors::TEXT_PRIMARY,
    desc_color: colors::TEXT_SECONDARY,
};

const INCLUDED_STYLE: PhaseStyle = PhaseStyle {
    fill: colors::INCLUDED_FILL,
    stroke: colors::INCLUDED_STROKE,
    title_color: colors::TEXT_INCLUDED,
    desc_color: colors::TEXT_INCLUDED_DESC,
};

const ONGOING_STYLE: PhaseStyle = PhaseStyle {
    fill: colors::SIDE_FILL,
    stroke: colors::SIDE_STROKE,
    title_color: colors::TEXT_SECONDARY,
    desc_color: colors::TEXT_SECONDARY,
};

#[must_use]
pub fn render_prisma_svg(data: &PrismaData) -> String {
    let x_center = WIDTH / 2;
    let x_box = x_center - BOX_W / 2;
    let side_x = x_box + BOX_W + GAP + 8;

    let mut y = 20;

    // Phase 1: Identification
    let identification = render_phase_box(
        x_box,
        y,
        BOX_W,
        BOX_H,
        "Identification",
        &format!("Records identified from databases (n = {})", data.records_identified),
        &MAIN_STYLE,
        false,
    );
    let arrow1_start = y + BOX_H;
    y += BOX_H + GAP;
    let mid1 = arrow1_start + (y - arrow1_start) / 2;
    let arrow1 = render_arrow(x_center, arrow1_start, x_center, y);
    let h_line1 = render_horizontal_line(x_box + BOX_W, mid1, side_x);
    let arrow1_side = render_arrow_right(x_box + BOX_W, mid1, side_x);
    let dup_box = render_side_box(
        side_x,
        mid1 - SIDE_H / 2,
        SIDE_W,
        SIDE_H,
        &format!("Duplicates removed (n = {})", data.duplicates_removed),
    );

    // Phase 2: Screening
    let screening = render_phase_box(
        x_box,
        y,
        BOX_W,
        BOX_H,
        "Screening",
        &format!("Records screened (n = {})", data.records_screened),
        &MAIN_STYLE,
        false,
    );
    let arrow2_start = y + BOX_H;
    y += BOX_H + GAP;
    let mid2 = arrow2_start + (y - arrow2_start) / 2;
    let arrow2 = render_arrow(x_center, arrow2_start, x_center, y);
    let h_line2 = render_horizontal_line(x_box + BOX_W, mid2, side_x);
    let arrow2_side = render_arrow_right(x_box + BOX_W, mid2, side_x);
    let excluded_box = render_side_box(
        side_x,
        mid2 - SIDE_H / 2,
        SIDE_W,
        SIDE_H,
        &format!("Records generally excluded (n = {})", data.records_excluded_general),
    );

    // Phase 3: Eligibility
    let eligibility = render_phase_box(
        x_box,
        y,
        BOX_W,
        BOX_H,
        "Eligibility",
        &format!("Full-text articles assessed (n = {})", data.records_assessed),
        &MAIN_STYLE,
        false,
    );
    let arrow3_start = y + BOX_H;
    y += BOX_H + GAP;
    let mid3 = arrow3_start + (y - arrow3_start) / 2;
    let arrow3 = render_arrow(x_center, arrow3_start, x_center, y);
    let h_line3 = render_horizontal_line(x_box + BOX_W, mid3, side_x);
    let arrow3_side = render_arrow_right(x_box + BOX_W, mid3, side_x);

    // Exclusion reasons side box (taller if reasons exist)
    let reasons_h = if data.exclusion_reasons.is_empty() {
        SIDE_H
    } else {
        SIDE_H + (data.exclusion_reasons.len() as i32) * 18
    };
    let reasons_box = render_reasons_side_box(
        side_x,
        mid3 - reasons_h / 2,
        SIDE_W,
        reasons_h,
        &format!("Records excluded with reasons (n = {})", data.records_excluded_with_reasons),
        &data.exclusion_reasons,
    );

    // Phase 4.5: Ongoing (in progress) — only if > 0
    let ongoing_svg = if data.records_in_progress > 0 {
        let arrow4_start = y + BOX_H;
        y += BOX_H + GAP;
        let arrow4 = render_arrow(x_center, arrow4_start, x_center, y);
        let ongoing = render_phase_box(
            x_box,
            y,
            BOX_W,
            BOX_H,
            "Ongoing",
            &format!("Articles in progress (n = {})", data.records_in_progress),
            &ONGOING_STYLE,
            false,
        );
        let arrow5_start = y + BOX_H;
        y += BOX_H + GAP;
        let arrow5 = render_arrow(x_center, arrow5_start, x_center, y);
        format!("{arrow4}{ongoing}{arrow5}")
    } else {
        String::new()
    };

    // Phase 5: Included (always last)
    let included = render_phase_box(
        x_box,
        y,
        BOX_W,
        BOX_H,
        "Included",
        &format!("Studies included in review (n = {})", data.studies_included),
        &INCLUDED_STYLE,
        true,
    );

    let height = y + BOX_H + 40;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{height}\" viewBox=\"0 0 {WIDTH} {height}\" font-family=\"Inter, system-ui, -apple-system, sans-serif\">\n"
    ));
    svg.push_str(&format!("  <rect width=\"{WIDTH}\" height=\"{height}\" fill=\"#ffffff\"/>\n"));

    // Phase 1
    svg.push_str(&identification);
    svg.push_str(&arrow1);
    svg.push_str(&h_line1);
    svg.push_str(&arrow1_side);
    svg.push_str(&dup_box);

    // Phase 2
    svg.push_str(&screening);
    svg.push_str(&arrow2);
    svg.push_str(&h_line2);
    svg.push_str(&arrow2_side);
    svg.push_str(&excluded_box);

    // Phase 3
    svg.push_str(&eligibility);
    svg.push_str(&arrow3);
    svg.push_str(&h_line3);
    svg.push_str(&arrow3_side);
    svg.push_str(&reasons_box);

    // Ongoing (conditional)
    svg.push_str(&ongoing_svg);

    // Phase 4
    svg.push_str(&included);

    svg.push_str("</svg>");
    svg
}

#[allow(clippy::too_many_arguments)]
fn render_phase_box(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    title: &str,
    desc: &str,
    style: &PhaseStyle,
    highlight: bool,
) -> String {
    let text_x = x + w / 2;
    let title_y = y + 22;
    let desc_y = y + 40;
    let stroke_w = if highlight { "2" } else { "1.5" };
    format!(
        "  <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"8\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{stroke_w}\"/>\n  <text x=\"{text_x}\" y=\"{title_y}\" text-anchor=\"middle\" font-size=\"13\" font-weight=\"600\" fill=\"{}\">{}</text>\n  <text x=\"{text_x}\" y=\"{desc_y}\" text-anchor=\"middle\" font-size=\"12\" fill=\"{}\">{}</text>\n",
        style.fill,
        style.stroke,
        style.title_color,
        escape_xml(title),
        style.desc_color,
        escape_xml(desc),
    )
}

fn render_side_box(x: i32, y: i32, w: i32, h: i32, text: &str) -> String {
    let text_x = x + w / 2;
    let text_y = y + h / 2 + 4;
    format!(
        "  <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"8\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\" stroke-dasharray=\"6 3\"/>\n  <text x=\"{text_x}\" y=\"{text_y}\" text-anchor=\"middle\" font-size=\"12\" fill=\"{}\">{}</text>\n",
        colors::SIDE_FILL,
        colors::SIDE_STROKE,
        colors::TEXT_SECONDARY,
        escape_xml(text),
    )
}

fn render_reasons_side_box(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    header: &str,
    reasons: &[super::data::ExclusionReason],
) -> String {
    let text_x = x + w / 2;
    let header_y = y + 20;
    let mut s = format!(
        "  <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"8\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\" stroke-dasharray=\"6 3\"/>\n  <text x=\"{text_x}\" y=\"{header_y}\" text-anchor=\"middle\" font-size=\"12\" fill=\"{}\">{}</text>\n",
        colors::SIDE_FILL,
        colors::SIDE_STROKE,
        colors::TEXT_SECONDARY,
        escape_xml(header),
    );
    // Render up to 8 exclusion reasons
    for (i, reason) in reasons.iter().take(8).enumerate() {
        let ry = header_y + 16 + (i as i32) * 18;
        s.push_str(&format!(
            "  <text x=\"{}\" y=\"{ry}\" font-size=\"11\" fill=\"{}\">\u{2022} {} (n={})</text>\n",
            x + 12,
            colors::TEXT_SECONDARY,
            escape_xml(&reason.criterion_text),
            reason.count,
        ));
    }
    if reasons.len() > 8 {
        let ry = header_y + 16 + 8 * 18;
        s.push_str(&format!(
            "  <text x=\"{}\" y=\"{ry}\" font-size=\"11\" fill=\"{}\">\u{2026} and {} more</text>\n",
            x + 12,
            colors::TEXT_SECONDARY,
            reasons.len() - 8,
        ));
    }
    s
}

fn render_arrow(x1: i32, y1: i32, x2: i32, y2: i32) -> String {
    let px_minus = x2 - 4;
    let px_plus = x2 + 4;
    let py = y2 - 8;
    format!(
        "  <line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"{}\" stroke-width=\"1.5\"/>\n  <polygon points=\"{x2},{y2} {px_minus},{py} {px_plus},{py}\" fill=\"{}\"/>\n",
        colors::CONNECTOR,
        colors::CONNECTOR,
    )
}

fn render_horizontal_line(x1: i32, y: i32, x2: i32) -> String {
    format!(
        "  <line x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\" stroke=\"{}\" stroke-width=\"1\"/>\n",
        colors::CONNECTOR,
    )
}

fn render_arrow_right(x1: i32, y: i32, x2: i32) -> String {
    let px_minus = y - 4;
    let px_plus = y + 4;
    let px = x2 - 8;
    format!(
        "  <line x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\" stroke=\"{}\" stroke-width=\"1\"/>\n  <polygon points=\"{x2},{y} {px},{px_minus} {px},{px_plus}\" fill=\"{}\"/>\n",
        colors::CONNECTOR,
        colors::CONNECTOR,
    )
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("\u{0026}amp;"),
            '<' => out.push_str("\u{0026}lt;"),
            '>' => out.push_str("\u{0026}gt;"),
            '"' => out.push_str("\u{0026}quot;"),
            _ => out.push(ch),
        }
    }
    out
}
