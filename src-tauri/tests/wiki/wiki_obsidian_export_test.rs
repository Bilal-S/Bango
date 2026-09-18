//! Integration tests for the Obsidian vault export (`wiki_export_obsidian`).
//!
//! These tests exercise the pure helpers in `bango_lib::wiki::obsidian_export`
//! (`build_article_slug_map`, `rewrite_frontmatter`, `rewrite_body`,
//! `write_vault`) plus the shared `zip_directory` helper directly, avoiding
//! the Tauri `State<DbState>` wrapper that cannot be unit-tested.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use bango_lib::commands::wiki_cmd::zip_directory;
use bango_lib::wiki::frontmatter::Frontmatter;
use bango_lib::wiki::obsidian_export::{
    article_alias, build_article_slug_map, rewrite_body, rewrite_frontmatter, write_vault,
    ArticleRow, ObsidianArticleMeta,
};
use tempfile::TempDir;

const ART_A: &str = "11111111-1111-4111-8111-111111111111";
const ART_B: &str = "22222222-2222-4222-8222-222222222222";
const ART_C: &str = "33333333-3333-4333-8333-333333333333";
const ART_UNMAPPED: &str = "99999999-9999-4999-8999-999999999999";

fn row(id: &str, authors: &[&str], year: Option<i32>, title: &str) -> ArticleRow {
    ArticleRow {
        id: id.to_string(),
        title: title.to_string(),
        year,
        authors: authors.iter().map(|a| (*a).to_string()).collect(),
    }
}

fn simple_map() -> HashMap<String, ObsidianArticleMeta> {
    build_article_slug_map(&[
        row(ART_A, &["Smith, John A."], Some(2023), "The Sugar Tax Effects"),
        row(ART_B, &["Doe, Jane", "Roe, Ann"], Some(2022), "Policy Lessons"),
    ])
}

/// Concept page exercising every rewrite surface: UUID wikilinks (with and
/// without alias), footnote refs + definition, user-doc ref, bare /raw/ path,
/// and the dropped internal frontmatter fields.
fn concept_page() -> String {
    format!(
        r#"---
id: sugar-tax
title: Sugar Tax
type: concept
slug: sugar-tax
status: draft
source_articles: ["{ART_A}"]
source_file: raw/{ART_A}.md
source_hash: deadbeef
links: ["[[{ART_A}]]", "[[user-notes]]"]
---

Body links: [[{ART_A}]] and [[{ART_A}|Custom Alias]] plus [[concept-other]].
Unmapped link [[{ART_UNMAPPED}]] and aliased [[{ART_UNMAPPED}|Kept Name]].
Refs: statement.[^art-{ART_A}] and user doc.[^art-user-notes]
Paths: /raw/{ART_A}.md and stale /raw/{ART_UNMAPPED}.md

[^art-{ART_A}]: /raw/{ART_A}.md
"#
    )
}

fn synthesis_page(id: &str, title: &str) -> String {
    format!(
        r#"---
id: {id}
title: {title}
type: synthesis
slug: {id}
status: draft
source_articles: ["{id}"]
links: ["[[sugar-tax]]"]
---

## Summary

Some digest.[^art-{id}]
"#
    )
}

fn author_page() -> String {
    r#"---
id: smith-john-a
title: John A. Smith
type: author
slug: smith-john-a
status: draft
source_articles: []
---

Author of [[sugar-tax]].
"#
    .to_string()
}

fn user_doc_page() -> String {
    r#"---
id: user-notes
title: My Notes
type: source
slug: user-notes
source_kind: user_text
source_articles: ["user-notes"]
---

Imported document. Self reference as [^art-user-notes].
"#
    .to_string()
}

/// Minimal wiki-root tree covering every page type + excluded artifacts.
fn make_wiki_root() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    for dir in
        ["wiki/concepts", "wiki/synthesis", "wiki/authors", "wiki/sources", "raw", "templates"]
    {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    fs::write(root.join("wiki/concepts/sugar-tax.md"), concept_page()).unwrap();
    fs::write(
        root.join("wiki/synthesis").join(format!("{ART_A}.md")),
        synthesis_page(ART_A, "The Sugar Tax Effects"),
    )
    .unwrap();
    fs::write(
        root.join("wiki/synthesis").join(format!("{ART_B}.md")),
        synthesis_page(ART_B, "Policy Lessons"),
    )
    .unwrap();
    fs::write(root.join("wiki/authors/smith-john-a.md"), author_page()).unwrap();
    fs::write(root.join("wiki/sources/user-notes.md"), user_doc_page()).unwrap();
    fs::write(root.join("wiki/log.md"), "# Audit Log").unwrap();
    fs::write(root.join("wiki/index.md"), "# catalog").unwrap();
    fs::write(root.join("raw").join(format!("{ART_A}.md")), "raw article text").unwrap();
    fs::write(root.join("templates/concept.md"), "template").unwrap();
    tmp
}

/// Recursively collect all files under `dir`.
fn walk_all(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// True when `s` contains a UUID-shaped token (8-4-4-4-12 hex).
fn contains_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 36 {
        return false;
    }
    (0..=b.len() - 36).any(|i| {
        b[i..i + 36].iter().enumerate().all(|(j, &c)| match j {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
    })
}

#[test]
fn slug_map_builds_author_year_title_keys() {
    let map = simple_map();
    let a = map.get(ART_A).unwrap();
    assert_eq!(a.slug, "smith2023sugar");
    assert_eq!(a.alias, "Smith 2023");
    let b = map.get(ART_B).unwrap();
    assert_eq!(b.slug, "doe2022policy");
    assert_eq!(b.alias, "Doe et al. 2022");
}

#[test]
fn slug_map_letter_suffix_dedup_and_fallbacks() {
    let map = build_article_slug_map(&[
        row(ART_A, &["Smith, J"], Some(2023), "Sugar Tax Effects"),
        row(ART_B, &["Smith, K"], Some(2023), "Sugar Tax Outcomes"),
        row(ART_C, &[], None, "Anonymous Study"),
    ]);
    assert_eq!(map.get(ART_A).unwrap().slug, "smith2023sugar");
    assert_eq!(map.get(ART_B).unwrap().slug, "smith2023sugarb");
    assert_eq!(map.get(ART_C).unwrap().slug, "anonndanonymous");

    // ASCII folding: accented surname + title word fold to base letters.
    let folded = build_article_slug_map(&[row(ART_A, &["Muller, H"], Some(2021), "Unicode Test")]);
    assert_eq!(folded.get(ART_A).unwrap().slug, "muller2021unicode");
}

#[test]
fn alias_format_multi_single_authorless() {
    assert_eq!(
        article_alias(&["Smith, J".to_string(), "Doe, A".to_string()], Some(2020)),
        "Smith et al. 2020"
    );
    assert_eq!(article_alias(&["Smith, J".to_string()], Some(2020)), "Smith 2020");
    assert_eq!(article_alias(&[], None), "Anon n.d.");
}

#[test]
fn rewrite_frontmatter_maps_uuids_drops_internal() {
    let map = simple_map();
    let mut fm = Frontmatter::default();
    fm.set("id", ART_A);
    fm.set("slug", ART_A);
    fm.set("title", "Concept Title");
    fm.set("source_articles", &format!("[\"{ART_A}\"]"));
    fm.set("links", &format!("[\"[[{ART_A}]]\", \"[[user-notes]]\", \"[[{ART_UNMAPPED}]]\"]"));
    fm.set("source_file", &format!("raw/{ART_A}.md"));
    fm.set("source_hash", "deadbeef");

    rewrite_frontmatter(&mut fm, &map, "fallback-stem");

    assert_eq!(fm.get("id"), Some("smith2023sugar"));
    assert_eq!(fm.get("slug"), Some("smith2023sugar"));
    assert_eq!(fm.get("title"), Some("Concept Title"));
    assert_eq!(fm.get("source_articles"), Some("[\"smith2023sugar\"]"));
    assert_eq!(fm.get("links"), Some("[\"[[smith2023sugar|Smith 2023]]\", \"[[user-notes]]\"]"));
    assert!(fm.get("source_file").is_none());
    assert!(fm.get("source_hash").is_none());
}

#[test]
fn rewrite_body_wikilinks_with_and_without_alias() {
    let map = simple_map();
    let body = format!(
        "See [[{ART_A}]] and [[{ART_A}|Custom Alias]] plus [[concept-other]].\nUnmapped [[{ART_UNMAPPED}]] and [[{ART_UNMAPPED}|Kept Name]].\n"
    );
    let out = rewrite_body(&body, &map);
    assert!(out.contains("[[smith2023sugar|Smith 2023]]"));
    assert!(out.contains("[[smith2023sugar|Custom Alias]]"));
    assert!(out.contains("[[concept-other]]"));
    assert!(out.contains("Kept Name"));
    assert!(!out.contains(ART_A));
    assert!(!out.contains(ART_UNMAPPED));
}

#[test]
fn rewrite_footnotes_rename_refs_and_definitions() {
    let map = simple_map();

    // Ref + existing definition: both renamed, definition becomes a wikilink.
    let body = format!(
        "Statement one.[^art-{ART_A}] and user doc.[^art-user-notes]\n\n[^art-{ART_A}]: /raw/{ART_A}.md\n"
    );
    let out = rewrite_body(&body, &map);
    assert!(out.contains("[^art-smith2023sugar]"));
    assert!(out.contains("[^art-smith2023sugar]: [[smith2023sugar|Smith 2023]]"));
    assert!(out.contains("[^art-user-notes]"));
    assert!(!out.contains("/raw/"));
    assert!(!out.contains(ART_A));

    // Ref with no definition: a definition block is appended.
    let body2 = format!("Statement two.[^art-{ART_B}]\n");
    let out2 = rewrite_body(&body2, &map);
    assert!(out2.contains("[^art-doe2022policy]"));
    assert!(out2.contains("[^art-doe2022policy]: [[doe2022policy|Doe et al. 2022]]"));

    // Unmapped ref: marker stripped, definition dropped, nothing appended.
    let body3 = format!(
        "Statement three.[^art-{ART_UNMAPPED}]\n\n[^art-{ART_UNMAPPED}]: /raw/{ART_UNMAPPED}.md\n"
    );
    let out3 = rewrite_body(&body3, &map);
    assert!(!out3.contains(ART_UNMAPPED));
    assert!(!out3.contains("[^art-"));
}

#[test]
fn rewrite_bare_raw_path_becomes_wikilink() {
    let map = simple_map();
    let body = format!("Full text at /raw/{ART_A}.md and stale /raw/{ART_UNMAPPED}.md.\n");
    let out = rewrite_body(&body, &map);
    assert!(out.contains("at [[smith2023sugar|Smith 2023]] and stale ."));
    assert!(!out.contains("/raw/"));
    assert!(!out.contains(ART_A));
}

#[test]
fn write_vault_renames_synthesis_and_excludes_internal() {
    let wiki_root = make_wiki_root();
    let staging = wiki_root.path().join("obsidian-export");
    let stats = write_vault(&wiki_root.path().join("wiki"), &staging, &simple_map()).unwrap();

    // Synthesis pages renamed to slugs; UUID filenames gone.
    assert!(staging.join("synthesis/smith2023sugar.md").exists());
    assert!(staging.join("synthesis/doe2022policy.md").exists());
    assert!(!staging.join(format!("synthesis/{ART_A}.md")).exists());
    assert_eq!(stats.renamed_synthesis, 2);

    // Internal artifacts excluded from the vault.
    assert!(!staging.join("log.md").exists());
    assert!(!staging.join("index.md").exists());
    assert!(!staging.join("raw").exists());
    assert!(!staging.join("templates").exists());

    // Vault furniture written.
    assert!(staging.join("Home.md").exists());
    assert!(staging.join(".obsidian/app.json").exists());
    assert!(staging.join(".obsidian/graph.json").exists());
    let home = fs::read_to_string(staging.join("Home.md")).unwrap();
    assert!(home.contains("## Concepts"));
    assert!(home.contains("[[sugar-tax|Sugar Tax]]"));
    assert!(home.contains("## Synthesis"));
    assert!(home.contains("[[smith2023sugar|"));

    // Concept page rewritten: frontmatter slugged, body cleaned.
    let concept = fs::read_to_string(staging.join("concepts/sugar-tax.md")).unwrap();
    assert!(concept.contains("source_articles: [\"smith2023sugar\"]"));
    assert!(!concept.contains("source_file"));
    assert!(concept.contains("[[smith2023sugar|Custom Alias]]"));
    assert!(concept.contains("[^art-user-notes]"));

    // concept + 2 synthesis + author + source pages.
    assert_eq!(stats.pages, 5);
}

#[test]
fn write_vault_orphaned_synthesis_fallback() {
    let wiki_root = make_wiki_root();
    let root = wiki_root.path();
    // Orphan with a usable title: renamed via the frontmatter fallback.
    fs::write(
        root.join("wiki/synthesis").join(format!("{ART_C}.md")),
        synthesis_page(ART_C, "Orphan Study Title"),
    )
    .unwrap();
    // Orphan without a title: skipped + counted.
    fs::write(
        root.join("wiki/synthesis").join(format!("{ART_UNMAPPED}.md")),
        format!(
            r#"---
type: synthesis
slug: {ART_UNMAPPED}
---

No title here.
"#
        ),
    )
    .unwrap();
    let staging = root.join("obsidian-export");
    let stats = write_vault(&root.join("wiki"), &staging, &simple_map()).unwrap();
    assert!(staging.join("synthesis/anonndorphan.md").exists());
    assert!(!staging.join(format!("synthesis/{ART_C}.md")).exists());
    assert!(!staging.join(format!("synthesis/{ART_UNMAPPED}.md")).exists());
    assert_eq!(stats.skipped_orphans, 1);
}

#[test]
fn write_vault_cross_type_collision_gets_suffix() {
    let wiki_root = make_wiki_root();
    let root = wiki_root.path();
    // A concept page already owns the stem ART_A's slug would take.
    fs::write(
        root.join("wiki/concepts/smith2023sugar.md"),
        "---\nid: smith2023sugar\nslug: smith2023sugar\ntitle: Colliding Concept\n---\n\nBody.\n",
    )
    .unwrap();
    let staging = root.join("obsidian-export");
    let stats = write_vault(&root.join("wiki"), &staging, &simple_map()).unwrap();
    assert!(staging.join("synthesis/smith2023sugarb.md").exists());
    assert!(staging.join("concepts/smith2023sugar.md").exists());
    assert_eq!(stats.renamed_synthesis, 2);
}

#[test]
fn write_vault_contains_no_uuids_anywhere() {
    let wiki_root = make_wiki_root();
    let root = wiki_root.path();
    // Extra orphan whose self-references must also be rewritten.
    fs::write(
        root.join("wiki/synthesis").join(format!("{ART_C}.md")),
        format!(
            r#"---
id: {ART_C}
title: Orphan Study Title
type: synthesis
slug: {ART_C}
source_articles: ["{ART_C}"]
---

Digest with [[{ART_C}]] self link and /raw/{ART_C}.md path.
"#
        ),
    )
    .unwrap();
    let staging = root.join("obsidian-export");
    write_vault(&root.join("wiki"), &staging, &simple_map()).unwrap();
    for file in walk_all(&staging) {
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        assert!(!contains_uuid(name), "UUID leaked into filename: {name}");
        let content = fs::read_to_string(&file).unwrap();
        assert!(!contains_uuid(&content), "UUID leaked into {}", file.display());
    }
}

#[test]
fn write_vault_leaves_user_doc_pages_untouched() {
    let wiki_root = make_wiki_root();
    let staging = wiki_root.path().join("obsidian-export");
    write_vault(&wiki_root.path().join("wiki"), &staging, &simple_map()).unwrap();
    let src = fs::read_to_string(staging.join("sources/user-notes.md")).unwrap();
    assert!(src.contains("id: user-notes"));
    assert!(src.contains("slug: user-notes"));
    assert!(src.contains("source_kind: user_text"));
    assert!(src.contains("[^art-user-notes]"));
    // No definition appended for user-doc keys (only mapped articles get one).
    assert!(!src.contains("[^art-user-notes]:"));
}

#[test]
fn zip_entries_match_staging_tree() {
    let wiki_root = make_wiki_root();
    let staging = wiki_root.path().join("obsidian-export");
    write_vault(&wiki_root.path().join("wiki"), &staging, &simple_map()).unwrap();

    let zip_path = wiki_root.path().join("vault.zip");
    zip_directory(&staging, &zip_path).unwrap();

    let file = fs::File::open(&zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).unwrap();
        names.push(entry.name().to_string());
    }
    names.sort();
    let mut expected: Vec<String> = walk_all(&staging)
        .iter()
        .map(|p| p.strip_prefix(&staging).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    expected.sort();
    assert_eq!(names, expected);

    // Content spot-check: the zip's Home.md matches the staged file.
    let mut home_in_zip = String::new();
    let idx = names.iter().position(|n| n == "Home.md").unwrap();
    archive.by_index(idx).unwrap().read_to_string(&mut home_in_zip).unwrap();
    assert_eq!(home_in_zip, fs::read_to_string(staging.join("Home.md")).unwrap());
}
