//! Standalone integration tests for `screening::evidence` (Tier 4.1).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` block in
//! `src/screening/evidence.rs` per CLAUDE.md lines 147-148:
//! "Avoid large inline unit tests in library source files... instead, move
//! them into standalone integration test files under `src-tauri/tests/`."

use bango_lib::screening::chunk_retrieval::ScoredChunk;
use bango_lib::screening::evidence::{resolve_evidence, EvidenceSource};

fn chunk(section: Option<&str>, content: &str, score: f64) -> ScoredChunk {
    ScoredChunk {
        chunk_index: 0,
        section: section.map(|s| s.to_string()),
        content: content.to_string(),
        score,
    }
}

fn summary_json(field: Option<&str>, extraction: &[(&str, &str)], digest: Option<&str>) -> String {
    let mut obj = serde_json::Map::new();
    if let Some(f) = field {
        obj.insert("field".to_string(), serde_json::Value::String(f.to_string()));
    }
    let extraction_obj: serde_json::Map<String, serde_json::Value> = extraction
        .iter()
        .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
        .collect();
    obj.insert("structured_extraction".to_string(), serde_json::Value::Object(extraction_obj));
    if let Some(d) = digest {
        obj.insert("summary_150_250_words".to_string(), serde_json::Value::String(d.to_string()));
    }
    serde_json::Value::Object(obj).to_string()
}

#[test]
fn resolve_evidence_prefers_summary_with_chunk_when_both_present() {
    let summary = summary_json(
        Some("medicine"),
        &[("study_type", "RCT"), ("population", "N=100")],
        Some("A digest."),
    );
    let chunks =
        vec![chunk(Some("Methods"), "We did X.", 1.0), chunk(Some("Results"), "We found Y.", 0.5)];
    let evidence = resolve_evidence(Some(&summary), &chunks);
    assert_eq!(evidence.source_type, EvidenceSource::AiSummaryWithChunk);
    // Only the top-1 chunk appears (complementarity).
    assert!(evidence.text.contains("[Source: Full Text - verbatim, §Methods]"));
    assert!(evidence.text.contains("We did X."));
    assert!(!evidence.text.contains("We found Y."));
}

#[test]
fn resolve_evidence_returns_summary_alone_when_no_chunks() {
    let summary = summary_json(Some("medicine"), &[("study_type", "RCT")], Some("A digest."));
    let evidence = resolve_evidence(Some(&summary), &[]);
    assert_eq!(evidence.source_type, EvidenceSource::AiSummaryAlone);
    assert!(!evidence.text.contains("[Source: Full Text"));
    assert!(evidence.text.contains("[Source: AI Summary - digest]"));
}

#[test]
fn resolve_evidence_falls_back_to_chunks_when_no_summary() {
    let chunks =
        vec![chunk(Some("Methods"), "We did X.", 1.0), chunk(Some("Results"), "We found Y.", 0.5)];
    let evidence = resolve_evidence(None, &chunks);
    assert_eq!(evidence.source_type, EvidenceSource::Chunks);
    assert!(evidence.text.contains("[§Methods] We did X."));
    assert!(evidence.text.contains("[§Results] We found Y."));
    assert!(!evidence.text.contains("[Source: AI Summary"));
}

#[test]
fn resolve_evidence_returns_none_when_both_absent() {
    let evidence = resolve_evidence(None, &[]);
    assert_eq!(evidence.source_type, EvidenceSource::None);
    assert!(evidence.text.is_empty());
}

#[test]
fn resolve_evidence_formats_structured_extraction_fields() {
    let summary = summary_json(
        Some("medicine"),
        &[("study_type", "RCT"), ("population", "N=1234 children")],
        Some("A digest."),
    );
    let evidence = resolve_evidence(Some(&summary), &[]);
    assert!(evidence.text.contains("[Source: AI Summary - structured extraction]"));
    assert!(evidence.text.contains("study_type: RCT"));
    assert!(evidence.text.contains("population: N=1234 children"));
}

#[test]
fn resolve_evidence_formats_single_top_chunk_for_summary_with_chunk() {
    let summary = summary_json(Some("medicine"), &[], Some("A digest."));
    let chunks = vec![
        chunk(Some("Methods"), "top chunk.", 1.0),
        chunk(Some("Results"), "second chunk.", 0.8),
        chunk(Some("Discussion"), "third chunk.", 0.6),
    ];
    let evidence = resolve_evidence(Some(&summary), &chunks);
    // Exactly one verbatim block (top-1 only).
    let verbatim_count = evidence.text.matches("[Source: Full Text - verbatim").count();
    assert_eq!(verbatim_count, 1, "complementarity sends top-1 chunk only");
    assert!(evidence.text.contains("top chunk."));
    assert!(!evidence.text.contains("second chunk."));
}

#[test]
fn resolve_evidence_handles_malformed_ai_summary_json() {
    // Malformed JSON -> falls back to chunks, no panic.
    let chunks = vec![chunk(Some("Methods"), "We did X.", 1.0)];
    let evidence = resolve_evidence(Some("not json at all"), &chunks);
    assert_eq!(evidence.source_type, EvidenceSource::Chunks);
}

#[test]
fn resolve_evidence_handles_summary_without_structured_extraction() {
    // Empty {} structured_extraction but has a digest -> summary-alone path.
    let summary = summary_json(Some("medicine"), &[], Some("A digest."));
    let evidence = resolve_evidence(Some(&summary), &[]);
    assert_eq!(evidence.source_type, EvidenceSource::AiSummaryAlone);
    // Digest-only block, still valid (no structured extraction lines).
    assert!(evidence.text.contains("[Source: AI Summary - digest]"));
    assert!(evidence.text.contains("A digest."));
}

#[test]
fn resolve_evidence_sections_label_includes_ai_summary_marker() {
    let summary = summary_json(Some("medicine"), &[("study_type", "RCT")], Some("d."));
    let chunks = vec![chunk(Some("Methods"), "x.", 1.0)];
    let evidence = resolve_evidence(Some(&summary), &chunks);
    assert!(
        evidence.sections_label.contains("AI Summary"),
        "sections_label must include AI Summary marker: {}",
        evidence.sections_label
    );
    assert!(evidence.sections_label.contains("§Methods"));
}

#[test]
fn resolve_evidence_chunks_path_sections_label_matches_tier3_format() {
    let chunks = vec![chunk(Some("Methods"), "a.", 1.0), chunk(Some("Results"), "b.", 0.5)];
    let evidence = resolve_evidence(None, &chunks);
    assert_eq!(evidence.sections_label, "§Methods, §Results");
}

#[test]
fn resolve_evidence_chunks_path_unchanged_from_tier3() {
    // The chunks-only body must be byte-identical to engine::format_chunks_as_evidence.
    let chunks = vec![
        chunk(Some("Methods"), "First chunk.", 1.0),
        chunk(Some("Results"), "Second chunk.", 0.7),
        chunk(None, "Unknown section chunk.", 0.3),
    ];
    let evidence = resolve_evidence(None, &chunks);
    let expected = bango_lib::screening::engine::format_chunks_as_evidence(&chunks);
    // `format_chunks_as_evidence` returns `Option<String>` (None on empty);
    // the chunks slice is non-empty here so it's `Some`.
    assert_eq!(evidence.text, expected.expect("non-empty chunks must produce Some"));
}

#[test]
fn resolve_evidence_skips_empty_structured_extraction_values() {
    let summary = summary_json(
        Some("medicine"),
        &[("study_type", "RCT"), ("population", "")],
        Some("A digest."),
    );
    let evidence = resolve_evidence(Some(&summary), &[]);
    assert!(evidence.text.contains("study_type: RCT"));
    // Empty population value must be skipped.
    assert!(!evidence.text.contains("population:"));
}
