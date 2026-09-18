//! Obsidian vault export: pre-processes the wiki Markdown tree into a
//! UUID-free vault that opens directly in Obsidian.
//!
//! Pure module (no Tauri state, no DB access): the command layer in
//! `commands/wiki_cmd/obsidian_export.rs` loads the article rows and calls
//! [`build_article_slug_map`] + [`write_vault`]. Contract: article (synthesis)
//! pages are renamed to `{author}{year}{title-word}` slugs (the shared BibTeX
//! citation-key convention), every UUID-shaped reference in frontmatter and
//! bodies is remapped or dropped, `raw/`/`templates/`/`log.md` stay out, and a
//! generated `Home.md` + minimal `.obsidian/` config round out the vault.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::bibtex::writer::{citation_key_from_parts, dedupe_keys, first_author_surname};
use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};
use crate::wiki::storage;

/// Article row loaded by the command layer for the slug map.
#[derive(Debug, Clone)]
pub struct ArticleRow {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub authors: Vec<String>,
}

/// Resolved per-article slug + display alias used in wikilinks/footnotes.
#[derive(Debug, Clone)]
pub struct ObsidianArticleMeta {
    pub slug: String,
    pub alias: String,
}

/// Article UUID -> resolved slug/alias.
pub type SlugMap = HashMap<String, ObsidianArticleMeta>;

/// Stats about a written vault (surfaced in the success toast).
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStats {
    /// Markdown pages written.
    pub pages: usize,
    /// Synthesis pages renamed from UUID filenames to slugs.
    pub renamed_synthesis: usize,
    /// Footnote definitions appended (refs without a definition block).
    pub notes_appended: usize,
    /// Orphaned synthesis pages skipped (article deleted, no usable title).
    pub skipped_orphans: usize,
}

/// One exported page, collected for the generated `Home.md` index.
#[derive(Debug, Clone)]
pub struct VaultPage {
    /// Vault-relative path, e.g. `synthesis/smith2023sugar.md`.
    pub rel_path: String,
    /// Final filename stem (post-rename for synthesis pages).
    pub stem: String,
    /// Frontmatter title (falls back to the stem).
    pub title: String,
    /// Top-level folder (`concepts`, `synthesis`, ...).
    pub folder: String,
}

/// Build the article UUID -> slug/alias map. Slugs use the shared BibTeX
/// citation-key convention with letter-suffix dedup across the whole export;
/// the command layer orders rows by `sequence_id` so suffix assignment is
/// stable across runs.
#[must_use]
pub fn build_article_slug_map(rows: &[ArticleRow]) -> SlugMap {
    let keys: Vec<String> = rows.iter().map(article_slug_key).collect();
    let keys = dedupe_keys(&keys);
    rows.iter()
        .zip(keys)
        .map(|(row, slug)| {
            let alias = article_alias(&row.authors, row.year);
            (row.id.clone(), ObsidianArticleMeta { slug, alias })
        })
        .collect()
}

/// Slug key for one article row (same composition as `make_citation_key`).
fn article_slug_key(row: &ArticleRow) -> String {
    citation_key_from_parts(&row.authors, row.year, &row.title)
}

/// Display alias for an article: `{Surname} et al. {Year}` for multiple
/// authors, `{Surname} {Year}` for one, `Anon` / `n.d.` fallbacks. Matches
/// the `[[{id}|{author_label} {year}]]` convention of the in-app renderers.
#[must_use]
pub fn article_alias(authors: &[String], year: Option<i32>) -> String {
    let surname_raw = first_author_surname(authors);
    let surname = if surname_raw.trim().is_empty() { "Anon".to_string() } else { surname_raw };
    let year_str = year.map_or_else(|| "n.d.".to_string(), |y| y.to_string());
    let multiple =
        authors.iter().map(String::as_str).map(str::trim).filter(|a| !a.is_empty()).count() > 1;
    if multiple {
        format!("{surname} et al. {year_str}")
    } else {
        format!("{surname} {year_str}")
    }
}

/// Rewrite a page's frontmatter: map `id`/`slug`/`source_articles`/`links`
/// article UUIDs to slugs (unmapped UUIDs fall back to `fallback_stem` or are
/// dropped from lists), and strip the internal `source_file`/`source_hash`
/// provenance fields (they leak `raw/<uuid>.md` paths).
pub fn rewrite_frontmatter(fm: &mut Frontmatter, map: &SlugMap, fallback_stem: &str) {
    for key in ["id", "slug"] {
        if let Some(val) = fm.get(key).map(str::to_string) {
            let new_val = map_field_value(&val, map, fallback_stem);
            fm.set(key, &new_val);
        }
    }
    for key in ["source_articles", "links"] {
        let Some(val) = fm.get(key).map(str::to_string) else {
            continue;
        };
        let members = frontmatter::parse_list(&val);
        if members.is_empty() {
            continue;
        }
        let mapped: Vec<String> = members.iter().filter_map(|m| map_list_member(m, map)).collect();
        let quoted: Vec<String> = mapped.iter().map(|m| format!("\"{m}\"")).collect();
        fm.set(key, &format!("[{}]", quoted.join(", ")));
    }
    fm.fields.remove("source_file");
    fm.fields.remove("source_hash");
}

/// Map a scalar field value: mapped UUID -> slug, unmapped UUID -> fallback
/// stem, anything else unchanged.
fn map_field_value(val: &str, map: &SlugMap, fallback: &str) -> String {
    let v = val.trim();
    if let Some(meta) = map.get(v) {
        return meta.slug.clone();
    }
    if is_uuid(v) {
        return fallback.to_string();
    }
    val.to_string()
}

/// Map one `source_articles`/`links` list member. `None` drops the member
/// (empty entries + unmapped article UUIDs).
fn map_list_member(member: &str, map: &SlugMap) -> Option<String> {
    let m = member.trim();
    if m.is_empty() {
        return None;
    }
    if let Some(meta) = map.get(m) {
        return Some(meta.slug.clone());
    }
    if let Some(inner) = m.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
        let (target, alias) = match inner.split_once('|') {
            Some((t, a)) => (t.trim(), Some(a.trim())),
            None => (inner.trim(), None),
        };
        if let Some(meta) = map.get(target) {
            let alias = alias.unwrap_or(meta.alias.as_str());
            return Some(format!("[[{}|{}]]", meta.slug, alias));
        }
        if is_uuid(target) {
            return None;
        }
        return Some(m.to_string());
    }
    if is_uuid(m) {
        return None;
    }
    Some(m.to_string())
}

/// Rewrite a page body: wikilinks, footnote refs/definitions, and bare
/// `/raw/<uuid>.md` paths. Appends a definitions block for `[^art-*]` refs
/// that reference mapped articles but carry no definition.
#[must_use]
pub fn rewrite_body(body: &str, map: &SlugMap) -> String {
    rewrite_body_inner(body, map).0
}

/// Inner worker returning `(rewritten_body, appended_definition_count)` so
/// `write_vault` can populate `VaultStats::notes_appended`.
fn rewrite_body_inner(body: &str, map: &SlugMap) -> (String, usize) {
    let mut out_lines: Vec<String> = Vec::new();
    let mut defined: HashSet<String> = HashSet::new();
    let mut used_refs: Vec<(String, String)> = Vec::new();
    for line in body.lines() {
        if let Some((key, _target)) = parse_footnote_definition(line) {
            if let Some(meta) = map.get(&key) {
                defined.insert(meta.slug.clone());
                out_lines.push(format!("[^art-{}]: [[{}|{}]]", meta.slug, meta.slug, meta.alias));
                continue;
            }
            if is_uuid(&key) {
                // Unmapped article: drop the definition (its ref is dropped too).
                continue;
            }
            // User-doc or unknown key: keep verbatim.
            out_lines.push(line.to_string());
            continue;
        }
        out_lines.push(rewrite_inline(line, map, &mut used_refs));
    }
    /* Append definitions for mapped refs lacking one (author/framework
    renderers emit refs without definitions; in-app they resolve from the
    in-memory sources map, Obsidian needs an on-page definition). */
    let mut seen: HashSet<String> = HashSet::new();
    let missing: Vec<&(String, String)> = used_refs
        .iter()
        .filter(|(slug, _)| !defined.contains(slug) && seen.insert(slug.clone()))
        .collect();
    let appended = missing.len();
    if appended > 0 {
        out_lines.push(String::new());
        for (slug, alias) in missing {
            out_lines.push(format!("[^art-{slug}]: [[{slug}|{alias}]]"));
        }
    }
    let mut out = out_lines.join("\n");
    if body.ends_with('\n') {
        out.push('\n');
    }
    (out, appended)
}

/// Parse a `[^art-<key>]: <target>` footnote definition line.
fn parse_footnote_definition(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("[^art-")?;
    let end = rest.find(']')?;
    let key = &rest[..end];
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    let target = rest[end + 1..].strip_prefix(':')?.trim().to_string();
    Some((key.to_string(), target))
}

/// Rewrite all inline tokens on one line: `[[uuid]]` / `[[uuid|Alias]]`
/// wikilinks, `[^art-<uuid>]` refs, and bare `/raw/<uuid>.md` paths.
/// Mapped UUIDs become slugs; unmapped UUIDs are dropped (alias text kept for
/// wikilinks); everything else passes through unchanged.
fn rewrite_inline(line: &str, map: &SlugMap, used_refs: &mut Vec<(String, String)>) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        // `[[target]]` / `[[target|alias]]` wikilink.
        if chars[i] == '[' && chars_start_with(&chars, i, "[[") {
            if let Some(end) = find_seq(&chars, i + 2, "]]") {
                let inner: String = chars[i + 2..end].iter().collect();
                let (target, alias) = match inner.split_once('|') {
                    Some((t, a)) => (t.trim(), Some(a.trim())),
                    None => (inner.trim(), None),
                };
                if let Some(meta) = map.get(target) {
                    let alias = alias.unwrap_or(meta.alias.as_str());
                    out.push_str(&format!("[[{}|{}]]", meta.slug, alias));
                } else if is_uuid(target) {
                    // Unmapped UUID: drop the brackets, keep the alias text only.
                    if let Some(a) = alias {
                        out.push_str(a);
                    }
                } else {
                    let token: String = chars[i..end + 2].iter().collect();
                    out.push_str(&token);
                }
                i = end + 2;
                continue;
            }
        }
        // `[^art-<key>]` footnote ref.
        if chars[i] == '[' && chars_start_with(&chars, i, "[^art-") {
            if let Some(end) = find_seq(&chars, i + 1, "]") {
                let key: String = chars[i + "[^art-".len()..end].iter().collect();
                if let Some(meta) = map.get(&key) {
                    out.push_str(&format!("[^art-{}]", meta.slug));
                    used_refs.push((meta.slug.clone(), meta.alias.clone()));
                } else if is_uuid(&key) {
                    // Unmapped article: strip the marker, keep the text.
                } else {
                    let token: String = chars[i..=end].iter().collect();
                    out.push_str(&token);
                }
                i = end + 1;
                continue;
            }
        }
        // Bare `/raw/<uuid>.md` path.
        if chars_start_with(&chars, i, "/raw/") {
            let mut j = i + "/raw/".len();
            let mut id = String::new();
            while j < chars.len() && (chars[j].is_ascii_hexdigit() || chars[j] == '-') {
                id.push(chars[j]);
                j += 1;
            }
            if is_uuid(&id) && chars_start_with(&chars, j, ".md") {
                if let Some(meta) = map.get(&id) {
                    out.push_str(&format!("[[{}|{}]]", meta.slug, meta.alias));
                }
                // Unmapped: drop the path entirely.
                i = j + ".md".len();
                continue;
            }
            out.push_str("/raw/");
            i += "/raw/".len();
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Stage the vault: rewrite + copy every `wiki/**/*.md` page (skipping
/// `log.md` + `index.md`), rename synthesis files to slugs (mapped articles,
/// frontmatter fallback for orphans, cross-type collision-guarded), and write
/// `Home.md` + the minimal `.obsidian/` config. The staging dir is cleared on
/// each run (mirroring `wiki-export/`).
pub fn write_vault(
    wiki_dir: &Path,
    staging_dir: &Path,
    map: &SlugMap,
) -> Result<VaultStats, AppError> {
    let mut stats = VaultStats::default();

    let mut pages: Vec<PathBuf> = storage::walk_markdown(wiki_dir, true)
        .into_iter()
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name != "log.md" && name != "index.md"
        })
        .collect();
    // Deterministic output order + suffix assignment.
    pages.sort();

    let synthesis: Vec<&PathBuf> =
        pages.iter().filter(|p| is_synthesis_page(wiki_dir, p)).collect();
    let others: Vec<&PathBuf> = pages.iter().filter(|p| !is_synthesis_page(wiki_dir, p)).collect();

    /* Effective map: apply the cross-type collision guard so a renamed
    synthesis filename can never shadow an existing page (Obsidian resolves
    `[[wikilinks]]` by filename; ambiguity would break navigation). */
    let mut taken: BTreeSet<String> = others.iter().map(|p| path_stem(p)).collect();
    let mut effective: SlugMap = HashMap::new();
    let mut entries: Vec<(&String, &ObsidianArticleMeta)> = map.iter().collect();
    entries.sort_by(|a, b| a.1.slug.cmp(&b.1.slug).then_with(|| a.0.cmp(b.0)));
    for (id, meta) in entries {
        let slug = ensure_unique(&meta.slug, &mut taken);
        let meta = if slug == meta.slug {
            meta.clone()
        } else {
            ObsidianArticleMeta { slug, alias: meta.alias.clone() }
        };
        effective.insert(id.clone(), meta);
    }

    /* Orphaned synthesis pages (article hard-deleted, wiki not rebuilt):
    derive the slug from the page's own frontmatter so no UUID filename
    survives; skip + count pages with no usable title. */
    for path in &synthesis {
        let stem = path_stem(path);
        if effective.contains_key(&stem) {
            continue;
        }
        let Ok((fm, _)) = frontmatter::read_file(path) else {
            stats.skipped_orphans += 1;
            continue;
        };
        let title = fm.get("title").unwrap_or("").trim().to_string();
        if title.is_empty() || is_uuid(&title) {
            stats.skipped_orphans += 1;
            continue;
        }
        let year = fm.get("year").and_then(|y| y.trim().parse::<i32>().ok());
        let slug = ensure_unique(&citation_key_from_parts(&[], year, &title), &mut taken);
        effective.insert(stem, ObsidianArticleMeta { slug, alias: article_alias(&[], year) });
    }

    // Fresh staging dir each run.
    if staging_dir.exists() {
        std::fs::remove_dir_all(staging_dir)?;
    }
    std::fs::create_dir_all(staging_dir)?;

    let mut vault_pages: Vec<VaultPage> = Vec::new();
    for path in &pages {
        let rel = path.strip_prefix(wiki_dir).unwrap_or(path.as_path());
        let folder = rel
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_default();
        let stem = path_stem(path);
        let (dest_rel, final_stem): (PathBuf, String);
        if is_synthesis_page(wiki_dir, path) {
            let Some(meta) = effective.get(&stem) else {
                continue; // skipped orphan, counted above
            };
            dest_rel = PathBuf::from("synthesis").join(format!("{}.md", meta.slug));
            final_stem = meta.slug.clone();
            stats.renamed_synthesis += 1;
        } else {
            dest_rel = rel.to_path_buf();
            final_stem = stem;
        }
        let (mut fm, body) = frontmatter::read_file(path)?;
        rewrite_frontmatter(&mut fm, &effective, &final_stem);
        let (new_body, appended) = rewrite_body_inner(&body, &effective);
        stats.notes_appended += appended;
        let dest = staging_dir.join(&dest_rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, fm.to_markdown(&new_body))?;
        stats.pages += 1;
        let title = fm
            .get("title")
            .map(str::to_string)
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| final_stem.clone());
        vault_pages.push(VaultPage {
            rel_path: dest_rel.to_string_lossy().to_string(),
            stem: final_stem,
            title,
            folder,
        });
    }

    std::fs::write(staging_dir.join("Home.md"), build_home_md(&vault_pages))?;
    write_obsidian_config(staging_dir)?;

    Ok(stats)
}

/// Generate the `Home.md` index: one section per page type (Concepts /
/// Authors / Methods / Frameworks / Synthesis / Sources), empty folders
/// omitted.
#[must_use]
pub fn build_home_md(pages: &[VaultPage]) -> String {
    const SECTIONS: [(&str, &str); 6] = [
        ("concepts", "Concepts"),
        ("authors", "Authors"),
        ("methods", "Methods"),
        ("frameworks", "Frameworks"),
        ("synthesis", "Synthesis"),
        ("sources", "Sources"),
    ];
    let mut out = String::from("# Wiki Home\n\nGenerated by Bango.\n");
    for (folder, heading) in SECTIONS {
        let entries: Vec<&VaultPage> = pages.iter().filter(|p| p.folder == folder).collect();
        if entries.is_empty() {
            continue;
        }
        out.push_str("\n## ");
        out.push_str(heading);
        out.push_str("\n\n");
        for page in entries {
            out.push_str(&format!("- [[{}|{}]]\n", page.stem, sanitize_alias(&page.title)));
        }
    }
    out
}

/// Strip characters that would break a wikilink alias.
fn sanitize_alias(text: &str) -> String {
    text.replace('|', "-").replace(['[', ']'], "")
}

/// Write the minimal `.obsidian/` config: `app.json` (empty, Obsidian fills
/// defaults) + `graph.json` color groups keyed on `path:` folder prefixes
/// using the app's page-type colors (`wiki-graph-panel.vue`).
fn write_obsidian_config(staging_dir: &Path) -> Result<(), AppError> {
    let obsidian_dir = staging_dir.join(".obsidian");
    std::fs::create_dir_all(&obsidian_dir)?;
    std::fs::write(obsidian_dir.join("app.json"), "{}\n")?;
    std::fs::write(obsidian_dir.join("graph.json"), build_graph_json())?;
    Ok(())
}

/// Build the Obsidian `graph.json` body with one color group per vault
/// folder, mirroring the app page-type colors (`wiki-graph-panel.vue`).
/// Serialized via `serde_json` so the escaped-quote `path:` queries stay
/// correct by construction.
fn build_graph_json() -> String {
    const GROUPS: [(&str, u32); 6] = [
        ("concepts", 0x6366f1),   // indigo
        ("authors", 0x22c55e),    // green
        ("methods", 0xf97316),    // orange
        ("frameworks", 0x14b8a6), // teal
        ("synthesis", 0xa855f7),  // purple
        ("sources", 0x64748b),    // slate
    ];
    let color_groups: Vec<serde_json::Value> = GROUPS
        .iter()
        .map(|(folder, rgb)| {
            serde_json::json!({
                "query": format!("path:\"{folder}/\""),
                "color": { "a": 1, "rgb": rgb }
            })
        })
        .collect();
    let graph = serde_json::json!({
        "collapse-filter": true,
        "search": "",
        "showTags": true,
        "showAttachments": false,
        "hideUnresolved": false,
        "showOrphans": true,
        "color-groups": color_groups,
    });
    serde_json::to_string_pretty(&graph).unwrap_or_default()
}

/// True when `s` is shaped like a UUID (8-4-4-4-12 hex). Detects article ids
/// that must never survive into the vault.
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Return `candidate` unless already taken; otherwise append the BibTeX
/// letter-suffix convention (`b`, `c`, ..., capped at `z`). The result is
/// inserted into `taken`.
fn ensure_unique(candidate: &str, taken: &mut BTreeSet<String>) -> String {
    if taken.insert(candidate.to_string()) {
        return candidate.to_string();
    }
    for offset in 0..25u8 {
        let suffixed = format!("{candidate}{}", char::from(b'b' + offset));
        if taken.insert(suffixed.clone()) {
            return suffixed;
        }
    }
    let capped = format!("{candidate}z");
    taken.insert(capped.clone());
    capped
}

/// Whether a page lives under `wiki/synthesis/`.
fn is_synthesis_page(wiki_dir: &Path, path: &Path) -> bool {
    path.strip_prefix(wiki_dir)
        .ok()
        .and_then(|rel| rel.components().next())
        .is_some_and(|c| c.as_os_str() == "synthesis")
}

/// Filename stem as a String (empty when unavailable).
fn path_stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string()
}

/// Whether `chars[at..]` starts with `prefix`.
fn chars_start_with(chars: &[char], at: usize, prefix: &str) -> bool {
    let prefix: Vec<char> = prefix.chars().collect();
    at + prefix.len() <= chars.len() && chars[at..at + prefix.len()] == prefix[..]
}

/// Find the first index >= `from` where `seq` starts in `chars`.
fn find_seq(chars: &[char], from: usize, seq: &str) -> Option<usize> {
    let seq: Vec<char> = seq.chars().collect();
    if seq.is_empty() {
        return Some(from);
    }
    let mut i = from;
    while i + seq.len() <= chars.len() {
        if chars[i..i + seq.len()] == seq[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}
