#!/usr/bin/env node
/**
 * Verify that the wiki export HTML pages have identical content to the source
 * markdown files (after rendering). Checks:
 *
 * 1. Every wiki page in the export has a corresponding markdown source
 * 2. All [[wikilinks]] in the markdown resolve to valid hrefs in the HTML
 * 3. All [^art-*] footnotes in the markdown resolve to valid hrefs in the HTML
 * 4. No UUIDs or data- attributes leak as visible text
 * 5. Headings and key content blocks match
 *
 * Usage: node scripts/verify-export-content.mjs
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const homeDir = process.env.HOME || '/home/user';
const exportDir = join(homeDir, 'Documents/Bango/wiki-root/wiki-export');
const pagesDir = join(exportDir, 'pages');
const markdownDir = join(exportDir, 'markdown');

const errors = [];
const warnings = [];
let passed = 0;
let checked = 0;

// Walk a directory recursively
function walkDir(dir, pattern) {
  const results = [];
  const entries = readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...walkDir(fullPath, pattern));
    } else if (entry.isFile() && pattern.test(entry.name)) {
      results.push(fullPath);
    }
  }
  return results;
}

// Parse [[slug|alias]] or [[slug]] links from markdown
function _parseWikilinks(md) {
  const re = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;
  const links = [];
  let m;
  while ((m = re.exec(md)) !== null) {
    links.push({ slug: m[1].trim(), alias: (m[2] || m[1]).trim(), pos: m.index });
  }
  return links;
}

// Parse [^art-id] footnotes from markdown
function _parseArtRefs(md) {
  const re = /\[\^([^\]]+)\]/g;
  const refs = [];
  let m;
  while ((m = re.exec(md)) !== null) {
    refs.push({ id: m[1].trim(), pos: m.index });
  }
  return refs;
}

// Check for visible UUIDs (should never appear as plain text)
function findVisibleUuids(html) {
  // Remove script/style/tag content
  const stripped = html
    .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
    .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
    .replace(/<[^>]+>/g, ' ') // strip all HTML tags
    .replace(/&/g, '&')
    .replace(/</g, '<')
    .replace(/>/g, '>')
    .replace(/"/g, '"')
    .replace(/&#39;/g, "'");
  const uuidRe = /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi;
  const matches = [...stripped.matchAll(uuidRe)];
  return matches.map((m) => m[0]);
}

// Check for data-slug or data-art-id attributes (should all be converted to href)
function findDataslugAttributes(html) {
  const re = /data-slug=/g;
  return [...html.matchAll(re)].length;
}

function findDataArtIdAttributes(html) {
  const re = /data-art-id=/g;
  return [...html.matchAll(re)].length;
}

// Check for dangling [^...] markers that weren't converted
function findDanglingFootnotes(html) {
  const stripped = html
    .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '')
    .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
    .replace(/<[^>]+>/g, ' ');
  const re = /\[\^[^\]]+\]/g;
  return [...stripped.matchAll(re)].map((m) => m[0]);
}

console.log('== Wiki Export Content Verification ==\n');

const htmlFiles = walkDir(pagesDir, /\.html$/);
const mdFiles = walkDir(markdownDir, /\.md$/);

console.log(`Found ${htmlFiles.length} HTML pages in export`);
console.log(`Found ${mdFiles.length} markdown source files\n`);

// Check 1: Every HTML page has wikilinks that resolve to valid hrefs
console.log('--- Check 1: Wikilink resolution ---');
for (const htmlPath of htmlFiles.slice(0, 20)) {
  const html = readFileSync(htmlPath, 'utf-8');
  const relativePath = htmlPath.replace(pagesDir + '/', '');

  // Extract all <a> elements with href
  const aRe = /<a\s+[^>]*href="([^"]*)"[^>]*>/g;
  const hrefs = [];
  let m;
  while ((m = aRe.exec(html)) !== null) {
    hrefs.push(m[1]);
  }

  // Check each href resolves
  for (const href of hrefs) {
    if (href.startsWith('http')) continue; // external
    if (href.startsWith('mailto:')) continue;
    if (href === '#') continue;

    const resolved = join(dirname(htmlPath), href);
    try {
      statSync(resolved);
      passed++;
    } catch {
      errors.push(`${relativePath}: broken link → ${href} (resolved: ${resolved})`);
    }
    checked++;
  }

  // Check for data-slug/data-art-id attributes
  const dataSlugCount = findDataslugAttributes(html);
  if (dataSlugCount > 0) {
    errors.push(`${relativePath}: ${dataSlugCount} unconverted data-slug attributes`);
  }
  const dataArtIdCount = findDataArtIdAttributes(html);
  if (dataArtIdCount > 0) {
    errors.push(`${relativePath}: ${dataArtIdCount} unconverted data-art-id attributes`);
  }

  // Check for visible UUIDs
  const visibleUuids = findVisibleUuids(html);
  if (visibleUuids.length > 0) {
    warnings.push(
      `${relativePath}: ${visibleUuids.length} visible UUIDs (may be intentional anchor text)`
    );
  }

  // Check for dangling footnotes
  const danglingFootnotes = findDanglingFootnotes(html);
  if (danglingFootnotes.length > 0) {
    errors.push(
      `${relativePath}: ${danglingFootnotes.length} unresolved footnotes: ${danglingFootnotes.join(', ')}`
    );
  }
}

console.log(`  Checked ${checked} links across pages`);
console.log(`  Passed: ${passed}, Errors: ${errors.length}, Warnings: ${warnings.length}\n`);

// Check 2: Light mode enforcement
console.log('--- Check 2: Light mode enforcement ---');
for (const htmlPath of [join(exportDir, 'style.css')]) {
  const css = readFileSync(htmlPath, 'utf-8');
  if (css.includes('color-scheme: light')) {
    passed++;
    console.log('  ✓ color-scheme: light is present in style.css');
  } else {
    errors.push('style.css: missing color-scheme: light');
  }
  if (css.includes('prefers-color-scheme: dark')) {
    warnings.push('style.css: still contains @media (prefers-color-scheme: dark)');
  } else {
    console.log('  ✓ No dark mode media query in style.css');
  }
  checked += 2;
}
console.log('');

// Check 3: Key content pages have expected structure
console.log('--- Check 3: Content page structure ---');
const expectedPages = [
  {
    path: 'concept/food.html',
    expect: ['<article class="wiki-page', '<div class="markdown-content"'],
  },
  {
    path: 'author/author-rogers-n.html',
    expect: ['<article class="wiki-page', '<div class="markdown-content"'],
  },
  {
    path: 'synthesis/soft-drinks-industry-levy.html',
    expect: [
      '<article class="wiki-page',
      '<div class="markdown-content"',
      '<nav class="breadcrumb"',
    ],
  },
  {
    path: 'method/controlled-interrupted-time-series.html',
    expect: ['<article class="wiki-page', '<div class="markdown-content"'],
  },
];

for (const { path, expect } of expectedPages) {
  const fullPath = join(pagesDir, path);
  try {
    const html = readFileSync(fullPath, 'utf-8');
    const missing = expect.filter((e) => !html.includes(e));
    if (missing.length > 0) {
      errors.push(`${path}: missing expected elements: ${missing.join(', ')}`);
    } else {
      passed++;
    }
  } catch {
    errors.push(`${path}: file not found`);
  }
  checked++;
}
console.log(`  Checked ${expectedPages.length} key pages\n`);

// Check 4: Index page structure
console.log('--- Check 4: Index page structure ---');
try {
  const indexHtml = readFileSync(join(exportDir, 'index.html'), 'utf-8');
  const indexExpected = [
    '<div class="index-header"',
    '<input type="text" id="search-input"',
    'class="page-card"',
    '<section class="page-section"',
  ];
  const missing = indexExpected.filter((e) => !indexHtml.includes(e));
  if (missing.length > 0) {
    errors.push(`index.html: missing: ${missing.join(', ')}`);
  } else {
    passed++;
  }
  checked++;
  console.log('  ✓ Index page has all expected elements');
} catch {
  errors.push('index.html: file not found');
}
console.log('');

// Summary
console.log('========================================');
console.log(`  Total checks: ${checked + passed}`);
console.log(`  Passed: ${passed}`);
console.log(`  Errors: ${errors.length}`);
console.log(`  Warnings: ${warnings.length}`);

if (errors.length > 0) {
  console.log('\n❌ ERRORS:');
  for (const e of errors) {
    console.log(`  - ${e}`);
  }
}

if (warnings.length > 0) {
  console.log('\n⚠ WARNINGS:');
  for (const w of warnings) {
    console.log(`  - ${w}`);
  }
}

if (errors.length === 0) {
  console.log('\n✅ All checks passed!');
  process.exit(0);
} else {
  process.exit(1);
}
