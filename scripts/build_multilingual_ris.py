#!/usr/bin/env python3
"""Build a combined RIS file from the multilingual-OA test assets and copy PDFs.

This script reads ``tests/assets/multilingual-oa/manifest.json``, converts each
per-article ``.ris.json`` file into a single RIS record (using the tag set that
Bango's importer understands — see ``src-tauri/src/export/ris_writer.rs``), and
writes the concatenated result to
``tests/assets/multilingual-oa/multilinguage.ris``.

It also copies every ``*.pdf`` under the asset subdirectories into
``~/Documents/Bango/fulltext/`` so Bango's batch importer
(``src-tauri/src/batch_import/full_text_phase.rs``) can attach them by DOI
match. The source PDFs are already named ``{clean_doi_filename(doi)}.pdf``,
which is exactly the stem the batch importer looks up.

Reproducible and idempotent: safe to re-run whenever the asset suite changes.
"""

import json
import os
import shutil
import sys

# Resolve paths relative to the repo root (parent of this script's dir).
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
WORKSPACE_DIR = os.path.dirname(SCRIPT_DIR)
ASSETS_DIR = os.path.join(WORKSPACE_DIR, "tests", "assets", "multilingual-oa")
MANIFEST_PATH = os.path.join(ASSETS_DIR, "manifest.json")
OUTPUT_RIS_PATH = os.path.join(ASSETS_DIR, "multilinguage.ris")

# Destination for full-text PDFs. Mirrors the Bango default storage root
# (``~/Documents/Bango``) + the ``fulltext/`` subdirectory used by the batch
# importer. Override via env var ``BANGO_FULLTEXT_DIR`` for non-default roots.
DEFAULT_FULLTEXT_DIR = os.path.join(os.path.expanduser("~"), "Documents", "Bango", "fulltext")
FULLTEXT_DIR = os.environ.get("BANGO_FULLTEXT_DIR", DEFAULT_FULLTEXT_DIR)


def record_to_ris(rec: dict) -> str:
    """Convert one parsed ``.ris.json`` record to an RIS text block.

    Mirrors the tag selection in ``src-tauri/src/export/ris_writer.rs``.
    ``None`` / empty fields are omitted so the RIS stays clean. Every record
    ends with the ``ER  -`` terminator.
    """
    lines = []

    lines.append(f"TY  - {rec.get('reference_type') or 'JOUR'}")

    title = (rec.get("title") or "").strip()
    if title:
        lines.append(f"TI  - {title}")

    abstract = (rec.get("abstract_text") or "").strip()
    if abstract:
        lines.append(f"AB  - {abstract}")

    for author in rec.get("authors") or []:
        author = author.strip()
        if author:
            lines.append(f"AU  - {author}")

    year = rec.get("publication_year")
    if year:
        lines.append(f"PY  - {year}")

    doi = rec.get("doi")
    if doi:
        lines.append(f"DO  - {doi}")

    journal = rec.get("journal")
    if journal:
        lines.append(f"T2  - {journal}")

    volume = rec.get("volume")
    if volume:
        lines.append(f"VL  - {volume}")

    issue = rec.get("issue")
    if issue:
        lines.append(f"IS  - {issue}")

    start_page = rec.get("start_page")
    if start_page:
        lines.append(f"SP  - {start_page}")

    end_page = rec.get("end_page")
    if end_page:
        lines.append(f"EP  - {end_page}")

    for kw in rec.get("keywords") or []:
        kw = kw.strip()
        if kw:
            lines.append(f"KW  - {kw}")

    url = rec.get("url")
    if url:
        lines.append(f"UR  - {url}")

    language = rec.get("language")
    if language:
        lines.append(f"LA  - {language}")

    publisher = rec.get("publisher")
    if publisher:
        lines.append(f"PB  - {publisher}")

    issn = rec.get("issn")
    if issn:
        lines.append(f"SN  - {issn}")

    notes = rec.get("notes")
    if notes:
        lines.append(f"N1  - {notes}")

    lines.append("ER  -")
    return "\n".join(lines) + "\n"


def main() -> int:
    if not os.path.exists(MANIFEST_PATH):
        print(f"ERROR: manifest not found at {MANIFEST_PATH}", file=sys.stderr)
        return 1

    with open(MANIFEST_PATH, "r", encoding="utf-8") as f:
        manifest = json.load(f)

    ris_blocks: list[str] = []
    pdf_sources: list[str] = []  # absolute paths to copy
    skipped: list[str] = []

    for entry in manifest:
        ris_json_rel = entry.get("local_ris_json")
        pdf_rel = entry.get("local_pdf")
        asset_id = entry.get("id", "<unknown>")

        if not ris_json_rel:
            skipped.append(f"{asset_id}: missing local_ris_json")
            continue

        ris_json_path = os.path.join(ASSETS_DIR, ris_json_rel)
        if not os.path.exists(ris_json_path):
            skipped.append(f"{asset_id}: missing {ris_json_path}")
            continue

        with open(ris_json_path, "r", encoding="utf-8") as f:
            rec = json.load(f)

        ris_blocks.append(record_to_ris(rec))

        if pdf_rel:
            pdf_path = os.path.join(ASSETS_DIR, pdf_rel)
            if os.path.exists(pdf_path):
                pdf_sources.append(pdf_path)
            else:
                skipped.append(f"{asset_id}: missing PDF {pdf_path}")

    # Write combined RIS (blank line between records).
    combined = "\n".join(ris_blocks) + "\n"
    with open(OUTPUT_RIS_PATH, "w", encoding="utf-8") as f:
        f.write(combined)
    print(f"Wrote {len(ris_blocks)} RIS records to {OUTPUT_RIS_PATH}")

    # Copy PDFs into the Bango fulltext directory.
    os.makedirs(FULLTEXT_DIR, exist_ok=True)
    copied = 0
    for src in pdf_sources:
        dest = os.path.join(FULLTEXT_DIR, os.path.basename(src))
        shutil.copy2(src, dest)
        copied += 1
    print(f"Copied {copied} PDFs to {FULLTEXT_DIR}")

    for msg in skipped:
        print(f"WARN: {msg}", file=sys.stderr)

    return 0


if __name__ == "__main__":
    sys.exit(main())