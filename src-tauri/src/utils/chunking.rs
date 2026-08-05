/*! Semantic chunking of classified sections into FTS5-indexable chunks.

Walks `Section`s and emits `Chunk`s bounded by target word counts. Used by
`wiki::fts::collect_page_rows` so Wiki Chat BM25 returns the relevant passage.

Pure, `#[must_use]`, no I/O, no DB. */

use crate::utils::sections::{Section, SectionKind};

/// Default target chunk length in words (mirrors Chunkr's `target_length`).
pub const DEFAULT_CHUNK_WORDS: usize = 512;

/// Don't emit tiny tail chunks; merge anything shorter into the previous chunk.
pub const MIN_CHUNK_WORDS: usize = 100;

/// Hard cap to bound BM25 row size. Sections longer than this are split at
/// sentence boundaries into <= `target_words` chunks.
pub const MAX_CHUNK_WORDS: usize = 1200;

/// A chunk of text derived from one `Section`, carrying its section provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// The section this chunk came from, e.g. `Some("Methods")`.
    /// `None` for chunks derived from `SectionKind::Text` with no heading.
    pub section: Option<String>,
    /// 0-based ordinal within the parent document (contiguous across sections).
    pub chunk_index: usize,
    pub text: String,
    pub word_count: usize,
}

/** Split `Section`s into `Chunk`s, skipping References and empty bodies.

Sections <= `target_words` become one chunk. Longer sections are split at
sentence boundaries, with tiny tails (< `MIN_CHUNK_WORDS`) merged into the
previous piece. Hard-cap at `MAX_CHUNK_WORDS`: overlong pieces without sentence
breaks get word-sliced. `chunk_index` is contiguous across the document.
`section` carries provenance for `(§Methods)` FTS5 citations. */
#[must_use]
pub fn chunk_sections(sections: &[Section], target_words: usize) -> Vec<Chunk> {
    let target = target_words.clamp(1, MAX_CHUNK_WORDS);
    let mut chunks = Vec::new();
    let mut chunk_index = 0usize;

    for section in sections {
        // Skip references entirely (they must never surface in RAG).
        if section.kind == SectionKind::References {
            continue;
        }
        // Skip empty bodies (keeps the index lean).
        if section.body.trim().is_empty() {
            continue;
        }

        let section_label = section_label_for(section);

        // Atomic Table/Figure arm (T2.2 Phase 1): a GFM table or a figure
        // caption block must NEVER be split across chunks (it would break the
        // Markdown table structure). Emit the whole section as a single chunk
        // regardless of `MAX_CHUNK_WORDS`.
        if section.kind == SectionKind::Table || section.kind == SectionKind::Figure {
            let piece = section.body.trim();
            let word_count = piece.split_whitespace().count();
            if word_count > 0 {
                chunks.push(Chunk {
                    section: section_label.clone(),
                    chunk_index,
                    text: piece.to_string(),
                    word_count,
                });
                chunk_index += 1;
            }
            continue;
        }

        let pieces = split_section_body(&section.body, target);

        for piece in pieces {
            let word_count = piece.split_whitespace().count();
            if word_count == 0 {
                continue;
            }
            chunks.push(Chunk {
                section: section_label.clone(),
                chunk_index,
                text: piece,
                word_count,
            });
            chunk_index += 1;
        }
    }

    // Merge a trailing chunk shorter than MIN_CHUNK_WORDS into the previous one.
    merge_tiny_tail(&mut chunks);

    chunks
}

/** Resolve the section label carried by a chunk.
Known sections return the kind name (so citations render `(§Methods)`).
`Heading` returns the heading text. `Text`/`References` → `None`. */
fn section_label_for(section: &Section) -> Option<String> {
    match section.kind {
        SectionKind::Methods => Some("Methods".to_string()),
        SectionKind::Results => Some("Results".to_string()),
        SectionKind::Discussion => Some("Discussion".to_string()),
        SectionKind::Conclusion => Some("Conclusion".to_string()),
        SectionKind::Introduction => Some("Introduction".to_string()),
        SectionKind::Abstract => Some("Abstract".to_string()),
        SectionKind::Heading => section.heading.clone(),
        SectionKind::Table => Some("Table".to_string()),
        SectionKind::Figure => Some("Figure".to_string()),
        SectionKind::Text => None,
        SectionKind::References => None,
    }
}

/// Split a section body into pieces of <= `target_words` words at sentence
/// boundaries. Falls back to hard word-slicing for pieces that exceed
/// `MAX_CHUNK_WORDS` with no sentence boundary inside.
fn split_section_body(body: &str, target: usize) -> Vec<String> {
    let body = body.trim();
    let total_words = body.split_whitespace().count();
    if total_words <= target {
        return vec![body.to_string()];
    }

    // Split into sentences at `.`, `!`, `?` followed by whitespace.
    let sentences = split_sentences(body);

    // Greedily pack sentences into pieces up to `target` words.
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_words = 0usize;

    for sentence in sentences {
        let sentence_words = sentence.split_whitespace().count();

        // If a single sentence is longer than MAX_CHUNK_WORDS, hard-slice it.
        if sentence_words > MAX_CHUNK_WORDS {
            // Flush the current piece first.
            if !current.is_empty() {
                pieces.push(std::mem::take(&mut current));
                current_words = 0;
            }
            for slice in hard_slice_words(&sentence, target) {
                pieces.push(slice);
            }
            continue;
        }

        // If adding this sentence would exceed the target and the current
        // piece is non-empty, flush first.
        if !current.is_empty() && current_words + sentence_words > target {
            pieces.push(std::mem::take(&mut current));
            current_words = 0;
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&sentence);
        current_words += sentence_words;
    }
    if !current.is_empty() {
        pieces.push(current);
    }

    pieces
}

/// Split text into sentences at `.`, `!`, `?` followed by whitespace.
///
/// Keeps the trailing punctuation with the sentence. Newlines also act as
/// sentence boundaries (tables / lists often have no terminal punctuation).
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        current.push(ch);
        if ch == '.' || ch == '!' || ch == '?' {
            // Consume any following quote/bracket, then break on whitespace.
            while let Some(&next) = chars.peek() {
                if matches!(next, '"' | '\'' | ')' | ']') {
                    // Safe: we just peeked a `Some`; consume it.
                    if let Some(ch) = chars.next() {
                        current.push(ch);
                    }
                } else {
                    break;
                }
            }
            if chars.peek().map(|c| c.is_whitespace()).unwrap_or(true) {
                sentences.push(std::mem::take(&mut current).trim().to_string());
            }
        } else if ch == '\n' {
            // Newline is also a boundary.
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let leftover = current.trim();
    if !leftover.is_empty() {
        sentences.push(leftover.to_string());
    }
    sentences
}

/// Hard-slice a long string into pieces of <= `target_words` words. Last resort
/// when a single sentence exceeds `MAX_CHUNK_WORDS`.
fn hard_slice_words(text: &str, target: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.chunks(target.max(1)).map(|chunk| chunk.join(" ")).collect()
}

/** Merge a trailing chunk shorter than `MIN_CHUNK_WORDS` into the previous one.
Only merges the very last chunk. Respects `MAX_CHUNK_WORDS`: if merging would
push the previous chunk over the cap, the tiny tail is left as-is. */
fn merge_tiny_tail(chunks: &mut Vec<Chunk>) {
    if chunks.len() < 2 {
        return;
    }
    let last_idx = chunks.len() - 1;
    if chunks[last_idx].word_count < MIN_CHUNK_WORDS {
        let prev_idx = last_idx - 1;
        // Respect MAX: skip the merge if it would push the previous chunk over
        // the hard cap. The tiny tail stays as its own chunk in that case.
        let combined = chunks[prev_idx].word_count + chunks[last_idx].word_count;
        if combined > MAX_CHUNK_WORDS {
            return;
        }
        // `pop` is `Some` because `chunks.len() >= 2` (guard at top).
        if let Some(last) = chunks.pop() {
            let prev = &mut chunks[prev_idx];
            prev.text.push(' ');
            prev.text.push_str(&last.text);
            prev.word_count += last.word_count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(kind: SectionKind, heading: Option<&str>, body: &str) -> Section {
        Section {
            kind,
            heading: heading.map(str::to_string),
            body: body.to_string(),
            word_count: body.split_whitespace().count(),
        }
    }

    fn word_list(n: usize) -> String {
        (0..n).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(chunk_sections(&[], DEFAULT_CHUNK_WORDS).is_empty());
    }

    #[test]
    fn single_short_section_one_chunk() {
        let s = section(SectionKind::Methods, Some("## Methods"), "one two three four five");
        let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].section.as_deref(), Some("Methods"));
        assert_eq!(chunks[0].word_count, 5);
    }

    #[test]
    fn skips_references_section() {
        let methods = section(SectionKind::Methods, Some("## Methods"), "m body");
        let refs = section(SectionKind::References, Some("## References"), "[1] ref");
        let chunks = chunk_sections(&[methods, refs], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].section.as_deref(), Some("Methods"));
    }

    #[test]
    fn long_section_split_at_sentence_boundaries() {
        // 5 sentences of ~120 words each = ~600 words. target=512 -> 2 chunks.
        let body = (0..5).map(|_| format!("{}.", word_list(120))).collect::<Vec<_>>().join(" ");
        let s = section(SectionKind::Methods, Some("## Methods"), &body);
        let chunks = chunk_sections(&[s], 512);
        assert!(chunks.len() >= 2, "should split long section: {} chunks", chunks.len());
        // Each chunk <= MAX_CHUNK_WORDS.
        for c in &chunks {
            assert!(
                c.word_count <= MAX_CHUNK_WORDS,
                "chunk {} exceeds MAX: {}",
                c.word_count,
                MAX_CHUNK_WORDS
            );
        }
        // chunk_index contiguous 0..n.
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.chunk_index, i);
        }
    }

    #[test]
    fn tiny_tail_merged_into_previous() {
        // Two sections: one big (~400 words) + one tiny (~10 words). With
        // target=512 the big section is one chunk, the tiny section is one
        // chunk below MIN -> merged.
        let big = section(SectionKind::Methods, Some("## Methods"), &word_list(400));
        let tiny = section(SectionKind::Results, Some("## Results"), &word_list(10));
        let chunks = chunk_sections(&[big, tiny], DEFAULT_CHUNK_WORDS);
        // The tiny tail should have been merged, so we have 1 chunk.
        assert_eq!(chunks.len(), 1, "tiny tail should merge: {} chunks", chunks.len());
        assert!(chunks[0].word_count >= MIN_CHUNK_WORDS);
    }

    #[test]
    fn text_section_has_no_section_label() {
        let s = section(SectionKind::Text, None, "just some prose body text");
        let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].section.is_none());
    }

    #[test]
    fn generic_heading_uses_heading_text_as_label() {
        let s = section(SectionKind::Heading, Some("2.1 Study Design"), "design body text here");
        let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].section.as_deref(), Some("2.1 Study Design"));
    }

    #[test]
    fn chunk_index_contiguous_across_sections() {
        // Each section is > MIN_CHUNK_WORDS so none is merged as a tiny tail.
        let s1 = section(SectionKind::Introduction, Some("## Introduction"), &word_list(150));
        let s2 = section(SectionKind::Methods, Some("## Methods"), &word_list(150));
        let s3 = section(SectionKind::Results, Some("## Results"), &word_list(150));
        let chunks = chunk_sections(&[s1, s2, s3], DEFAULT_CHUNK_WORDS);
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.chunk_index, i, "contiguous chunk_index");
        }
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn skips_empty_body_section() {
        let empty = section(SectionKind::Methods, Some("## Methods"), "   ");
        let real = section(SectionKind::Results, Some("## Results"), "real body");
        let chunks = chunk_sections(&[empty, real], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].section.as_deref(), Some("Results"));
    }

    // ── Tier 2 Phase 1: atomic Table/Figure arm ────────────────────────────

    #[test]
    fn chunk_sections_table_is_atomic() {
        // A Table section larger than MAX_CHUNK_WORDS must emit exactly 1 chunk
        // (never split, so the GFM table survives intact).
        let body = format!(
            "| col | val |\n| --- | --- |\n{}",
            (0..2000).map(|i| format!("| r{i} | v{i} |")).collect::<Vec<_>>().join("\n")
        );
        let s = section(SectionKind::Table, Some("Table 1"), &body);
        let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1, "table must be one chunk: {} chunks", chunks.len());
        // The chunk can exceed MAX_CHUNK_WORDS (atomic exception).
        assert!(chunks[0].word_count > MAX_CHUNK_WORDS, "table chunk should be > MAX");
    }

    #[test]
    fn chunk_sections_figure_is_atomic() {
        // A Figure section larger than MAX_CHUNK_WORDS must emit exactly 1 chunk.
        let body = (0..2000).map(|i| format!("caption word{i}")).collect::<Vec<_>>().join(" ");
        let s = section(SectionKind::Figure, Some("Figure 1"), &body);
        let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1, "figure must be one chunk: {} chunks", chunks.len());
        assert!(chunks[0].word_count > MAX_CHUNK_WORDS, "figure chunk should be > MAX");
    }

    #[test]
    fn chunk_sections_table_carries_section_label() {
        let body = "| a | b |\n| --- | --- |\n| c | d |";
        let s = section(SectionKind::Table, Some("Table 1"), body);
        let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].section.as_deref(), Some("Table"), "table chunk label");
    }

    #[test]
    fn chunk_sections_figure_carries_section_label() {
        let body = "A figure caption body.";
        let s = section(SectionKind::Figure, Some("Figure 1"), body);
        let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].section.as_deref(), Some("Figure"), "figure chunk label");
    }
}
