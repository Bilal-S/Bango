use tauri::State;

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::prisma::data::{self, PrismaData};
use crate::prisma::report;
use crate::prisma::svg;

fn render_svg(db_state: &State<'_, DbState>) -> Result<String, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let prisma_data = data::compute_prisma_data(&conn)?;
    Ok(svg::render_prisma_svg(&prisma_data))
}

#[tauri::command]
pub fn get_prisma_data(db_state: State<'_, DbState>) -> Result<PrismaData, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    data::compute_prisma_data(&conn)
}

#[tauri::command]
pub fn get_prisma_svg(db_state: State<'_, DbState>) -> Result<String, AppError> {
    render_svg(&db_state)
}

#[tauri::command]
pub fn export_prisma_svg_to_file(
    db_state: State<'_, DbState>,
    path: String,
) -> Result<(), AppError> {
    let svg_content = render_svg(&db_state)?;
    std::fs::write(path, svg_content).map_err(AppError::Io)
}

#[tauri::command]
pub fn export_prisma_png_to_file(
    db_state: State<'_, DbState>,
    path: String,
) -> Result<(), AppError> {
    let svg_content = render_svg(&db_state)?;

    // Load system fonts so text renders in the PNG output.
    // Must set generic family mappings because Linux lacks Arial/Times New Roman.
    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();
    fontdb.set_sans_serif_family("Liberation Sans");
    fontdb.set_serif_family("DejaVu Serif");
    fontdb.set_monospace_family("DejaVu Sans Mono");

    let opts = resvg::usvg::Options { fontdb: std::sync::Arc::new(fontdb), ..Default::default() };

    let tree = resvg::usvg::Tree::from_str(&svg_content, &opts)
        .map_err(|e| AppError::Rendering(format!("SVG parse error: {e}")))?;

    let size = tree.size();
    let scale = 2.0_f32;
    let pixmap_w = (size.width() * scale) as u32;
    let pixmap_h = (size.height() * scale) as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(pixmap_w, pixmap_h)
        .ok_or_else(|| AppError::Rendering("Failed to create pixmap".to_string()))?;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let png_data =
        pixmap.encode_png().map_err(|e| AppError::Rendering(format!("PNG encode error: {e}")))?;

    std::fs::write(path, png_data).map_err(AppError::Io)
}

/// Screening reasons report as Markdown (four tables + explanatory text).
/// The frontend saves it via `write_text_to_file` (Markdown) or renders it
/// to HTML and prints it (PDF via the webview print dialog).
#[tauri::command]
pub fn get_prisma_report_markdown(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let report = report::compute_prisma_report(&conn)?;
    Ok(report::render_prisma_report_markdown(&report))
}
