/*! Pre-parser for LLM-emitted JSON responses.

LLMs occasionally place raw control characters (most commonly `0x0A` newlines)
inside JSON string values instead of escaping them. The outer OpenAI envelope
is well-formed, but `serde_json::from_str` fails because RFC 8259 forbids
literal control bytes inside JSON string literals.

[`escape_control_chars_in_json`] repairs non-destructively: walks the document
tracking `"..."` state, re-emits raw control chars as JSON escapes. Structural
whitespace between tokens is preserved. Valid JSON passes through byte-
identical.

Project-wide fix for the recurring "control character" class of errors. Strict
superset of correctness vs. stripping (which would corrupt paragraph breaks
in summaries and screening reasoning). */

/// Escape raw control characters (`0x00`–`0x1F`) that appear **inside JSON
/// string literals**, so `serde_json::from_str` accepts otherwise-valid LLM
/// responses whose string values contain literal newlines/tabs/NULs.
///
/// # What it does
///
/// Walks the input char-by-char, tracking `in_string` and `escape_next`.
/// Inside a string literal, control chars are emitted as JSON escapes
/// (`\n`, `\r`, `\t`, `\b`, `\f`, or `\u00XX`). Outside strings, whitespace
/// passes through unchanged.
///
/// # Why escape, not strip
///
/// Stripping would collapse paragraph breaks, run together multi-line
/// reasoning, and damage bullet lists — silent content corruption in
/// user-facing text. Escaping preserves the logical content.
///
/// # Idempotence
///
/// Already-valid JSON passes through byte-identical. Safe on any LLM response.
///
/// Pure: no I/O, no panics.
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

    /* If the document ended mid-string with a dangling `\`, we've already
    emitted the backslash above; the caller's `serde_json::from_str` will
    report a clean "EOF" error. Further repair is `balance_braces`'s job. */
    out
}

/** Strip markdown code fences (```` ```json ```` / ```` ``` ````) and trim.

Unlike `screening_engine::extract_json` (which unwraps first nested array for
the top-level-array screening schema), this does ONE safe thing: strip fences.
It does not attempt array/object shape normalization — the caller's
`serde_json::from_str` is the source of truth for JSON validity.

Rationale: the summary schema is a top-level JSON **object** containing
legitimate arrays (`section_summaries`, figure descriptions). Feeding it
through `extract_json` would corrupt it into just the first nested array. */
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

/** Combined pre-parser used by `LlmOrchestrator::send_json` and by
JSON-parsing call sites. Runs fences-first then control-char escape.
Pure, composable with further shape-specific repair. */
#[must_use]
pub fn prepare_llm_json(raw: &str) -> String {
    escape_control_chars_in_json(&strip_code_fences(raw))
}

/** Whether `ch` is a JSON control character (U+0000–U+001F).
`0x7F` (DEL) and C1 controls (U+0080–U+009F) are intentionally excluded:
`serde_json` accepts them as-is inside strings. */
#[inline]
fn is_json_control_char(ch: char) -> bool {
    ('\u{0000}'..='\u{001F}').contains(&ch)
}

/** Push the canonical JSON escape sequence for a control char into `out`.
Uses the same two-char escapes (`\n`, `\t`, …) that `serde_json` emits;
remaining 27 codepoints use `\u00XX`. */
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
