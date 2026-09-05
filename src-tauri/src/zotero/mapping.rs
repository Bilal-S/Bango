//! Pure Zotero item -> `RisRecord` mapping, tag sanitization, and
//! attachment-candidate filtering. No I/O; every helper is unit-tested on
//! every platform.

use std::collections::HashMap;

use super::{ZoteroChildData, ZoteroChildItem, ZoteroCreator, ZoteroItem, ZoteroNoteItem};
use crate::ris::doi::normalize_doi;
use crate::ris::types::RisRecord;
use crate::screening::tags_labels::{truncate_at_word_boundary, MAX_NEW_TAG_LABEL_LEN};

/// Zotero itemType -> canonical RIS reference type (the same codes
/// `ris::parser::normalize_reference_type` produces for known inputs).
/// `None` for unsupported types (attachment, note, webpage, ...): the record
/// is skipped and surfaced through the "Unsupported Zotero item type" error
/// group.
#[must_use]
pub fn map_item_type_to_ris_type(item_type: &str) -> Option<&'static str> {
    match item_type {
        "journalArticle" => Some("JOUR"),
        "conferencePaper" => Some("CONF"),
        // No preprint code exists in RIS or the normalizer.
        "preprint" => Some("GEN"),
        "book" => Some("BOOK"),
        "bookSection" => Some("CHAP"),
        "thesis" => Some("THES"),
        "report" => Some("RPRT"),
        "document" => Some("GEN"),
        "manuscript" => Some("GEN"),
        "encyclopediaArticle" => Some("ENCYC"),
        "dictionaryEntry" => Some("DICT"),
        "newspaperArticle" => Some("NEWS"),
        // Valid RIS code; the normalizer passes it through.
        "magazineArticle" => Some("MGZN"),
        _ => None,
    }
}

/// Sanitize a Zotero tag into a Bango tag name (spec 2.1 rules): strip
/// `inclusion:`/`exclusion:` prefixes, lowercase, spaces and punctuation to
/// hyphens (collapsed), cap at 35 chars truncating at the last word boundary
/// (never mid-word). Empty results are dropped (`None`).
#[must_use]
pub fn sanitize_zotero_tag(raw: &str) -> Option<String> {
    let lower = raw.trim().to_lowercase();
    let stripped = lower
        .strip_prefix("inclusion:")
        .or_else(|| lower.strip_prefix("exclusion:"))
        .map(str::trim_start)
        .unwrap_or(&lower);

    let mut out = String::new();
    let mut last_hyphen = false;
    for c in stripped.chars() {
        if c.is_alphanumeric() {
            out.push(c);
            last_hyphen = false;
        } else if !last_hyphen {
            // Whitespace / punctuation / hyphens -> a single (collapsed) hyphen.
            out.push('-');
            last_hyphen = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        return None;
    }
    let sanitized = truncate_at_word_boundary(trimmed, MAX_NEW_TAG_LABEL_LEN);
    let sanitized = sanitized.trim_matches('-');
    (!sanitized.is_empty()).then(|| sanitized.to_string())
}

/// Split a Zotero `pages` value ("1-10", "1--10", "1 - 10", "5") into
/// start/end page strings.
#[must_use]
pub fn parse_pages(pages: &str) -> (Option<String>, Option<String>) {
    let cleaned = pages.trim();
    if cleaned.is_empty() {
        return (None, None);
    }
    // Normalize en/em dashes and double hyphens to a single '-'.
    let normalized: String = cleaned
        .chars()
        .map(|c| match c {
            '–' | '—' | '−' => '-',
            other => other,
        })
        .collect();
    let single = normalized.replace("--", "-");
    let (start, end) = match single.split_once('-') {
        Some((s, e)) => (s.trim(), e.trim()),
        None => (single.as_str(), ""),
    };
    ((!start.is_empty()).then(|| start.to_string()), (!end.is_empty()).then(|| end.to_string()))
}

/// Extract the 4-digit year from `meta.parsedDate` (`YYYY-MM-DD`, `YYYY-MM`,
/// or `YYYY`).
#[must_use]
pub fn extract_year(parsed_date: &str) -> Option<i32> {
    let year_part = parsed_date.split('-').next()?.trim();
    if year_part.len() != 4 {
        return None;
    }
    year_part.parse::<i32>().ok()
}

/// Creators -> "Lastname, Firstname" strings. `creatorType = author` is
/// preferred; when no authors exist, editors are used instead. A single-field
/// `name` (institutional author) is used verbatim.
#[must_use]
pub fn map_creators(creators: &[ZoteroCreator]) -> Vec<String> {
    for wanted in ["author", "editor"] {
        let mut out: Vec<String> = Vec::new();
        for creator in creators {
            if creator.creator_type != wanted {
                continue;
            }
            if let Some(name) = creator.name.as_deref() {
                if !name.trim().is_empty() {
                    out.push(name.trim().to_string());
                }
                continue;
            }
            let first = creator.first_name.as_deref().unwrap_or("").trim();
            let last = creator.last_name.as_deref().unwrap_or("").trim();
            if first.is_empty() && last.is_empty() {
                continue;
            }
            if last.is_empty() {
                out.push(first.to_string());
            } else if first.is_empty() {
                out.push(last.to_string());
            } else {
                out.push(format!("{last}, {first}"));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

/// Map a Zotero item to a `RisRecord`. Returns `None` for unsupported item
/// types (attachments, notes, webpages, ...). Zotero tags are deliberately
/// NOT written to `keywords` - they flow to Bango tags post-insert (one
/// representation, no double counting in keyword networks or exports).
#[must_use]
pub fn map_item_to_ris_record(item: &ZoteroItem) -> Option<RisRecord> {
    let reference_type = map_item_type_to_ris_type(&item.data.item_type)?.to_string();
    let (start_page, end_page) = parse_pages(item.data.pages.as_deref().unwrap_or(""));
    Some(RisRecord {
        reference_type: Some(reference_type),
        title: item.data.title.clone(),
        abstract_text: item.data.abstract_note.clone(),
        authors: map_creators(&item.data.creators),
        publication_year: item.meta.parsed_date.as_deref().and_then(extract_year),
        doi: normalize_doi(item.data.doi.as_deref()),
        journal: item.data.publication_title.clone(),
        volume: item.data.volume.clone(),
        issue: item.data.issue.clone(),
        start_page,
        end_page,
        keywords: Vec::new(),
        url: item.data.url.clone(),
        language: item.data.language.clone(),
        publisher: item.data.publisher.clone(),
        publisher_city: item.data.place.clone(),
        issn: item.data.issn.clone(),
        date: item.data.date.clone(),
        notes: item.data.extra.clone(),
        ..RisRecord::default()
    })
}

/// Zotero note HTML -> plain text: `<br>` and block-element boundaries become
/// newlines, every other tag is dropped, and the common named/numeric
/// entities decode. Runs of newlines collapse to ONE newline (paragraph gaps
/// included) so the text never contains a blank line - blank lines are the
/// block separator of the merged user-notes format and must stay unambiguous.
/// The result is trimmed.
#[must_use]
pub fn note_html_to_text(html: &str) -> String {
    let decoded = decode_entities(&drop_tags_keep_breaks(html));
    let mut collapsed = String::with_capacity(decoded.len());
    let mut previous_was_newline = false;
    for c in decoded.chars() {
        if c == '\n' {
            if !previous_was_newline {
                collapsed.push('\n');
            }
            previous_was_newline = true;
        } else {
            previous_was_newline = false;
            collapsed.push(c);
        }
    }
    collapsed.trim().to_string()
}

/// Drop every tag, keeping one newline per line-break/block tag (`<br>`,
/// `<p>`, headings, list items, ...). Malformed unterminated tags simply end
/// at the string's end.
fn drop_tags_keep_breaks(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut chars = html.chars();
    while let Some(c) = chars.next() {
        if c != '<' {
            text.push(c);
            continue;
        }
        let mut tag = String::new();
        for tc in chars.by_ref() {
            if tc == '>' {
                break;
            }
            tag.push(tc);
        }
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "br" | "p"
                | "div"
                | "li"
                | "blockquote"
                | "pre"
                | "tr"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        ) {
            text.push('\n');
        }
    }
    text
}

/// Decode every recognizable entity (`&amp;`-style named + decimal/hex
/// numeric); anything else passes through verbatim.
fn decode_entities(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '&' {
            let limit = std::cmp::min(index + 11, chars.len());
            if let Some(semi) = (index + 1..limit).find(|&j| chars[j] == ';') {
                let entity: String = chars[index + 1..semi].iter().collect();
                if let Some(decoded) = decode_entity(&entity) {
                    out.push(decoded);
                    index = semi + 1;
                    continue;
                }
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

/// One entity body (without `&`/`;`) -> its character, if recognized.
fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let number = entity.strip_prefix('#')?;
            let code =
                if let Some(hex) = number.strip_prefix('x').or_else(|| number.strip_prefix('X')) {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    number.parse::<u32>().ok()
                };
            code.and_then(char::from_u32)
        }
    }
}

/// Merge a parent item's child notes into the single Bango user-notes text.
/// Notes are ordered by `data.dateAdded` (oldest first; ISO-8601 strings sort
/// chronologically, missing values first). Each note contributes one block:
/// its first line (the title), a `---` separator line, then the remaining
/// body text; blocks are joined by one blank line. Returns `None` when no
/// note has non-empty text.
#[must_use]
pub fn merge_child_notes(notes: &[&ZoteroNoteItem]) -> Option<String> {
    let mut ordered: Vec<&ZoteroNoteItem> = notes.to_vec();
    ordered.sort_by_key(|note| note.data.date_added.clone().unwrap_or_default());
    let mut blocks: Vec<String> = Vec::new();
    for note in ordered {
        let text = note_html_to_text(note.data.note.as_deref().unwrap_or_default());
        if text.is_empty() {
            continue;
        }
        let mut lines = text.lines();
        let title = lines.next().unwrap_or_default().trim();
        let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
        if body.is_empty() {
            blocks.push(format!("{title}\n---"));
        } else {
            blocks.push(format!("{title}\n---\n{body}"));
        }
    }
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

/// Group child notes by their parent item key.
#[must_use]
pub fn group_notes_by_parent(notes: &[ZoteroNoteItem]) -> HashMap<String, Vec<&ZoteroNoteItem>> {
    let mut map: HashMap<String, Vec<&ZoteroNoteItem>> = HashMap::new();
    for note in notes {
        if let Some(parent) = note.data.parent_item.as_deref() {
            map.entry(parent.to_string()).or_default().push(note);
        }
    }
    map
}

/// True when a child attachment is a full-text candidate: a local-file
/// attachment (`imported_file` / `linked_file` / `imported_url` - live Zotero
/// 10 stores connector-saved PDFs as `imported_url` with a real storage file)
/// whose contentType is `application/pdf` / `text/plain` OR whose filename
/// ends in `.pdf`/`.txt`. Anything else could not pass
/// `attach_full_text_inner` (which hard-errors on non-pdf/txt), so it is
/// skipped and counted.
#[must_use]
pub fn is_full_text_candidate(child: &ZoteroChildData) -> bool {
    let link_ok = matches!(
        child.link_mode.as_deref(),
        Some("imported_file") | Some("linked_file") | Some("imported_url")
    );
    if !link_ok {
        return false;
    }
    let content_type = child.content_type.as_deref().unwrap_or_default();
    if content_type == "application/pdf" || content_type == "text/plain" {
        return true;
    }
    let filename = child.filename.as_deref().unwrap_or_default().to_lowercase();
    filename.ends_with(".pdf") || filename.ends_with(".txt")
}

/// Group child attachments by their parent item key.
#[must_use]
pub fn group_attachments_by_parent(
    children: &[ZoteroChildItem],
) -> HashMap<String, Vec<&ZoteroChildItem>> {
    let mut map: HashMap<String, Vec<&ZoteroChildItem>> = HashMap::new();
    for child in children {
        if let Some(parent) = child.data.parent_item.as_deref() {
            map.entry(parent.to_string()).or_default().push(child);
        }
    }
    map
}

/// The first pdf/txt attachment candidate among a parent's children, if any.
#[must_use]
pub fn first_full_text_candidate<'a>(
    children: &[&'a ZoteroChildItem],
) -> Option<&'a ZoteroChildItem> {
    children.iter().copied().find(|child| is_full_text_candidate(&child.data))
}
