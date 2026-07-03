use std::collections::HashMap;
use std::path::Path;

use lopdf::Document as LopdfDocument;

/// Maximum number of words allowed in extracted full text.
const MAX_WORDS: usize = 30_000;

/// Extracts clean full text from a PDF file.
///
/// Pipeline:
/// 1. Extract text page-by-page using lopdf for header/footer detection
/// 2. Detect repeating header/footer lines across pages
/// 3. Extract full text using `unpdf` (best quality, handles custom font encodings)
///    - Falls back to `pdf-extract` if `unpdf` fails (legacy compat)
///    - Falls back to `lopdf` page-by-page if both fail (degraded but always works)
/// 4. Remove detected headers/footers from the extracted text
/// 5. Strip abstract and references sections
/// 6. Truncate to MAX_WORDS
pub fn extract_pdf_text(file_path: &Path) -> Result<String, String> {
    // Step 1: Detect headers/footers from lopdf (also pre-loads pages for fallback)
    let header_footer_lines = detect_headers_footers(file_path)?;

    // Step 2: Extract full text.
    // Tier 1: unpdf (best quality — handles Type1 custom fonts without panicking)
    // Tier 2: pdf-extract (legacy fallback — may panic on broken Unicode maps)
    // Tier 3: lopdf page-by-page (last resort — degraded but always works)
    let raw_text = match extract_via_unpdf(file_path) {
        Ok(text) if !text.trim().is_empty() => text,
        _ => {
            // unpdf failed or returned empty — try pdf-extract with panic safety.
            match extract_text_safe(file_path) {
                Ok(text) => text,
                Err(e) => {
                    // pdf-extract also failed — fall back to lopdf.
                    eprintln!(
                        "[pdf_extract] pdf-extract failed/panicked: {e} - falling back to lopdf extraction"
                    );
                    let doc = LopdfDocument::load(file_path)
                        .map_err(|e2| format!("Fallback PDF load also failed: {e2}"))?;
                    extract_all_pages_text(&doc)?
                }
            }
        }
    };

    // Step 3: Remove detected header/footer lines
    let cleaned = remove_header_footer_lines(&raw_text, &header_footer_lines);

    // Step 4: Strip abstract and references
    let stripped = strip_abstract_and_references(&cleaned);

    // Step 5: Truncate to word limit
    let truncated = truncate_to_word_limit(&stripped, MAX_WORDS);

    Ok(truncated)
}

/// Call `pdf_extract::extract_text` wrapped in `catch_unwind` so panics
/// (e.g. FromUtf16Error on malformed font maps) are converted to `Err`.
fn extract_text_safe(file_path: &Path) -> Result<String, String> {
    std::panic::catch_unwind(|| pdf_extract::extract_text(file_path))
        .map_err(|_| "PDF extraction panicked - the PDF may contain unsupported fonts".to_string())?
        .map_err(|e| format!("PDF extraction failed: {e}"))
}

/// Extract text via `unpdf` — the highest-quality PDF text extractor.
///
/// `unpdf` handles custom Type1 font encodings that cause `pdf-extract` to
/// panic and `lopdf` to produce garbled output. Pure Rust, MIT-licensed,
/// no external dependencies. Synchronous API (no async runtime needed).
fn extract_via_unpdf(file_path: &Path) -> Result<String, String> {
    let doc = unpdf::parse_file(file_path).map_err(|e| format!("unpdf parse failed: {e}"))?;
    let options = unpdf::render::RenderOptions::default();
    unpdf::render::to_text(&doc, &options).map_err(|e| format!("unpdf text extraction failed: {e}"))
}

/// Fallback: extract text from all pages using lopdf (used when pdf-extract fails).
fn extract_all_pages_text(doc: &LopdfDocument) -> Result<String, String> {
    let pages = doc.get_pages();
    let mut full_text = String::new();
    for &page_num in pages.keys() {
        if let Ok(page_text) = extract_page_text(doc, page_num) {
            full_text.push_str(&page_text);
            full_text.push('\n');
        }
    }
    if full_text.trim().is_empty() {
        return Err("lopdf fallback extracted no text from PDF".to_string());
    }
    Ok(full_text)
}

/// Reads plain text from a .txt file, strips abstract/references, truncates.
pub fn extract_txt_text(content: &str) -> String {
    let stripped = strip_abstract_and_references(content);
    truncate_to_word_limit(&stripped, MAX_WORDS)
}

/// Detect header/footer lines by extracting text page-by-page using lopdf.
/// Returns a set of lines that appear on >50% of pages at the top or bottom.
fn detect_headers_footers(file_path: &Path) -> Result<Vec<String>, String> {
    let doc = LopdfDocument::load(file_path).map_err(|e| format!("Failed to load PDF: {e}"))?;

    let pages = doc.get_pages();
    let num_pages = pages.len();
    if num_pages < 3 {
        // Not enough pages to reliably detect headers/footers
        return Ok(Vec::new());
    }

    let mut top_line_counts: HashMap<String, usize> = HashMap::new();
    let mut bottom_line_counts: HashMap<String, usize> = HashMap::new();

    for &page_num in pages.keys() {
        if let Ok(page_text) = extract_page_text(&doc, page_num) {
            let lines: Vec<&str> = page_text.lines().collect();
            if lines.is_empty() {
                continue;
            }

            // Collect top 3 non-empty lines
            let top_lines: Vec<&str> =
                lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).take(3).collect();

            // Collect bottom 3 non-empty lines
            let bottom_lines: Vec<&str> =
                lines.iter().rev().map(|l| l.trim()).filter(|l| !l.is_empty()).take(3).collect();

            for line in &top_lines {
                *top_line_counts.entry(normalize_line(line)).or_insert(0) += 1;
            }
            for line in &bottom_lines {
                *bottom_line_counts.entry(normalize_line(line)).or_insert(0) += 1;
            }
        }
    }

    let threshold = (num_pages as f64 * 0.5).ceil() as usize;
    let mut result = Vec::new();

    for (line, count) in &top_line_counts {
        if *count >= threshold && !is_page_number(line) {
            result.push(line.clone());
        }
    }

    for (line, count) in &bottom_line_counts {
        if *count >= threshold && !is_page_number(line) {
            result.push(line.clone());
        }
    }

    // Also add common page number patterns (these are always removed)
    result.push("__PAGE_NUMBER__".to_string());

    Ok(result)
}

/// Extract text from a single page using lopdf.
fn extract_page_text(doc: &LopdfDocument, page_num: u32) -> Result<String, String> {
    let page_id = doc
        .get_pages()
        .get(&page_num)
        .copied()
        .ok_or_else(|| format!("Page {page_num} not found"))?;

    let objects = doc.get_page_contents(page_id);

    let mut text = String::new();
    for obj_id in objects {
        if let Ok(object) = doc.get_object(obj_id) {
            if let Ok(stream) = object.as_stream() {
                if let Ok(content) = stream.decompressed_content() {
                    let content_str = String::from_utf8_lossy(&content);
                    let page_text = parse_text_operations(&content_str);
                    text.push_str(&page_text);
                }
            }
        }
    }

    Ok(text)
}

/// Basic text extraction from PDF content stream.
/// This is a simplified parser that captures text between BT...ET operators.
fn parse_text_operations(content: &str) -> String {
    let mut text = String::new();
    let mut in_text_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "BT" {
            in_text_block = true;
            continue;
        }
        if trimmed == "ET" {
            in_text_block = false;
            continue;
        }

        if in_text_block {
            // Look for Tj and TJ operators
            if trimmed.ends_with("Tj") {
                // Format: (text) Tj
                if let Some(t) = extract_parenthesis_text(trimmed) {
                    text.push_str(&t);
                    text.push(' ');
                }
            } else if trimmed.ends_with("TJ") {
                // Format: [(text) num (text)] TJ
                if let Some(t) = extract_array_text(trimmed) {
                    text.push_str(&t);
                    text.push(' ');
                }
            }
        }
    }

    text
}

/// Extract text from a Tj operation: `(Hello World) Tj` → `Hello World`
fn extract_parenthesis_text(op: &str) -> Option<String> {
    let s = op.strip_suffix("Tj")?.trim();
    if s.starts_with('(') && s.ends_with(')') {
        Some(s[1..s.len() - 1].to_string())
    } else if s.starts_with('<') && s.ends_with('>') {
        // Hex string - decode
        Some(decode_hex_string(&s[1..s.len() - 1]))
    } else {
        None
    }
}

/// Extract text from a TJ operation: `[(Hello) 10 (World)] TJ` → `Hello World`
fn extract_array_text(op: &str) -> Option<String> {
    let s = op.strip_suffix("TJ")?.trim();
    let s = s.strip_prefix('[')?.strip_suffix(']')?;

    let mut result = String::new();
    let mut current = String::new();
    let mut in_parens = false;
    let mut paren_depth = 0;

    for ch in s.chars() {
        match ch {
            '(' if !in_parens => {
                in_parens = true;
                paren_depth = 1;
            }
            '(' if in_parens => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' if in_parens => {
                paren_depth -= 1;
                if paren_depth == 0 {
                    in_parens = false;
                    result.push_str(&current);
                    current.clear();
                } else {
                    current.push(ch);
                }
            }
            _ if in_parens => {
                current.push(ch);
            }
            _ => {
                // Non-text element (number), ignore
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Decode a hex string from a PDF hex string.
fn decode_hex_string(hex: &str) -> String {
    let hex = hex.replace([' ', '\n', '\r'], "");
    (0..hex.len())
        .step_by(2)
        .filter_map(|i| {
            let byte = u8::from_str_radix(&hex[i..i + 2], 0o10).ok()?;
            Some(byte as char)
        })
        .collect()
}

/// Normalize a line for comparison (lowercase, collapse whitespace).
#[must_use]
pub fn normalize_line(line: &str) -> String {
    line.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Check if a normalized line is just a page number.
#[must_use]
pub fn is_page_number(line: &str) -> bool {
    let trimmed = line.trim();
    // Pure number
    if trimmed.parse::<u32>().is_ok() {
        return true;
    }
    // Patterns like "- 3 -", "page 4", "p. 5"
    let patterns = [
        |l: &str| l.starts_with("page ") && l[5..].trim().parse::<u32>().is_ok(),
        |l: &str| l.starts_with("p. ") && l[3..].trim().parse::<u32>().is_ok(),
        |l: &str| {
            l.starts_with('-')
                && l.ends_with('-')
                && l.trim_matches('-').trim().parse::<u32>().is_ok()
        },
    ];
    patterns.iter().any(|p| p(trimmed))
}

/// Remove header/footer lines from the extracted text.
#[must_use]
pub fn remove_header_footer_lines(text: &str, header_footer_lines: &[String]) -> String {
    if header_footer_lines.is_empty() {
        return text.to_string();
    }

    let remove_page_numbers = header_footer_lines.iter().any(|l| l == "__PAGE_NUMBER__");

    let hf_set: Vec<&String> =
        header_footer_lines.iter().filter(|l| *l != "__PAGE_NUMBER__").collect();

    text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return true; // Keep blank lines for paragraph separation
            }

            // Check page number patterns
            if remove_page_numbers && is_page_number(trimmed) {
                return false;
            }

            // Check against detected header/footer lines
            let normalized = normalize_line(trimmed);
            !hf_set.iter().any(|hf| {
                let hf_norm = normalize_line(hf);
                normalized == hf_norm || normalized.contains(&hf_norm)
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip the abstract section from the beginning of the text.
/// Looks for "Abstract" heading and removes everything up to the next section heading.
///
/// Note: `classify_sections` (in `utils::sections`) provides a more structured
/// view of the same document. This substring-based stripper is kept because it
/// is proven and covered by existing tests; new consumers (T1.2 chunking,
/// T1.3 section summaries) use `classify_sections` directly.
#[must_use]
pub fn strip_abstract(text: &str) -> String {
    let abstract_patterns = [
        "\nabstract\n",
        "\nabstract ",
        "\nAbstract\n",
        "\nAbstract ",
        "\nABSTRACT\n",
        "\nABSTRACT ",
    ];

    // Find where abstract starts
    let mut abstract_start: Option<usize> = None;
    for pattern in &abstract_patterns {
        if let Some(pos) = text.find(pattern) {
            // Find the actual start of the line
            if let Some(line_start) = text[..pos].rfind('\n') {
                let candidate = line_start + 1;
                if abstract_start.is_none_or(|s| candidate < s) {
                    abstract_start = Some(candidate);
                }
            }
        }
    }

    let Some(start) = abstract_start else {
        return text.to_string();
    };

    // Now find where the next section starts (Introduction, Methods, etc.)
    let after_abstract = &text[start..];
    let section_patterns = [
        "\n1.",
        "\n1 ",
        "\nIntroduction",
        "\nINTRODUCTION",
        "\nBackground",
        "\nBACKGROUND",
        "\nMain Text",
        "\nMethods\n",
        "\nMETHODS",
    ];

    let mut section_end: Option<usize> = None;
    for pattern in &section_patterns {
        if let Some(pos) = after_abstract.find(pattern) {
            let candidate = pos + start;
            if section_end.is_none_or(|s| candidate < s) {
                section_end = Some(candidate);
            }
        }
    }

    match section_end {
        Some(end) => {
            let before = &text[..start];
            let after = &text[end..];
            format!("{before}{after}")
        }
        None => text.to_string(),
    }
}

/// Strip the references section from the end of the text.
///
/// Note: `classify_sections` (in `utils::sections`) provides a more structured
/// view of the same document. This substring-based stripper is kept because it
/// is proven and covered by existing tests; new consumers (T1.2 chunking,
/// T1.3 section summaries) use `classify_sections` directly.
#[must_use]
pub fn strip_references(text: &str) -> String {
    let ref_patterns = [
        "\nReferences\n",
        "\nReferences ",
        "\nREFERENCES\n",
        "\nREFERENCES ",
        "\nBibliography\n",
        "\nBIBLIOGRAPHY\n",
        "\nReferences and Notes\n",
        "\nLiterature Cited\n",
    ];

    let mut ref_pos: Option<usize> = None;
    // Search from the end - find the LAST occurrence of a references heading
    for pattern in &ref_patterns {
        if let Some(pos) = text.rfind(pattern) {
            // Only consider it if it's in the latter half of the document
            if pos > text.len() / 2 && ref_pos.is_none_or(|p| pos < p) {
                ref_pos = Some(pos);
            }
        }
    }

    match ref_pos {
        Some(pos) => text[..pos].to_string(),
        None => text.to_string(),
    }
}

/// Strip both abstract and references from text.
fn strip_abstract_and_references(text: &str) -> String {
    let result = strip_abstract(text);
    strip_references(&result)
}

/// Truncate text to a maximum number of words.
#[must_use]
pub fn truncate_to_word_limit(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        return text.to_string();
    }
    words[..max_words].join(" ")
}
