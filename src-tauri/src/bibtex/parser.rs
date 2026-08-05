use std::collections::HashMap;

/// Parsed BibTeX entry.
#[derive(Debug, Clone, Default)]
pub struct BibtexEntry {
    pub entry_type: String,
    pub key: String,
    /// Name-value pairs in order of appearance.
    pub fields: Vec<(String, String)>,
}

/// Result of parsing a complete BibTeX file.
#[derive(Debug)]
pub struct BibtexParseResult {
    pub entries: Vec<BibtexEntry>,
    pub errors: Vec<BibtexParseError>,
}

/// A single parse error for an entry in the BibTeX file.
#[derive(Debug)]
pub struct BibtexParseError {
    /// 1-based index of the entry in the file.
    pub entry_index: usize,
    pub message: String,
}

/// Parses a `.bib` file. Handles `@type{key, ...}`, `{value}`/`"value"`
/// delimiters, nested braces, `@string` macros, `%` comments.
/// Skips `@comment` and `@preamble`.
#[must_use]
pub fn parse_bibtex(content: &str) -> BibtexParseResult {
    // Strip BOM if present
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);

    // Normalize line endings
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");

    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut string_macros: HashMap<String, String> = HashMap::new();
    let mut entry_index = 0;

    let mut pos = 0u32;
    let chars: Vec<char> = normalized.chars().collect();
    let len = chars.len();

    while (pos as usize) < len {
        // Skip whitespace and comments
        pos = skip_whitespace_and_comments(&chars, pos, len);

        if (pos as usize) >= len {
            break;
        }

        // Expect '@'
        if chars[pos as usize] != '@' {
            pos += 1;
            continue;
        }
        pos += 1;

        pos = skip_whitespace_and_comments(&chars, pos, len);
        if (pos as usize) >= len {
            break;
        }

        // Read entry type
        let entry_type_start = pos as usize;
        while (pos as usize) < len && is_entry_type_char(chars[pos as usize]) {
            pos += 1;
        }
        let entry_type =
            chars[entry_type_start..pos as usize].iter().collect::<String>().to_lowercase();

        pos = skip_whitespace_and_comments(&chars, pos, len);
        if (pos as usize) >= len {
            break;
        }

        // Handle special entry types
        if entry_type == "comment" {
            // Skip until matching brace or end of line
            pos = skip_block(&chars, pos, len);
            continue;
        }

        if entry_type == "preamble" {
            pos = skip_block(&chars, pos, len);
            continue;
        }

        // Expect opening delimiter: '{' or '('
        let open_delim = chars[pos as usize];
        if open_delim != '{' && open_delim != '(' {
            errors.push(BibtexParseError {
                entry_index: entry_index + 1,
                message: format!(
                    "Expected '{{' or '(' after @{}, found '{}'",
                    entry_type, chars[pos as usize]
                ),
            });
            pos += 1;
            continue;
        }
        let close_delim = if open_delim == '{' { '}' } else { ')' };
        pos += 1;

        if entry_type == "string" {
            // Parse @string{key = "value"} or @string{key = {value}}
            pos = skip_whitespace_and_comments(&chars, pos, len);
            let (macro_key, macro_value, new_pos) =
                parse_string_entry(&chars, pos, len, close_delim, &string_macros);
            pos = new_pos;
            if let (Some(k), Some(v)) = (macro_key, macro_value) {
                string_macros.insert(k.to_lowercase(), v);
            }
            // Skip closing delimiter
            if (pos as usize) < len && chars[pos as usize] == close_delim {
                pos += 1;
            }
            continue;
        }

        // Parse citation key (everything up to the first ',')
        pos = skip_whitespace_and_comments(&chars, pos, len);
        let key_start = pos as usize;
        while (pos as usize) < len
            && chars[pos as usize] != ','
            && chars[pos as usize] != close_delim
        {
            pos += 1;
        }
        let key: String =
            chars[key_start..pos as usize].iter().collect::<String>().trim().to_string();

        // Skip the comma separator
        if (pos as usize) < len && chars[pos as usize] == ',' {
            pos += 1;
        }

        // Parse fields until closing delimiter
        let mut fields: Vec<(String, String)> = Vec::new();
        let field_result = parse_fields(&chars, pos, len, close_delim, &string_macros, &mut fields);

        match field_result {
            Ok(new_pos) => pos = new_pos,
            Err(msg) => {
                entry_index += 1;
                errors.push(BibtexParseError { entry_index, message: msg });
                // Try to recover: skip to closing delimiter
                pos = find_closing_delim(&chars, pos, len, close_delim);
                if (pos as usize) < len && chars[pos as usize] == close_delim {
                    pos += 1;
                }
                continue;
            }
        }

        // Skip closing delimiter
        if (pos as usize) < len && chars[pos as usize] == close_delim {
            pos += 1;
        }

        entry_index += 1;
        entries.push(BibtexEntry { entry_type, key, fields });
    }

    BibtexParseResult { entries, errors }
}

/// Parse fields within an entry until the closing delimiter.
/// Returns Ok(new_position) after the last field, or Err with a message.
fn parse_fields(
    chars: &[char],
    mut pos: u32,
    len: usize,
    close_delim: char,
    string_macros: &HashMap<String, String>,
    fields: &mut Vec<(String, String)>,
) -> Result<u32, String> {
    loop {
        pos = skip_whitespace_and_comments(chars, pos, len);
        if pos as usize >= len {
            return Err("Unexpected end of file while parsing fields".to_string());
        }

        // Check for closing delimiter
        if chars[pos as usize] == close_delim {
            return Ok(pos);
        }

        // Read field name
        let name_start = pos as usize;
        while (pos as usize) < len && is_field_name_char(chars[pos as usize]) {
            pos += 1;
        }
        let field_name: String =
            chars[name_start..pos as usize].iter().collect::<String>().to_lowercase();

        if field_name.is_empty() {
            // Skip unexpected characters
            pos += 1;
            continue;
        }

        pos = skip_whitespace_and_comments(chars, pos, len);

        // Expect '='
        if pos as usize >= len || chars[pos as usize] != '=' {
            return Err(format!(
                "Expected '=' after field name '{}', found '{}'",
                field_name,
                if (pos as usize) < len {
                    chars[pos as usize].to_string()
                } else {
                    "EOF".to_string()
                }
            ));
        }
        pos += 1;

        pos = skip_whitespace_and_comments(chars, pos, len);

        // Read field value (could be concatenation with #)
        let mut value_parts = Vec::new();
        loop {
            pos = skip_whitespace_and_comments(chars, pos, len);
            if pos as usize >= len {
                break;
            }

            let ch = chars[pos as usize];
            if ch == '{' {
                // Brace-delimited value
                let (val, new_pos) = read_braced_value(chars, pos + 1, len);
                value_parts.push(val);
                pos = new_pos;
            } else if ch == '"' {
                // Quote-delimited value
                let (val, new_pos) = read_quoted_value(chars, pos + 1, len);
                value_parts.push(val);
                pos = new_pos;
            } else if ch.is_ascii_digit() {
                // Bare number value
                let num_start = pos as usize;
                while (pos as usize) < len && chars[pos as usize].is_ascii_digit() {
                    pos += 1;
                }
                let num: String = chars[num_start..pos as usize].iter().collect();
                value_parts.push(num);
            } else if is_field_name_char(ch) {
                // Could be a string macro reference
                let ident_start = pos as usize;
                while (pos as usize) < len && is_field_name_char(chars[pos as usize]) {
                    pos += 1;
                }
                let ident: String =
                    chars[ident_start..pos as usize].iter().collect::<String>().to_lowercase();
                if let Some(macro_val) = string_macros.get(&ident) {
                    value_parts.push(macro_val.clone());
                } else {
                    // Unknown macro - use the identifier as-is
                    value_parts.push(ident);
                }
            } else {
                break;
            }

            pos = skip_whitespace_and_comments(chars, pos, len);

            // Check for concatenation (#)
            if (pos as usize) < len && chars[pos as usize] == '#' {
                pos += 1;
                continue;
            } else {
                break;
            }
        }

        let value = value_parts.join("");

        // Strip whitespace-only values should still be stored (validation handles them)
        fields.push((field_name, value));

        pos = skip_whitespace_and_comments(chars, pos, len);

        // Skip optional comma separator
        if (pos as usize) < len && chars[pos as usize] == ',' {
            pos += 1;
        }
    }
}

/// Read a brace-delimited value, tracking nesting depth.
/// `pos` should point to the first character after the opening '{'.
/// Returns (value, position_after_closing_brace).
fn read_braced_value(chars: &[char], mut pos: u32, len: usize) -> (String, u32) {
    let mut value = String::new();
    let mut depth = 1u32;

    while (pos as usize) < len && depth > 0 {
        let ch = chars[pos as usize];
        if ch == '{' {
            depth += 1;
            value.push(ch);
        } else if ch == '}' {
            depth -= 1;
            if depth > 0 {
                value.push(ch);
            }
        } else {
            value.push(ch);
        }
        pos += 1;
    }

    (value, pos)
}

/// `pos` is first char after `"`. Returns (value, pos_after_closing_quote).
/// Escape conventions: `\"` (LaTeX) and `""` (EBSCO).
fn read_quoted_value(chars: &[char], mut pos: u32, len: usize) -> (String, u32) {
    let mut value = String::new();

    while (pos as usize) < len {
        let ch = chars[pos as usize];
        if ch == '"' {
            // Check for escaped quote "" (EBSCO convention: "" → literal ")
            if (pos as usize) + 1 < len && chars[(pos as usize) + 1] == '"' {
                value.push('"');
                pos += 2;
                continue;
            }
            // Heuristic for EBSCO-style unescaped quotes inside values:
            // If the next non-whitespace char is NOT a field/entry separator
            // (comma, closing brace, hash), treat this " as a literal quote.
            let next_idx = (pos as usize) + 1;
            if next_idx < len {
                let mut peek = next_idx;
                while peek < len && chars[peek].is_whitespace() {
                    peek += 1;
                }
                if peek < len {
                    let next_ch = chars[peek];
                    if next_ch != ',' && next_ch != '}' && next_ch != ')' && next_ch != '#' {
                        // Looks like content after the quote → treat as literal
                        value.push('"');
                        pos += 1;
                        continue;
                    }
                }
            }
            pos += 1;
            break;
        } else if ch == '\\' && (pos as usize) + 1 < len && chars[(pos as usize) + 1] == '"' {
            // Backslash-escaped quote: consume both, emit quote
            value.push('"');
            pos += 2;
        } else if ch == '{' {
            // Braces inside quotes are literal
            let (braced, new_pos) = read_braced_value(chars, pos + 1, len);
            value.push('{');
            value.push_str(&braced);
            value.push('}');
            pos = new_pos;
        } else {
            value.push(ch);
            pos += 1;
        }
    }

    (value, pos)
}

/// Parse a @string entry and return (key, value, new_position).
fn parse_string_entry(
    chars: &[char],
    mut pos: u32,
    len: usize,
    _close_delim: char,
    string_macros: &HashMap<String, String>,
) -> (Option<String>, Option<String>, u32) {
    pos = skip_whitespace_and_comments(chars, pos, len);

    // Read key
    let key_start = pos as usize;
    while (pos as usize) < len && is_field_name_char(chars[pos as usize]) {
        pos += 1;
    }
    let key: String = chars[key_start..pos as usize].iter().collect::<String>().trim().to_string();

    pos = skip_whitespace_and_comments(chars, pos, len);

    // Expect '='
    if pos as usize >= len || chars[pos as usize] != '=' {
        return (None, None, pos);
    }
    pos += 1;

    pos = skip_whitespace_and_comments(chars, pos, len);

    // Read value (single part, no concatenation for simplicity)
    let value = if (pos as usize) < len && chars[pos as usize] == '{' {
        let (v, new_pos) = read_braced_value(chars, pos + 1, len);
        pos = new_pos;
        Some(v)
    } else if (pos as usize) < len && chars[pos as usize] == '"' {
        let (v, new_pos) = read_quoted_value(chars, pos + 1, len);
        pos = new_pos;
        Some(v)
    } else if (pos as usize) < len && is_field_name_char(chars[pos as usize]) {
        let ident_start = pos as usize;
        while (pos as usize) < len && is_field_name_char(chars[pos as usize]) {
            pos += 1;
        }
        let ident: String =
            chars[ident_start..pos as usize].iter().collect::<String>().to_lowercase();
        string_macros.get(&ident).cloned().or(Some(ident))
    } else {
        None
    };

    pos = skip_whitespace_and_comments(chars, pos, len);

    // Skip optional comma
    if (pos as usize) < len && chars[pos as usize] == ',' {
        pos += 1;
    }

    (if key.is_empty() { None } else { Some(key) }, value, pos)
}

/// Skip whitespace and `%` comment lines.
fn skip_whitespace_and_comments(chars: &[char], mut pos: u32, len: usize) -> u32 {
    while (pos as usize) < len {
        let ch = chars[pos as usize];
        if ch.is_whitespace() {
            pos += 1;
        } else if ch == '%' {
            // Skip to end of line
            while (pos as usize) < len && chars[pos as usize] != '\n' {
                pos += 1;
            }
            // Skip the newline too
            if (pos as usize) < len {
                pos += 1;
            }
        } else {
            break;
        }
    }
    pos
}

/// Skip a block (comment/preamble) delimited by braces or parentheses.
fn skip_block(chars: &[char], mut pos: u32, len: usize) -> u32 {
    if pos as usize >= len {
        return pos;
    }

    let ch = chars[pos as usize];
    if ch == '{' {
        let (_, new_pos) = read_braced_value(chars, pos + 1, len);
        new_pos
    } else if ch == '(' {
        pos += 1;
        let mut depth = 1u32;
        while (pos as usize) < len && depth > 0 {
            if chars[pos as usize] == '(' {
                depth += 1;
            } else if chars[pos as usize] == ')' {
                depth -= 1;
            }
            pos += 1;
        }
        pos
    } else {
        // No delimiter found; skip to end of line
        while (pos as usize) < len && chars[pos as usize] != '\n' {
            pos += 1;
        }
        pos
    }
}

/// Find a closing delimiter at depth 0, for error recovery.
fn find_closing_delim(chars: &[char], mut pos: u32, len: usize, close_delim: char) -> u32 {
    let mut depth = 1u32;
    while (pos as usize) < len && depth > 0 {
        if chars[pos as usize] == '{' || chars[pos as usize] == '(' {
            depth += 1;
        } else if chars[pos as usize] == close_delim {
            depth -= 1;
        }
        if depth > 0 {
            pos += 1;
        }
    }
    pos
}

/// Check if a character is valid in an entry type name.
fn is_entry_type_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '+' || ch == '-'
}

/// Check if a character is valid in a field name.
fn is_field_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}
