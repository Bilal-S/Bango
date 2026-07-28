use crate::error::AppError;
use crate::screening::engine::LlmScreeningResponse;
use crate::utils::json_repair::escape_control_chars_in_json;

/// Debug-only logging macro. Compiles to a no-op in release builds.
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        { eprintln!($($arg)*); }
    };
}

/// Parse the LLM response as a JSON array of screening results.
pub fn process_screening_responses(raw: &str) -> Result<Vec<LlmScreeningResponse>, AppError> {
    debug_log!("[screening] process_screening_responses received {} bytes", raw.len());
    debug_log!("[screening] raw first 300 chars: {}", &raw[..raw.len().min(300)]);

    let json_str = extract_json(raw);

    debug_log!("[screening] extract_json produced {} bytes", json_str.len());
    debug_log!(
        "[screening] extracted JSON first 300 chars: {}",
        &json_str[..json_str.len().min(300)]
    );

    match serde_json::from_str::<Vec<LlmScreeningResponse>>(&json_str) {
        Ok(mut results) => {
            // M9: Validate and normalize LLM decision values
            for r in &mut results {
                let d = r.decision.to_lowercase();
                match d.as_str() {
                    "include" | "exclude" | "error" => {
                        r.decision = d;
                    }
                    _ => {
                        debug_log!(
                            "[screening] Unexpected decision '{}', treating as error",
                            r.decision
                        );
                        r.reasoning = format!(
                            "Unexpected LLM decision: '{}'. Original reasoning: {}",
                            r.decision, r.reasoning
                        );
                        r.decision = "error".to_string();
                    }
                }
            }
            debug_log!("[screening] successfully parsed {} screening results", results.len());
            Ok(results)
        }
        Err(e) => {
            debug_log!("[screening] FAILED to parse screening response: {e}");
            debug_log!(
                "[screening] attempted JSON (first 500 chars): {}",
                &json_str[..json_str.len().min(500)]
            );

            // Try truncated JSON repair: find last complete `}` and add missing `]`
            if let Some(repaired) = repair_truncated_json_array(&json_str) {
                debug_log!("[screening] attempting truncated JSON repair...");
                match serde_json::from_str::<Vec<LlmScreeningResponse>>(&repaired) {
                    Ok(mut results) => {
                        // M9: Validate repaired results too
                        for r in &mut results {
                            let d = r.decision.to_lowercase();
                            match d.as_str() {
                                "include" | "exclude" | "error" => {
                                    r.decision = d;
                                }
                                _ => {
                                    r.reasoning = format!(
                                        "Unexpected LLM decision: '{}'. Original reasoning: {}",
                                        r.decision, r.reasoning
                                    );
                                    r.decision = "error".to_string();
                                }
                            }
                        }
                        debug_log!(
                            "[screening] repair succeeded! Recovered {} results",
                            results.len()
                        );
                        return Ok(results);
                    }
                    Err(_repair_err) => {
                        debug_log!("[screening] repair also failed: {_repair_err}");
                    }
                }
            }

            Err(AppError::Import(format!("Malformed LLM response: {e}")))
        }
    }
}

/// Attempt to repair a truncated JSON array by finding the last complete object
/// and closing the array with `]`.
#[must_use]
pub fn repair_truncated_json_array(json: &str) -> Option<String> {
    // Only attempt if it looks like an incomplete array
    let trimmed = json.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    // If it already ends with `]`, it's not truncated (or is valid)
    if trimmed.ends_with(']') {
        return None;
    }

    // Find the last occurrence of `}` - the end of the last complete object
    let last_brace = trimmed.rfind('}')?;
    let candidate = &trimmed[..=last_brace];

    // Close the array
    let repaired = format!("{}]", candidate);

    // Quick sanity: must still start with `[`
    if !repaired.starts_with('[') {
        return None;
    }

    Some(repaired)
}

pub fn extract_json(raw: &str) -> String {
    // Sanitize raw control chars (literal newlines/tabs/NULs) the LLM may
    // have placed inside JSON string values BEFORE running the array/object
    // extraction strategies. See `utils::json_repair` for why we escape
    // rather than strip (data-fidelity preservation in user-facing content).
    let sanitized = escape_control_chars_in_json(raw);
    let trimmed = sanitized.trim();

    // Strategy 1: Code-fence stripping
    if trimmed.starts_with("```") {
        let without_start = trimmed.trim_start_matches("```json").trim_start_matches("```");
        let without_end = without_start.trim_end_matches("```");
        let inner = without_end.trim();
        // If the code-fence content is already a bare array, return it
        if inner.starts_with('[') {
            return inner.to_string();
        }
        // If it's a JSON object, try to extract an embedded array
        if inner.starts_with('{') {
            if let Some(arr) = extract_array_from_object(inner) {
                return arr;
            }
        }
        // LLMs may omit the opening `{` - repair brace balance before returning
        return balance_braces(inner);
    }

    // Strategy 2: Bare array (already correct)
    if trimmed.starts_with('[') {
        return trimmed.to_string();
    }

    // Strategy 3: JSON object wrapping an array - extract the array
    if trimmed.starts_with('{') {
        if let Some(arr) = extract_array_from_object(trimmed) {
            return arr;
        }
    }

    // Strategy 4: Try to find a JSON array anywhere in the text
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if end > start {
                let candidate = &trimmed[start..=end];
                // Validate it parses as JSON
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return candidate.to_string();
                }
            }
        }
    }

    // Strategy 5: Try to find a JSON object anywhere in the text
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                let candidate = &trimmed[start..=end];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return candidate.to_string();
                }
            }
        }
    }

    // Final fallback: repair brace balance (e.g. missing opening `{`)
    balance_braces(trimmed)
}

/// Repair missing opening `{` or closing `}` in a JSON-like string.
/// LLMs sometimes omit the opening brace, producing e.g. `"field": "value" ... }`.
#[must_use]
pub fn balance_braces(s: &str) -> String {
    // Count structural braces (ignoring those inside JSON string literals)
    let mut open = 0usize;
    let mut close = 0usize;
    let mut in_string = false;
    let mut escape_next = false;

    for ch in s.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            if ch == '{' {
                open += 1;
            } else if ch == '}' {
                close += 1;
            }
        }
    }

    let mut result = s.to_string();
    // Missing opening braces: prepend them
    if close > open {
        for _ in 0..(close - open) {
            result.insert(0, '{');
        }
    }
    // Missing closing braces: append them
    if open > close {
        for _ in 0..(open - close) {
            result.push('}');
        }
    }
    result
}

/// Given a JSON object string, find and extract the first JSON array value
/// at the first two levels of nesting that contains objects (screening results).
fn extract_array_from_object(obj_str: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(obj_str).ok()?;
    extract_array_from_value(&value)
}

/// Recursively search for a non-empty array whose first element is an object.
fn extract_array_from_value(value: &serde_json::Value) -> Option<String> {
    if let Some(obj) = value.as_object() {
        // Level 1: scan top-level keys for arrays containing objects
        for (_, v) in obj {
            if let Some(arr) = v.as_array() {
                if arr.first().is_some_and(|el| el.is_object()) {
                    return Some(v.to_string());
                }
            }
        }
        // Level 2: scan nested objects
        for (_, v) in obj {
            if let Some(result) = extract_array_from_value(v) {
                return Some(result);
            }
        }
    }
    None
}
