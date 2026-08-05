/// Extracts (num_cited, num_references) from an N1 field via `strip_prefix`
/// on each line — faster than regex, no external deps. Returns `None` for
/// each unfound value. The full N1 value stays in `notes` regardless.
pub fn parse_n1_citation_data(n1_value: &str) -> (Option<i32>, Option<i32>) {
    let mut num_cited: Option<i32> = None;
    let mut num_references: Option<i32> = None;

    for line in n1_value.lines() {
        let trimmed = line.trim();

        if num_cited.is_none() {
            if let Some(rest) = trimmed.strip_prefix("Total Times Cited:") {
                if let Ok(val) = rest.trim().parse::<i32>() {
                    num_cited = Some(val);
                }
            }
        }

        if num_references.is_none() {
            if let Some(rest) = trimmed.strip_prefix("Cited Reference Count:") {
                if let Ok(val) = rest.trim().parse::<i32>() {
                    num_references = Some(val);
                }
            }
        }
    }

    (num_cited, num_references)
}
