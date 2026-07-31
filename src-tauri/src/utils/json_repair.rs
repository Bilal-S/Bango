//! Pre-parser for LLM-emitted JSON responses.
//!
//! LLMs occasionally place **raw control characters** (most commonly `0x0A`
//! newlines, but also tabs / form-feeds / NULs) inside JSON string values
//! instead of escaping them as `\n` / `\t`. The outer OpenAI envelope is
//! well-formed JSON, so `llm::client` can decode `choices[0].message.content`
//! into a Rust `String` - but the inner summary/schema JSON, when re-parsed by
//! `serde_json::from_str`, fails with:
//!
//! ```text
//! control character (\u0000-\u001F) found while parsing a string at line N column M
//! ```
//!
//! because RFC 8259 forbids literal control bytes inside JSON string literals.
//!
//! [`escape_control_chars_in_json`] repairs this **non-destructively**: it walks
//! the document tracking whether the cursor is inside a `"..."` string literal,
//! and re-emits any raw control char it finds *inside* a string as the
//! equivalent JSON escape (`\n`, `\t`, `\u0000`, etc.). Structural whitespace
//! between tokens is preserved unchanged. A well-formed JSON document passes
//! through byte-identical; only otherwise-unparseable payloads are normalized.
//!
//! This is the project-wide fix for the recurring "AI summary failed: Invalid
//! JSON response from LLM: control character" class of errors. It is a strict
//! superset of correctness vs. the (rejected) alternative of *stripping*
//! control chars, which would silently corrupt user-facing content such as
//! `summary_150_250_words` paragraph breaks or screening `reasoning`.

/// Escape raw control characters (`0x00`–`0x1F`) that appear **inside JSON
/// string literals**, so `serde_json::from_str` accepts LLM responses whose
/// string values contain literal newlines / tabs / form-feeds / NULs instead
/// of the proper `\n` / `\t` escapes.
///
/// # What it does
///
/// Walks the input char-by-char, tracking:
/// - `in_string`: are we inside a `"..."` literal?
/// - `escape_next`: was the previous char an unescaped `\`?
///
/// - **Inside a string literal**: a char in `0x00..=0x1F` is emitted as its
///   JSON escape (`\n`, `\r`, `\t`, `\b`, `\f`, or `\u00XX` for the rest).
///   Everything else passes through verbatim. Crucially, a `"` preceded by an
///   unescaped `\` does **not** flip `in_string` off, so escaped quotes inside
///   the value are handled correctly.
/// - **Outside a string literal** (structural JSON): whitespace newlines/tabs
///   are already legal JSON insignia between tokens - pass through unchanged.
///
/// # Why escape, not strip
///
/// Stripping control chars (`s.retain(|c| !c.is_control())`) would collapse
/// paragraph breaks in `summary_150_250_words`, run together multi-line
/// `reasoning`, and damage `key_insights` bullets - silent content corruption
/// in user-facing text. Escaping preserves the logical content; only the JSON
/// representation is normalized, matching what the LLM *should* have emitted.
///
/// # Idempotence
///
/// A document that is already valid JSON (no raw control chars in strings)
/// passes through byte-identical. Safe to call on any LLM response.
///
/// Pure function: no I/O, no panics. Tested inline + via the regression case
/// in `tests/json_repair_test.rs`.
#[must_use]
pub fn escape_control_chars_in_json(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_string = false;
    let mut escape_next = false;

    for ch in raw.chars() {
        if in_string {
            if escape_next {
                // Previous char was `\`: this char is part of an escape
                // sequence (e.g. `\"`, `\n`, `\\`). Emit it verbatim and
                // resume normal scanning.
                out.push(ch);
                escape_next = false;
                continue;
            }
            if ch == '\\' {
                out.push('\\');
                escape_next = true;
                continue;
            }
            if ch == '"' {
                // Unescaped quote: closes the string literal.
                out.push('"');
                in_string = false;
                continue;
            }
            // Raw control char inside a string literal: escape it.
            if is_json_control_char(ch) {
                push_json_escape(&mut out, ch);
                continue;
            }
            // Normal string content.
            out.push(ch);
        } else {
            // Outside a string literal.
            if ch == '"' {
                in_string = true;
            }
            out.push(ch);
        }
    }

    // If the document ended mid-string with a dangling `\`, we've already
    // emitted the backslash above; the caller's `serde_json::from_str` will
    // report a clean "EOF while parsing a string" error. We do not attempt
    // further repair here - that is `balance_braces`'s job (screening path)
    // and out of scope for control-char normalization.
    out
}

/// Strip markdown code fences (```` ```json ```` / ```` ``` ````) from an LLM
/// response and trim whitespace.
///
/// **Why this exists (not `screening_engine::extract_json`):** `extract_json`
/// was written for the screening prompt, whose schema is a top-level JSON
/// **array**. Its Strategy 3 (`extract_array_from_object`) actively unwraps the
/// first nested array-of-objects it finds inside a JSON object. The article
/// summary schema is a top-level JSON **object** that legitimately contains
/// arrays-of-objects (e.g. `section_summaries`, figure descriptions). Feeding a
/// valid summary object through `extract_json` silently corrupts it into just
/// the `section_summaries` array, which then fails the substantive-content
/// check and triggers a spurious markdown-fallback retry.
///
/// This helper does ONE thing: strip code fences. It does not attempt any
/// array/object shape normalization. The caller's `serde_json::from_str` is the
/// single source of truth for JSON validity.
///
/// Pure function: no I/O.
#[must_use]
pub fn strip_code_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    // Strip ```json or ``` prefix.
    let after_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|rest| rest.trim_start());
    let Some(rest) = after_open else {
        // No code fence - return as-is (already trimmed).
        return trimmed.to_string();
    };
    // Strip the trailing ``` if present.
    if let Some(body) = rest.strip_suffix("```") {
        body.trim().to_string()
    } else {
        // Opening fence with no closing fence (truncated response). Return the
        // remainder so the caller's JSON parser can report a clean error.
        rest.to_string()
    }
}

/// The combined pre-parser used by [`crate::llm::orchestrator::LlmOrchestrator::send_json`]
/// and by JSON-parsing call sites that go through the orchestrator directly.
///
/// Runs the two repairs in the correct order:
/// 1. [`strip_code_fences`] - MUST run first because the fence strippers match
///    against the raw response (a leading ```` ``` ```` inside an already-escaped
///    JSON string value would be a false positive).
/// 2. [`escape_control_chars_in_json`] - sanitizes raw control chars the LLM may
///    have placed inside JSON string values.
///
/// Pure function: no I/O. Composable with further shape-specific repair
/// (e.g. screening's `extract_json` brace-balance pass).
#[must_use]
pub fn prepare_llm_json(raw: &str) -> String {
    escape_control_chars_in_json(&strip_code_fences(raw))
}

/// Whether `ch` is a JSON control character (U+0000 through U+001F) that RFC
/// 8259 requires to be escaped inside a string literal.
///
/// `0x7F` (DEL) and C1 controls (U+0080–U+009F) are intentionally NOT included:
/// they are not in the RFC 8259 "control character" range and `serde_json`
/// accepts them as-is inside strings, so escaping them would be a gratuitous
/// change to otherwise-valid input.
#[inline]
fn is_json_control_char(ch: char) -> bool {
    ('\u{0000}'..='\u{001F}').contains(&ch)
}

/// Push the canonical JSON escape sequence for an RFC 8259 control char into
/// `out`.
///
/// The two-char escapes (`\n`, `\t`, etc.) are the same ones `serde_json`
/// emits on serialization, so a round-trip through this helper + `to_string`
/// produces byte-identical output to a hand-correct document. The remaining
/// 27 codepoints in `0x00..=0x1F` (NUL, SOH, STX, …, US) use the
/// lowercase-hex `\u00XX` form that `serde_json` also accepts.
///
/// Pushing directly into the caller's buffer avoids the awkward
/// `&'static str` vs owned-`String` return type - the rare-control-char branch
/// can allocate a small 6-byte string without leaking.
#[inline]
fn push_json_escape(out: &mut String, ch: char) {
    match ch {
        '\u{0008}' => out.push_str("\\b"),
        '\u{0009}' => out.push_str("\\t"),
        '\u{000A}' => out.push_str("\\n"),
        '\u{000C}' => out.push_str("\\f"),
        '\u{000D}' => out.push_str("\\r"),
        _ => {
            // Remaining control chars use the `\u00XX` form. These are
            // vanishingly rare in LLM output, so the allocation is fine.
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let code = ch as u32;
            out.push_str("\\u00");
            out.push(HEX[((code >> 4) & 0x0F) as usize] as char);
            out.push(HEX[(code & 0x0F) as usize] as char);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_json_passes_through_unchanged() {
        let input = r#"{"key": "value", "n": 1, "arr": ["a", "b"]}"#;
        assert_eq!(escape_control_chars_in_json(input), input);
    }

    #[test]
    fn escapes_literal_newline_inside_string_value() {
        // The classic failure: LLM emits a raw newline inside a string value.
        let input = "{\"summary\": \"line one\nline two\"}";
        let repaired = escape_control_chars_in_json(input);
        assert_eq!(repaired, r#"{"summary": "line one\nline two"}"#);
        // And the repaired output now parses.
        let v: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v["summary"].as_str().unwrap(), "line one\nline two");
    }

    #[test]
    fn preserves_structural_newlines_between_tokens() {
        // Pretty-printed JSON has real newlines BETWEEN tokens - those are
        // legal and must be preserved.
        let input = "{\n  \"key\": \"value\"\n}";
        assert_eq!(escape_control_chars_in_json(input), input);
        let v: serde_json::Value = serde_json::from_str(input).unwrap();
        assert_eq!(v["key"].as_str().unwrap(), "value");
    }

    #[test]
    fn handles_escaped_quote_inside_string() {
        // `\"` inside a string must not flip in_string off.
        let input = r#"{"text": "she said \"hi\", then left"}"#;
        assert_eq!(escape_control_chars_in_json(input), input);
        let v: serde_json::Value = serde_json::from_str(input).unwrap();
        assert_eq!(v["text"].as_str().unwrap(), r#"she said "hi", then left"#);
    }

    #[test]
    fn handles_dangling_backslash_without_panic() {
        // String ends with `\` followed by other content. The `\` escapes the
        // next char so the string does NOT close - but that would be invalid
        // JSON anyway. We just ensure no panic and that the backslash is
        // preserved for serde_json to report cleanly.
        let input = r#"{"text": "trailing \"   }"#;
        let out = escape_control_chars_in_json(input);
        // No transformation expected (no raw control chars).
        assert_eq!(out, input);
    }

    #[test]
    fn escapes_tab_and_carriage_return_inside_strings() {
        let input = "{\"a\": \"x\ty\", \"b\": \"p\rq\"}";
        let out = escape_control_chars_in_json(input);
        assert_eq!(out, r#"{"a": "x\ty", "b": "p\rq"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["a"].as_str().unwrap(), "x\ty");
        assert_eq!(v["b"].as_str().unwrap(), "p\rq");
    }

    #[test]
    fn escapes_nul_and_other_rare_control_chars() {
        // NUL, SOH, US - the `\u00XX` branch.
        let input = "{\"a\": \"x\u{0000}y\", \"b\": \"\u{0001}\", \"c\": \"\u{001F}\"}";
        assert!(serde_json::from_str::<serde_json::Value>(input).is_err());
        let out = escape_control_chars_in_json(input);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["a"].as_str().unwrap(), "x\u{0000}y");
        assert_eq!(v["b"].as_str().unwrap(), "\u{0001}");
        assert_eq!(v["c"].as_str().unwrap(), "\u{001F}");
        // Confirm the lowercase-hex form is produced.
        assert!(out.contains("\\u0000"));
        assert!(out.contains("\\u0001"));
        assert!(out.contains("\\u001f"));
    }

    #[test]
    fn regression_real_article_summary_payload() {
        // Reproduces the failure reported in the bug report: the LLM placed
        // real `0x0A` newline bytes (decoded from `\n\n` in the envelope
        // string) inside the `summary_150_250_words` JSON string value.
        //
        // We build the payload with `format!` + a real newline (`'\n'`) so
        // the resulting `String` actually contains an unescaped control byte
        // inside the JSON string literal - exactly what `serde_json` rejects
        // with "control character (\u0000-\u001F) found while parsing a
        // string". (Using a `r#"..."#` raw string would make `\n` two literal
        // chars - backslash + n - and the payload would already be valid
        // JSON, defeating the regression's purpose.)
        let raw = format!(
            "{{\"field\": \"business_economics_finance\",\n\"summary_150_250_words\": \"This paper examines how digital technology platforms influence strategic behavior in green supply chain finance.{nl}{nl}The results indicate that platform access makes desirable strategies more likely to emerge.\",\n\"keywords\": [\"a\", \"b\"]}}",
            nl = '\n'
        );
        // Sanity: the raw payload is NOT valid JSON (literal newline inside
        // the summary string).
        assert!(serde_json::from_str::<serde_json::Value>(&raw).is_err());

        let repaired = escape_control_chars_in_json(&raw);
        // After repair it parses cleanly.
        let v: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v["field"].as_str().unwrap(), "business_economics_finance");
        // The logical newline is preserved in the parsed value (data fidelity).
        assert!(v["summary_150_250_words"].as_str().unwrap().contains('\n'));
        assert_eq!(v["keywords"][1].as_str().unwrap(), "b");
    }

    #[test]
    fn idempotent_on_clean_input() {
        let input = r#"{"a": "1\n2", "b": ["\t"]}"#;
        let once = escape_control_chars_in_json(input);
        let twice = escape_control_chars_in_json(&once);
        assert_eq!(once, input);
        assert_eq!(twice, input);
    }

    #[test]
    fn array_with_control_chars_in_string_values() {
        // Screening path: top-level array of objects, reasoning contains newlines.
        let raw =
            "[{\"decision\": \"include\", \"reasoning\": \"criterion 1 met.\nAlso criterion 2.\"}]";
        assert!(serde_json::from_str::<serde_json::Value>(raw).is_err());
        let repaired = escape_control_chars_in_json(raw);
        let v: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v[0]["decision"].as_str().unwrap(), "include");
        assert!(v[0]["reasoning"].as_str().unwrap().contains('\n'));
    }
}
