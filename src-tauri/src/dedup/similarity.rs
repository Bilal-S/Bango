/// Minimum character length for a title to participate in title-based matching.
const MIN_TITLE_LENGTH: usize = 10;

/// Normalizes a title for comparison:
/// 1. Lowercase
/// 2. Strip all punctuation
/// 3. Collapse whitespace
/// 4. Trim
#[must_use]
pub fn normalize_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let stripped: String = lower
        .chars()
        .map(|c| {
            if matches!(
                c,
                '.' | ','
                    | ';'
                    | ':'
                    | '!'
                    | '?'
                    | '\''
                    | '"'
                    | '-'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
            ) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut result = String::with_capacity(stripped.len());
    let mut last_was_space = true; // trim leading
    for c in stripped.chars() {
        if c == ' ' {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }
    // Trim trailing space
    let trimmed = result.trim_end();
    trimmed.to_string()
}

/// Returns true if the normalized title is too short for title-based matching.
#[must_use]
pub fn short_title_guard(title: &str) -> bool {
    let normalized = normalize_title(title);
    normalized.len() < MIN_TITLE_LENGTH
}

/// Computes Levenshtein distance between two strings.
#[must_use]
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, val) in matrix[0].iter_mut().enumerate() {
        *val = j;
    }

    for (i, a_char) in a.chars().enumerate() {
        for (j, b_char) in b.chars().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            matrix[i + 1][j + 1] =
                (matrix[i][j + 1] + 1).min(matrix[i + 1][j] + 1).min(matrix[i][j] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Computes normalized similarity (0.0-1.0) based on Levenshtein distance.
#[must_use]
pub fn levenshtein_similarity(a: &str, b: &str) -> f64 {
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let distance = levenshtein_distance(a, b);
    1.0 - (distance as f64 / max_len as f64)
}
