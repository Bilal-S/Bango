//! Mojibake detection + recovery tests for legacy CJK PDFs without a
//! ToUnicode CMap.
//!
//! The pure-helper tests (`is_mojibake`, `c1_control_density`, `recover_mojibake`)
//! cover the detector logic on synthetic inputs (no I/O). The PDF-fixture
//! regression test `#[ignore]`'d because it loads a ~3 MB asset; run it with
//! `cargo test --test pdf_mojibake_test -- --ignored`.

use std::path::Path;

use bango_lib::utils::pdf_extract::{
    c1_control_density, extract_pdf_text, is_mojibake, recover_mojibake,
};

// ─── Pure-helper tests (no I/O) ─────────────────────────────────────────────

/// Build a synthetic mojibake string: take valid Japanese, encode it as
/// Shift-JIS, then cast each byte to a `char` (the same Latin-1 misinterpretation
/// `unpdf` performs on fonts without a ToUnicode CMap).
fn synthetic_sjis_mojibake(jp: &str) -> String {
    let bytes = encoding_rs::SHIFT_JIS.encode(jp).0;
    bytes.iter().map(|&b| b as char).collect()
}

#[test]
fn c1_density_is_zero_for_clean_ascii() {
    let en = "This study examines the effects of the policy on the population, and the \
              results indicate that the intervention was effective for the cohort of interest.";
    assert_eq!(c1_control_density(en), 0.0);
}

#[test]
fn c1_density_is_zero_for_clean_utf8_japanese() {
    // Valid UTF-8 Japanese text has no C1 control chars (U+0080-U+009F).
    let jp = "代謝症候群の定義と診断基準についてのレビュー。メタボリックシンドロームは\
              内臓脂肪の蓄積によりインスリン抵抗性、ブドウ糖不耐症、脂質異常症、高血圧などの\
              心血管疾患のリスク因子が集積する病態です。";
    assert_eq!(c1_control_density(jp), 0.0);
}

#[test]
fn c1_density_detects_sjis_mojibake() {
    let jp = "代謝症候群の定義と診断基準についてのレビュー。メタボリックシンドロームは\
              内臓脂肪の蓄積によりインスリン抵抗性、ブドウ糖不耐症、脂質異常症、高血圧などの\
              心血管疾患のリスク因子が集積する病態です。";
    let mojibake = synthetic_sjis_mojibake(jp);
    // The SJIS-encoded Japanese produces a very high C1 density (>30%).
    assert!(c1_control_density(&mojibake) > 0.30, "expected high C1 density for SJIS mojibake");
}

#[test]
fn is_mojibake_false_for_clean_english() {
    let en = "This study examines the effects of the policy on the population, and the \
              results indicate that the intervention was effective for the cohort of interest.";
    assert!(!is_mojibake(en));
}

#[test]
fn is_mojibake_false_for_clean_utf8_japanese() {
    let jp = "代謝症候群の定義と診断基準についてのレビュー。メタボリックシンドロームは\
              内臓脂肪の蓄積によりインスリン抵抗性、ブドウ糖不耐症、脂質異常症、高血圧などの\
              心血管疾患のリスク因子が集積する病態です。";
    assert!(!is_mojibake(jp));
}

#[test]
fn is_mojibake_false_for_short_text() {
    // Even with a couple of C1 chars, short text is below the min-chars guard.
    let short = "ab\u{0080}\u{0081} cd";
    assert!(!is_mojibake(short));
}

#[test]
fn is_mojibake_false_for_few_c1_chars_in_long_text() {
    // A long English text with a stray C1 char (e.g. from one malformed glyph)
    // must NOT trigger recovery: the absolute-count floor catches this.
    let mut text = String::from("This is a long English abstract. ").repeat(20);
    text.push('\u{0080}');
    text.push_str(&" and more text ".repeat(50));
    assert!(!is_mojibake(&text), "a single stray C1 char must not trigger mojibake");
}

#[test]
fn is_mojibake_true_for_sjis_mojibake() {
    let jp = "代謝症候群の定義と診断基準についてのレビュー。メタボリックシンドロームは\
              内臓脂肪の蓄積によりインスリン抵抗性、ブドウ糖不耐症、脂質異常症、高血圧などの\
              心血管疾患のリスク因子が集積する病態です。";
    let mojibake = synthetic_sjis_mojibake(jp);
    assert!(is_mojibake(&mojibake), "synthetic SJIS mojibake must be detected");
}

#[test]
fn recover_mojibake_recovers_sjis_japanese() {
    let jp = "代謝症候群の定義と診断基準についてのレビュー。メタボリックシンドロームは\
              内臓脂肪の蓄積によりインスリン抵抗性、ブドウ糖不耐症、脂質異常症、高血圧などの\
              心血管疾患のリスク因子が集積する病態です。";
    let mojibake = synthetic_sjis_mojibake(jp);
    let recovered = recover_mojibake(&mojibake);
    assert!(recovered.contains('代'), "recovered text must contain real kanji");
    assert!(recovered.contains('症'), "recovered text must contain real kanji");
    assert!(!is_mojibake(&recovered), "recovered text must not re-trigger mojibake");
}

#[test]
fn recover_mojibake_passthrough_clean_english() {
    let en = "This study examines the effects of the policy on the population, and the \
              results indicate that the intervention was effective for the cohort of interest.";
    assert_eq!(recover_mojibake(en), en);
}

#[test]
fn recover_mojibake_passthrough_clean_utf8_japanese() {
    let jp = "代謝症候群の定義と診断基準についてのレビュー。メタボリックシンドロームは\
              内臓脂肪の蓄積によりインスリン抵抗性、ブドウ糖不耐症、脂質異常症、高血圧などの\
              心血管疾患のリスク因子が集積する病態です。";
    assert_eq!(recover_mojibake(jp), jp);
}

#[test]
fn recover_mojibake_recovers_mixed_japanese_and_english_loanwords() {
    // Mirrors the naika fixture: SJIS Japanese interleaved with ASCII English
    // loanwords + digits. The C1 density is diluted by the ASCII content but
    // still well above the 0.5% threshold once the document is long enough.
    let jp_segment = "代謝症候群の定義について。";
    let en_segment = "World Health Organization (WHO) and NCEP ATP III ";
    let mut raw = String::new();
    for _ in 0..20 {
        raw.push_str(&synthetic_sjis_mojibake(jp_segment));
        raw.push(' ');
        raw.push_str(en_segment);
    }
    // Sanity: the synthetic input is detected as mojibake.
    assert!(is_mojibake(&raw), "mixed JP-loanword SJIS mojibake must be detected");
    let recovered = recover_mojibake(&raw);
    assert!(recovered.contains('代'), "recovered text must contain real kanji");
    assert!(recovered.contains("World Health Organization"), "English loanwords must survive");
    assert!(!is_mojibake(&recovered));
}

// ─── PDF fixture regression test (slow; uses a ~3 MB asset) ─────────────────

/// The legacy Japanese medical PDF whose fonts use Shift-JIS without a
/// ToUnicode CMap. Before the mojibake-recovery pass, `unpdf` returned
/// mojibake for this fixture (C1 control-char density 3.9%, e.g. `"ÍN"`
/// instead of `"は年"`); the recovery pass re-decodes it to real Japanese.
const NAIKA_PDF: &str = "../tests/assets/multilingual-oa/ja/10.2169_naika.94.794.pdf";

#[test]
#[ignore = "slow"] // slow; loads a ~3 MB PDF asset
fn extract_recovers_japanese_mojibake_from_naika_fixture() {
    let path = Path::new(NAIKA_PDF);
    if !path.exists() {
        eprintln!("Skipping: test asset not found at {NAIKA_PDF}");
        return;
    }
    let text = extract_pdf_text(path).expect("extraction must succeed");

    // The recovered text must contain real Japanese kana / kanji (the bytes
    // `0x82 0xCD` should decode to `は`, `0x94 0x4E` to `年`, etc.).
    let has_hiragana = text.chars().any(|c| ('\u{3040}'..='\u{309F}').contains(&c));
    let has_kanji = text.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c));
    assert!(
        has_hiragana || has_kanji,
        "recovered text must contain real Japanese kana or kanji, got: {}",
        text.chars().take(120).collect::<String>()
    );

    // The recovered text must NOT be flagged as mojibake by the detector.
    assert!(!is_mojibake(&text), "recovered text must not be flagged as mojibake");

    // The C1 control-char density must be ~0 after recovery (the original
    // mojibake was 3.9% C1).
    assert!(
        c1_control_density(&text) < 0.005,
        "recovered text must have near-zero C1 density, got {:.4}",
        c1_control_density(&text)
    );
}
