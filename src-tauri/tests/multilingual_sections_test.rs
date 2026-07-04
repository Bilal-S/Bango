//! Multilingual section classification tests (language-plan-v2 Phase 3).

use bango_lib::utils::sections::{classify_sections, SectionKind};

/// Assert that a heading line classifies to the expected `SectionKind` when
/// embedded in a minimal document.
fn assert_heading_kind(heading: &str, expected: SectionKind) {
    let text = format!("{heading}\nbody text for the section.\n");
    let sections = classify_sections(&text);
    assert!(
        sections.iter().any(|s| s.kind == expected),
        "heading {heading:?} should classify as {expected:?}; got: {sections:?}"
    );
}

#[test]
fn localized_headings_map_to_section_kind() {
    // TC-02: localized heading keywords (FR/ES/JA/ZH/DE/RU/PT/IT/AR/TR) map to
    // the right SectionKind. Tests one representative heading per language per
    // section kind.
    // French
    assert_heading_kind("Introduction", SectionKind::Introduction);
    assert_heading_kind("Conclusion", SectionKind::Conclusion);
    assert_heading_kind("Références", SectionKind::References);
    // Spanish
    assert_heading_kind("Introducción", SectionKind::Introduction);
    assert_heading_kind("Método", SectionKind::Methods);
    assert_heading_kind("Resultados", SectionKind::Results);
    assert_heading_kind("Discusión", SectionKind::Discussion);
    assert_heading_kind("Conclusión", SectionKind::Conclusion);
    assert_heading_kind("Referencias", SectionKind::References);
    // Japanese
    assert_heading_kind("はじめに", SectionKind::Introduction);
    assert_heading_kind("方法", SectionKind::Methods);
    assert_heading_kind("結果", SectionKind::Results);
    assert_heading_kind("おわりに", SectionKind::Conclusion);
    assert_heading_kind("参考文献", SectionKind::References);
    // Chinese
    assert_heading_kind("引言", SectionKind::Introduction);
    assert_heading_kind("结论", SectionKind::Conclusion);
    assert_heading_kind("参考文献", SectionKind::References);
    // German
    assert_heading_kind("Einleitung", SectionKind::Introduction);
    assert_heading_kind("Ergebnisse", SectionKind::Results);
    assert_heading_kind("Diskussion", SectionKind::Discussion);
    assert_heading_kind("Fazit", SectionKind::Conclusion);
    assert_heading_kind("Literatur", SectionKind::References);
    // Russian
    assert_heading_kind("Введение", SectionKind::Introduction);
    assert_heading_kind("Методы", SectionKind::Methods);
    assert_heading_kind("Результаты", SectionKind::Results);
    assert_heading_kind("Обсуждение", SectionKind::Discussion);
    assert_heading_kind("Заключение", SectionKind::Conclusion);
    assert_heading_kind("Список литературы", SectionKind::References);
    // Portuguese
    assert_heading_kind("Introdução", SectionKind::Introduction);
    assert_heading_kind("Métodos", SectionKind::Methods);
    assert_heading_kind("Resultados", SectionKind::Results);
    assert_heading_kind("Discussão", SectionKind::Discussion);
    assert_heading_kind("Conclusões", SectionKind::Conclusion);
    assert_heading_kind("Referências", SectionKind::References);
    // Italian
    assert_heading_kind("Introduzione", SectionKind::Introduction);
    assert_heading_kind("Risultati", SectionKind::Results);
    assert_heading_kind("Discussione", SectionKind::Discussion);
    assert_heading_kind("Conclusioni", SectionKind::Conclusion);
    assert_heading_kind("Bibliografia", SectionKind::References);
    // Arabic
    assert_heading_kind("مقدمة", SectionKind::Introduction);
    assert_heading_kind("النتائج", SectionKind::Results);
    assert_heading_kind("الخاتمة", SectionKind::Conclusion);
    assert_heading_kind("المراجع", SectionKind::References);
    // Turkish
    assert_heading_kind("Giriş", SectionKind::Introduction);
    assert_heading_kind("Yöntem", SectionKind::Methods);
    assert_heading_kind("Sonuçlar", SectionKind::Results);
    assert_heading_kind("Tartışma", SectionKind::Discussion);
    assert_heading_kind("Sonuç", SectionKind::Conclusion);
    assert_heading_kind("Kaynakça", SectionKind::References);
}

#[test]
fn unicode_numbered_headings_are_detected() {
    // TC-03: Unicode-aware numbered heading detection. Russian headings from
    // the manifest (e.g. `1 Введение`, `2 Обзор связанных работ`) must be
    // detected as headings so they bound sections.
    let text =
        "1 Введение\nIntro body about the study.\n\n2 Обзор связанных работ\nReview body text.\n";
    let sections = classify_sections(text);
    assert!(
        sections.len() >= 2,
        "expected at least 2 sections from numbered Unicode headings, got {}: {sections:?}",
        sections.len()
    );
    assert!(
        sections.iter().any(|s| s.kind == SectionKind::Introduction),
        "first numbered heading should classify as Introduction: {sections:?}"
    );
}
