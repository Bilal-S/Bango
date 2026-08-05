//! Minimal YAML frontmatter parser/serializer for wiki pages. Parses the subset we emit:
//! scalar `key: value`, `key: "quoted"`, inline `[a, b, c]` lists. Avoids `serde_yaml` dep.
//! A wiki Markdown file is `---\n<frontmatter>\n---\n<body>`.

use std::collections::BTreeMap;

use crate::error::AppError;

const FM_DELIM: &str = "---";

/// Parsed frontmatter: ordered key/value pairs (raw strings; lists preserved as `[a, b]`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub fields: BTreeMap<String, String>,
}

impl Frontmatter {
    /// Parse a full Markdown file into `(frontmatter, body)`. Empty frontmatter + full input as body
    /// when no leading `---` block is present.
    #[must_use]
    pub fn split_markdown(input: &str) -> (Self, String) {
        let trimmed = input.strip_prefix(FM_DELIM);
        let Some(rest) = trimmed else {
            return (Self::default(), input.to_string());
        };
        // skip the newline right after the first ---
        let rest = rest.strip_prefix('\n').unwrap_or(rest);
        let Some(end_idx) = rest.find("\n---") else {
            // No closing delimiter: treat as no frontmatter.
            return (Self::default(), input.to_string());
        };
        let yaml_block = &rest[..end_idx];
        // body starts after the closing `\n---\n`
        let body_start = end_idx + "\n---".len();
        let body = rest
            .get(body_start..)
            .map(|s| s.trim_start_matches('\n').to_string())
            .unwrap_or_default();
        (Self::parse(yaml_block), body)
    }

    /// Parse a YAML block (between the `---` fences) into key/value pairs.
    #[must_use]
    pub fn parse(yaml_block: &str) -> Self {
        let mut fields = BTreeMap::new();
        for raw_line in yaml_block.lines() {
            let line = raw_line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_string();
            let mut value = value.trim().to_string();
            // strip matching surrounding quotes
            if value.len() >= 2 {
                let first = value.chars().next();
                let last = value.chars().last();
                if matches!(first, Some('"') | Some('\'')) && first == last {
                    value = value[1..value.len() - 1].to_string();
                }
            }
            if !key.is_empty() {
                fields.insert(key, value);
            }
        }
        Self { fields }
    }

    /// Get a field value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    /// Set a field value.
    pub fn set(&mut self, key: &str, value: &str) {
        self.fields.insert(key.to_string(), value.to_string());
    }

    /// Serialize to a YAML block (without `---` fences). Fields in canonical order, then alphabetical extras.
    #[must_use]
    pub fn to_yaml(&self) -> String {
        let mut out = String::new();
        // Stable, human-friendly order: canonical schema then alphabetical extras.
        let canonical = [
            "id",
            "title",
            "type",
            "slug",
            "summary",
            "created",
            "updated",
            "status",
            "source_articles",
            "source_file",
            "source_kind",
            "source_hash",
            "authors",
            "year",
            "journal",
            "doi",
            "keywords",
            "tags",
            "labels",
            "links",
            "content_source",
            "llm_model",
        ];
        let mut emitted = std::collections::HashSet::new();
        for key in canonical {
            if let Some(val) = self.fields.get(key) {
                out.push_str(&format_line(key, val));
                emitted.insert(key.to_string());
            }
        }
        // Any extra fields, alphabetical.
        let mut extras: Vec<&String> =
            self.fields.keys().filter(|k| !emitted.contains(k.as_str())).collect();
        extras.sort();
        for key in extras {
            if let Some(val) = self.fields.get(key) {
                out.push_str(&format_line(key, val));
            }
        }
        out
    }

    /// Render the full Markdown file: `---\n<yaml>\n---\n<body>`.
    #[must_use]
    pub fn to_markdown(&self, body: &str) -> String {
        let mut out = String::new();
        out.push_str(FM_DELIM);
        out.push('\n');
        out.push_str(&self.to_yaml());
        out.push_str(FM_DELIM);
        out.push('\n');
        if !body.is_empty() {
            out.push('\n');
            out.push_str(body);
        }
        out
    }
}

/// Format a single `key: value` line, quoting when the value needs it.
fn format_line(key: &str, value: &str) -> String {
    // Values that are already structured (lists `[...]`, or raw placeholders
    // like `<uuid>`) are emitted unquoted. Strings with spaces get quoted.
    let needs_quotes = !value.is_empty()
        && !value.starts_with('[')
        && !value.starts_with('<')
        && (value.contains(' ') || value.contains(':') || value.contains('#'));
    if needs_quotes {
        format!("{}: \"{}\"\n", key, value.replace('"', "\\\""))
    } else {
        format!("{}: {}\n", key, value)
    }
}

/// Parse a list-looking value `[a, b, c]` into trimmed members. Empty vec for non-list values.
#[must_use]
pub fn parse_list(value: &str) -> Vec<String> {
    let v = value.trim();
    let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return Vec::new();
    };
    inner
        .split(',')
        .map(|s| {
            let s = s.trim();
            if s.len() >= 2 {
                let first = s.chars().next();
                let last = s.chars().last();
                if matches!(first, Some('"') | Some('\'')) && first == last {
                    return s[1..s.len() - 1].to_string();
                }
            }
            s.to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Append a run entry to `wiki/log.md`. Creates the `# Wiki Audit Log` header if the body is empty.
#[must_use]
pub fn append_log_entry(body: &str, entry: &str) -> String {
    let mut out = if body.trim().is_empty() {
        "# Wiki Audit Log\n\nAppend-only record of ingest and lint runs.\n\n".to_string()
    } else {
        body.to_string()
    };
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("- ");
    out.push_str(entry);
    out.push('\n');
    out
}

/// Read a Markdown file and split into frontmatter + body.
pub fn read_file(path: &std::path::Path) -> Result<(Frontmatter, String), AppError> {
    let content = std::fs::read_to_string(path)?;
    Ok(Frontmatter::split_markdown(&content))
}

/// Write a Markdown file from frontmatter + body.
pub fn write_file(path: &std::path::Path, fm: &Frontmatter, body: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, fm.to_markdown(body))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_markdown_extracts_frontmatter_and_body() {
        let input = "---\ntitle: \"Hello\"\nstatus: draft\n---\n\n# Body\nText.\n";
        let (fm, body) = Frontmatter::split_markdown(input);
        assert_eq!(fm.get("title"), Some("Hello"));
        assert_eq!(fm.get("status"), Some("draft"));
        assert!(body.starts_with("# Body"));
        assert!(body.contains("Text."));
    }

    #[test]
    fn split_markdown_no_frontmatter() {
        let input = "# Just body\n";
        let (fm, body) = Frontmatter::split_markdown(input);
        assert!(fm.fields.is_empty());
        assert_eq!(body, input);
    }

    #[test]
    fn split_markdown_no_closing_delim_falls_back_to_body() {
        let input = "---\ntitle: oops\n";
        let (fm, body) = Frontmatter::split_markdown(input);
        assert!(fm.fields.is_empty());
        assert_eq!(body, input);
    }

    #[test]
    fn parse_strips_quotes() {
        let fm = Frontmatter::parse("title: \"My Title\"\nslug: my-slug");
        assert_eq!(fm.get("title"), Some("My Title"));
        assert_eq!(fm.get("slug"), Some("my-slug"));
    }

    #[test]
    fn parse_handles_lists_as_raw() {
        let fm = Frontmatter::parse("tags: [a, b, c]\nlinks: [\"[[x]]\", \"[[y]]\"]");
        assert_eq!(fm.get("tags"), Some("[a, b, c]"));
        let tags = parse_list(fm.get("tags").unwrap_or(""));
        assert_eq!(tags, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let links = parse_list(fm.get("links").unwrap_or(""));
        assert_eq!(links, vec!["[[x]]".to_string(), "[[y]]".to_string()]);
    }

    #[test]
    fn parse_skips_blank_and_comment_lines() {
        let fm = Frontmatter::parse("\n# a comment\ntitle: x\n\n");
        assert_eq!(fm.fields.len(), 1);
        assert_eq!(fm.get("title"), Some("x"));
    }

    #[test]
    fn to_yaml_emits_canonical_order_and_quotes_spaces() {
        let mut fm = Frontmatter::default();
        fm.set("title", "Hello World");
        fm.set("status", "draft");
        fm.set("slug", "hello");
        let yaml = fm.to_yaml();
        // title (quoted, has space) before status before slug (canonical order)
        // Canonical order: id, title, type, slug, summary, created, updated, status, ...
        let title_idx = yaml.find("title:").unwrap();
        let slug_idx = yaml.find("slug:").unwrap();
        let status_idx = yaml.find("status:").unwrap();
        assert!(title_idx < slug_idx);
        assert!(slug_idx < status_idx);
        assert!(yaml.contains("title: \"Hello World\""));
        assert!(yaml.contains("slug: hello"));
    }

    #[test]
    fn round_trip_preserves_semantics() {
        let mut fm = Frontmatter::default();
        fm.set("title", "Round Trip");
        fm.set("tags", "[a, b]");
        fm.set("status", "reviewed");
        let md = fm.to_markdown("# Body\n");
        let (fm2, body2) = Frontmatter::split_markdown(&md);
        assert_eq!(fm2.get("title"), Some("Round Trip"));
        assert_eq!(fm2.get("status"), Some("reviewed"));
        assert_eq!(parse_list(fm2.get("tags").unwrap_or("")), vec!["a", "b"]);
        assert!(body2.contains("# Body"));
    }

    #[test]
    fn append_log_entry_creates_header_when_empty() {
        let out = append_log_entry("", "2026-06-19 ingest: 5 pages");
        assert!(out.contains("# Wiki Audit Log"));
        assert!(out.contains("- 2026-06-19 ingest: 5 pages"));
    }

    #[test]
    fn append_log_entry_appends_to_existing() {
        let existing = "# Wiki Audit Log\n\n- first\n";
        let out = append_log_entry(existing, "second");
        assert!(out.contains("- first\n"));
        assert!(out.contains("- second\n"));
    }

    #[test]
    fn write_and_read_file_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("page.md");
        let mut fm = Frontmatter::default();
        fm.set("title", "File Test");
        fm.set("status", "draft");
        write_file(&path, &fm, "# Hello\n").unwrap();
        let (fm2, body) = read_file(&path).unwrap();
        assert_eq!(fm2.get("title"), Some("File Test"));
        assert_eq!(fm2.get("status"), Some("draft"));
        assert!(body.contains("# Hello"));
    }
}
