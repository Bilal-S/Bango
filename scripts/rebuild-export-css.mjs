#!/usr/bin/env node
/**
 * Rebuild the wiki-export style.css from source files.
 * Used for testing the export CSS without re-running the full Tauri app.
 *
 * Usage: node scripts/rebuild-export-css.mjs
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '..');

const markdownCss = readFileSync(join(projectRoot, 'src/styles/markdown.css'), 'utf-8');

const WIKI_LINK_STYLES = `
.wikilink {
  color: rgb(79 70 229);
  text-decoration: underline;
  cursor: pointer;
  text-decoration-style: dotted;
}
.wikilink:hover { text-decoration-style: solid; }
.wikilink--synthesis {
  display: inline-block;
  background: rgb(168 85 247 / 0.12);
  color: rgb(126 34 206);
  border: 1px solid rgb(168 85 247 / 0.3);
  padding: 0.0625rem 0.375rem;
  border-radius: 0.25rem;
  font-size: 0.8em;
  font-weight: 500;
  text-decoration: none;
  cursor: pointer;
}
.wikilink--synthesis:hover { background: rgb(168 85 247 / 0.2); }
.art-ref {
  display: inline;
  color: rgb(21 128 61);
  background: rgb(240 253 244);
  border: 1px solid rgb(220 252 231);
  padding: 0 0.3rem;
  border-radius: 0.25rem;
  font-size: 0.75rem;
  cursor: pointer;
  text-decoration: none;
  font-weight: 500;
}
.art-ref:hover { background: rgb(220 252 231); }
.art-ref--missing {
  color: rgb(148 163 184);
  background: rgb(241 245 249);
  border-color: rgb(226 232 240);
}
.section-badge {
  display: inline-block;
  margin-left: 0.25rem;
  padding: 0.0625rem 0.3125rem;
  font-size: 0.7em;
  font-weight: 500;
  color: rgb(100 116 139);
  background: rgb(241 245 249);
  border: 1px solid rgb(226 232 240);
  border-radius: 0.25rem;
  vertical-align: baseline;
}
.ref-missing {
  color: rgb(148 163 184);
  font-style: italic;
  font-size: 0.85em;
}
`;

const PAGE_CHROME_CSS = `
/* === Page chrome (layout, navigation, footer) === */
:root {
  color-scheme: light;
  --color-bg: #ffffff;
  --color-surface: #f8f9fa;
  --color-text: #1b1b24;
  --color-text-muted: #64748b;
  --color-primary: #3525cd;
  --color-border: #e2e8f0;
  --color-link: #3525cd;
  --color-link-hover: #4f46e5;
  --font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  --max-width: 800px;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--color-bg);
  color: var(--color-text);
  font-family: var(--font-family);
  line-height: 1.6;
}
nav.breadcrumb {
  padding: 1rem 1.5rem;
  border-bottom: 1px solid var(--color-border);
  font-size: 0.85rem;
  color: var(--color-text-muted);
}
nav.breadcrumb a {
  color: var(--color-primary);
  text-decoration: none;
}
nav.breadcrumb a:hover { text-decoration: underline; }
article.wiki-page, article.article-stub, .index-container {
  max-width: var(--max-width);
  margin: 0 auto;
  padding: 2rem 1.5rem;
}
article.wiki-page h1, article.article-stub h1 {
  margin-top: 0;
  color: var(--color-text);
}
.article-meta { margin: 1.5rem 0; }
.meta-row { margin: 0.4rem 0; font-size: 0.95rem; }
.meta-label { font-weight: 600; color: var(--color-text-muted); margin-right: 0.5rem; }
.abstract-text { margin-top: 1rem; line-height: 1.7; }
.index-header {
  max-width: var(--max-width);
  margin: 0 auto;
  padding: 2rem 1.5rem 1rem;
}
.index-header h1 {
  margin: 0 0 0.5rem;
  font-size: 1.8rem;
  color: var(--color-text);
}
.index-header .subtitle {
  color: var(--color-text-muted);
  margin: 0 0 1.5rem;
}
.search-box {
  width: 100%;
  padding: 0.6rem 0.9rem;
  border: 1px solid var(--color-border);
  border-radius: 0.5rem;
  font-size: 0.95rem;
  background: var(--color-bg);
  color: var(--color-text);
}
.search-box:focus {
  outline: none;
  border-color: var(--color-primary);
}
.page-section {
  max-width: var(--max-width);
  margin: 0 auto;
  padding: 0 1.5rem 1.5rem;
}
.page-section h2 {
  border-bottom: 1px solid var(--color-border);
  padding-bottom: 0.4rem;
  color: var(--color-text-muted);
  text-transform: uppercase;
  font-size: 0.8rem;
  letter-spacing: 0.05em;
}
.card-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 1rem;
  margin-top: 1rem;
}
.page-card {
  display: block;
  padding: 1rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 0.5rem;
  text-decoration: none;
  color: var(--color-text);
  transition: border-color 0.15s;
}
.page-card:hover { border-color: var(--color-primary); }
.page-card h3 {
  margin: 0 0 0.4rem;
  font-size: 1rem;
  color: var(--color-primary);
}
.page-card p {
  margin: 0;
  font-size: 0.85rem;
  color: var(--color-text-muted);
}
.site-footer {
  max-width: var(--max-width);
  margin: 2rem auto;
  padding: 1rem 1.5rem;
  border-top: 1px solid var(--color-border);
  text-align: center;
  color: var(--color-text-muted);
  font-size: 0.8rem;
}
.site-footer a { color: var(--color-primary); text-decoration: none; }
.site-footer a:hover { text-decoration: underline; }
`;

const combined = [
  '/* === markdown.css (the real in-app wiki typography) === */',
  markdownCss,
  '/* === Wiki link styles (from wiki-page-viewer.vue) === */',
  WIKI_LINK_STYLES,
  PAGE_CHROME_CSS,
].join('\n');

const exportDir = join(
  process.env.HOME || '/home/user',
  'Documents/Bango/wiki-root/wiki-export'
);
const stylePath = join(exportDir, 'style.css');

writeFileSync(stylePath, combined, 'utf-8');
console.log(`Written ${Buffer.byteLength(combined)} bytes to ${stylePath}`);