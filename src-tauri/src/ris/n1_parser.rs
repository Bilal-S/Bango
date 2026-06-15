/// Parses citation data from an N1 (Notes) field value.
///
/// Returns (num_cited, num_references) where each is `Some(count)` if found,
/// or `None` if the field was not present in the N1 value.
///
/// The N1 value is preserved in full in the article's `notes` field regardless
/// of whether citation data was extracted.
///
/// Uses `strip_prefix` on each trimmed line - WoS N1 values always have the
/// key at the start of a line. This is faster than regex and has no external
/// dependency.
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
