use std::collections::HashMap;
use std::path::Path;

use lopdf::Document as LopdfDocument;

/// Maximum number of words allowed in extracted full text.
const MAX_WORDS: usize = 30_000;

/// Fraction of non-whitespace characters in the C1 control range
/// (U+0080-U+009F) at/above which the extracted text is treated as mojibake.
///
/// Legacy CJK PDFs often embed fonts that use a Shift-JIS / EUC-JP / CP949 /
/// GB18030 byte encoding without a ToUnicode CMap. `unpdf` trusts the font and
/// emits the raw byte values as Latin-1 code points, producing strings like
/// `"ÍN"` where `0x82 0xCD` should have decoded as `は` and `0x94 0x4E` as
/// `年`. The telltale signature is a high density of C1 control characters
/// (which are exceedingly rare in valid UTF-8 text), so we detect the pattern
/// and re-decode the underlying bytes via `chardetng` + `encoding_rs`.
///
/// The threshold is intentionally low (0.5%): C1 controls are non-printable
/// device-control characters (e.g. "Partial Line Forward", "Private Use 2")
/// that have no legitimate place in extracted document text, so even a small
/// density is definitive evidence of mojibake. For a 15 000-char extraction,
/// 0.5% is ~75 C1 chars - well above the noise floor and well below the
/// diluted ratios seen in CJK-with-heavy-English-loanwords PDFs (the naika
/// fixture is 3.9% despite being ~60% English loanwords + digits).
const MOJIBAKE_C1_DENSITY_THRESHOLD: f64 = 0.005;

/// Minimum non-whitespace character count before mojibake detection runs.
/// Tiny strings can produce noisy ratios; skip them.
const MOJIBAKE_MIN_CHARS: usize = 50;

/// Absolute minimum C1 control-char count for the mojibake verdict. Guards
/// against false positives on short extractions that contain a stray control
/// char (e.g. a single U+0080 from a malformed font glyph). Combined with the
/// density threshold: a text is mojibake when it has at least this many C1
/// chars AND the density is at/above the threshold.
const MOJIBAKE_MIN_C1_COUNT: usize = 10;

/// Extracts clean full text from a PDF file.
///
/// Pipeline:
/// 1. Extract text page-by-page using lopdf for header/footer detection
/// 2. Detect repeating header/footer lines across pages
/// 3. Extract full text using `unpdf` (best quality, handles custom font encodings)
///    - Falls back to `pdf-extract` if `unpdf` fails (legacy compat)
///    - Falls back to `lopdf` page-by-page if both fail (degraded but always works)
/// 4. **Mojibake recovery**: legacy CJK PDFs without a ToUnicode CMap produce
///    garbled text (raw Shift-JIS/EUC-JP byte values emitted as Latin-1 code
///    points). When the C1 control-char density exceeds the threshold, the
///    bytes are re-detected via `chardetng` and re-decoded via `encoding_rs`.
/// 5. Remove detected headers/footers from the extracted text
/// 6. Strip abstract and references sections
/// 7. Truncate to MAX_WORDS
pub fn extract_pdf_text(file_path: &Path) -> Result<String, String> {
    // Step 1: Detect headers/footers from lopdf (also pre-loads pages for fallback)
    let header_footer_lines = detect_headers_footers(file_path)?;

    // Step 2: Extract full text.
    // Tier 1: unpdf (best quality - handles Type1 custom fonts without panicking)
    // Tier 2: pdf-extract (legacy fallback - may panic on broken Unicode maps)
    // Tier 3: lopdf page-by-page (last resort - degraded but always works)
    let raw_text = match extract_via_unpdf(file_path) {
        Ok(text) if !text.trim().is_empty() => text,
        _ => {
            // unpdf failed or returned empty - try pdf-extract with panic safety.
            match extract_text_safe(file_path) {
                Ok(text) => text,
                Err(e) => {
                    // pdf-extract also failed - fall back to lopdf.
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

    // Step 3: Mojibake recovery. Runs BEFORE header/footer stripping so the
    // cleaner Unicode text flows through the rest of the pipeline (section
    // classification, chunking, translation). Only fires when the C1
    // control-char density exceeds the threshold; clean UTF-8 text passes
    // through unchanged.
    let recovered = recover_mojibake(&raw_text);

    // Step 4: Remove detected header/footer lines
    let cleaned = remove_header_footer_lines(&recovered, &header_footer_lines);

    // Step 5: Strip abstract and references
    let stripped = strip_abstract_and_references(&cleaned);

    // Step 6: Truncate to word limit
    let truncated = truncate_to_word_limit(&stripped, MAX_WORDS);

    Ok(truncated)
}

// ---------------------------------------------------------------------------
// Mojibake detection + recovery (legacy CJK PDFs without a ToUnicode CMap)
// ---------------------------------------------------------------------------

/// Detect whether `text` is mojibake and, if so, try to recover the original
/// Unicode text by re-decoding the underlying bytes.
///
/// Returns the recovered Unicode text when recovery succeeds, or the original
/// text unchanged when:
/// - the text is too short to evaluate (`MOJIBAKE_MIN_CHARS`),
/// - the C1 control-char density is below the threshold (not mojibake), or
/// - `chardetng` is not confident enough / the candidate encoding is not one of
///   the known legacy CJK encodings.
///
/// This is a heuristic, not a guarantee: it trades a narrow false-positive risk
/// on genuinely Latin-1 text (which is rare in academic PDFs) for fixing the
/// common legacy-CJK-PDF case where `unpdf` emits raw Shift-JIS byte values as
/// Latin-1 code points. The detector only fires on a high density of C1 control
/// characters (U+0080-U+009F), which are exceedingly rare in valid UTF-8 text
/// and in valid Latin-1 text (they are non-printable controls).
#[must_use]
pub fn recover_mojibake(text: &str) -> String {
    if !is_mojibake(text) {
        return text.to_string();
    }
    // Re-encode the (already-decoded) Rust string back to its original bytes
    // via Latin-1. This is the inverse of the buggy decode `unpdf` performed:
    // it took each raw byte and cast it to a `char`, which in Rust is the same
    // as Latin-1 decoding for bytes in 0x00-0xFF.
    let bytes: Vec<u8> = text.chars().map(|c| u8::try_from(c).unwrap_or(b'?')).collect();

    // Run the charset detector on the raw bytes.
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(&bytes, true);
    let (encoding, confident) = detector.guess_assess(None, true);

    // Only accept the candidate when (a) the detector is confident AND (b) the
    // encoding is one of the known legacy CJK encodings. This guards against
    // false-positive re-decode of legitimate Latin-1 / Windows-1252 text.
    if !confident || !is_legacy_cjk_encoding(encoding) {
        return text.to_string();
    }

    let (decoded, _, had_errors) = encoding.decode(&bytes);
    if had_errors {
        // The detector's candidate still produced replacement chars; keep the
        // original rather than introducing a different kind of garbage.
        return text.to_string();
    }
    // Sanity check: the recovered text must have a LOWER C1 density than the
    // original (otherwise we made things worse). This catches pathological
    // cases where the detector picked a wrong-but-non-erroring encoding.
    if c1_control_density(&decoded) >= c1_control_density(text) {
        return text.to_string();
    }
    decoded.into_owned()
}

/// `true` when `text` shows the mojibake signature: enough non-whitespace
/// content to evaluate, an absolute C1 control-char count at/above the floor,
/// AND a C1 control-char density at/above the threshold.
///
/// The two-part guard (absolute count + density) prevents false positives on
/// short extractions with a stray control char while still catching the
/// diluted-but-real mojibake from CJK PDFs whose text is heavy with ASCII
/// loanwords + digits (e.g. the naika fixture: 591 C1 chars / 3.9% density).
#[must_use]
pub fn is_mojibake(text: &str) -> bool {
    let non_ws: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if non_ws.len() < MOJIBAKE_MIN_CHARS {
        return false;
    }
    let c1_count = non_ws.iter().filter(|&&c| ('\u{0080}'..='\u{009F}').contains(&c)).count();
    if c1_count < MOJIBAKE_MIN_C1_COUNT {
        return false;
    }
    let density = c1_count as f64 / non_ws.len() as f64;
    density >= MOJIBAKE_C1_DENSITY_THRESHOLD
}

/// Fraction of characters in `text` that fall in the C1 control range
/// (U+0080-U+009F). These bytes are the raw Shift-JIS / EUC-JP lead/trail
/// bytes that `unpdf` cast directly to `char`.
#[must_use]
pub fn c1_control_density(text: &str) -> f64 {
    let non_ws: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if non_ws.is_empty() {
        return 0.0;
    }
    c1_control_ratio(&non_ws)
}

/// Compute the C1 ratio over a pre-filtered (non-whitespace) char slice.
fn c1_control_ratio(non_ws: &[char]) -> f64 {
    if non_ws.is_empty() {
        return 0.0;
    }
    let c1 = non_ws.iter().filter(|&&c| ('\u{0080}'..='\u{009F}').contains(&c)).count();
    c1 as f64 / non_ws.len() as f64
}

/// `true` when `encoding` is one of the legacy CJK encodings whose raw byte
/// streams produce the mojibake signature when misinterpreted as Latin-1.
/// Restricting recovery to this set prevents false-positive re-decode of
/// legitimate Latin-1 / Windows-1252 / UTF-8 text.
fn is_legacy_cjk_encoding(encoding: &'static encoding_rs::Encoding) -> bool {
    // `encoding_rs` exposes the canonical static singletons; compare by name.
    let name = encoding.name();
    matches!(
        name,
        "Shift_JIS"
            | "EUC-JP"
            | "ISO-2022-JP"
            | "EUC-KR"
            | "ISO-2022-KR"
            | "GB18030"
            | "GBK"
            | "gb18030"
            | "Big5"
    )
}

/// Call `pdf_extract::extract_text` wrapped in `catch_unwind` so panics
/// (e.g. FromUtf16Error on malformed font maps) are converted to `Err`.
fn extract_text_safe(file_path: &Path) -> Result<String, String> {
    std::panic::catch_unwind(|| pdf_extract::extract_text(file_path))
        .map_err(|_| "PDF extraction panicked - the PDF may contain unsupported fonts".to_string())?
        .map_err(|e| format!("PDF extraction failed: {e}"))
}

/// Extract text via `unpdf` - the highest-quality PDF text extractor.
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
