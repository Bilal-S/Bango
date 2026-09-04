//! DOI canonicalization (VERSION 9).
//!
//! Heals legacy mixed-case, prefixed, and whitespace-wrapped DOIs, merges
//! case-variant duplicate `reference_papers`, and rebuilds the DOI unique
//! index case-insensitively on `LOWER(doi)`. See `.worktrees/doifix.md`.
//!
//! ## Why statement order matters
//!
//! The old BINARY `uq_ref_papers_doi` index is dropped FIRST: the healing
//! UPDATEs would violate it wherever case variants exist, aborting the
//! transaction and app startup for exactly the databases this migration
//! must heal. Healing runs before the merge so prefix variants collapse
//! into the same duplicate group, and the new `LOWER(doi)` index is created
//! LAST so it cannot fail on pre-existing variants.
//!
//! ## Merge semantics (pure SQL: the runner executes `up_sql` via
//! `execute_batch`, no Rust loops)
//!
//! A temp table maps every case-variant duplicate to its group survivor via
//! `FIRST_VALUE` over `PARTITION BY doi` (exact groups after healing),
//! ordered by match-state rank (`matched`/`imported` > `unmatched` > other)
//! then lowest `rowid`. Rank-based survivor selection preserves match state
//! without a coalescing pass: a matched dupe becomes the survivor. Links are
//! remapped with `UPDATE OR IGNORE` (the `(parent, paper, type)` unique
//! collisions are absorbed), leftover links on dupes and the dupe rows are
//! deleted, and `citation_count`/`reference_count` are recomputed from the
//! surviving links (they are link-derived and maintained incrementally in
//! Rust, so the migration must recount after remapping).
//!
//! ## Idempotency
//!
//! Every statement is idempotent on canonical data (the leading
//! `DROP TABLE IF EXISTS` guards the temp table). No
//! `ALTER TABLE ADD COLUMN`, so no `heal_partial_migrations` marker probe.

pub const VERSION: i32 = 9;

pub const UP_SQL: &str = "\
-- (1) The old BINARY unique index must go before healing.
DROP INDEX IF EXISTS uq_ref_papers_doi;

-- (2) Heal articles.doi to canonical form. Trim first so the prefix
-- strips also catch values that arrived with surrounding whitespace.
UPDATE articles SET doi = TRIM(doi) WHERE doi IS NOT NULL;
UPDATE articles SET doi = SUBSTR(doi, 17) WHERE doi LIKE 'https://doi.org/%';
UPDATE articles SET doi = SUBSTR(doi, 16) WHERE doi LIKE 'http://doi.org/%';
UPDATE articles SET doi = SUBSTR(doi, 20) WHERE doi LIKE 'https://dx.doi.org/%';
UPDATE articles SET doi = SUBSTR(doi, 19) WHERE doi LIKE 'http://dx.doi.org/%';
UPDATE articles SET doi = SUBSTR(doi, 5)
 WHERE doi LIKE 'doi:10.%' OR doi LIKE 'doi: 10.%';
UPDATE articles SET doi = NULL
 WHERE doi IS NOT NULL AND UPPER(TRIM(doi)) IN ('NA', 'N/A', 'NULL', 'NONE', '-');
UPDATE articles SET doi = NULLIF(TRIM(LOWER(doi)), '') WHERE doi IS NOT NULL;

-- ... and reference_papers.doi, identical block.
UPDATE reference_papers SET doi = TRIM(doi) WHERE doi IS NOT NULL;
UPDATE reference_papers SET doi = SUBSTR(doi, 17) WHERE doi LIKE 'https://doi.org/%';
UPDATE reference_papers SET doi = SUBSTR(doi, 16) WHERE doi LIKE 'http://doi.org/%';
UPDATE reference_papers SET doi = SUBSTR(doi, 20) WHERE doi LIKE 'https://dx.doi.org/%';
UPDATE reference_papers SET doi = SUBSTR(doi, 19) WHERE doi LIKE 'http://dx.doi.org/%';
UPDATE reference_papers SET doi = SUBSTR(doi, 5)
 WHERE doi LIKE 'doi:10.%' OR doi LIKE 'doi: 10.%';
UPDATE reference_papers SET doi = NULL
 WHERE doi IS NOT NULL AND UPPER(TRIM(doi)) IN ('NA', 'N/A', 'NULL', 'NONE', '-');
UPDATE reference_papers SET doi = NULLIF(TRIM(LOWER(doi)), '') WHERE doi IS NOT NULL;

-- (3) Merge duplicate papers. After healing, groups are exact-DOI groups.
DROP TABLE IF EXISTS doi_paper_merge;
CREATE TEMP TABLE doi_paper_merge AS
SELECT p.id AS dupe_id,
       FIRST_VALUE(p.id) OVER (
           PARTITION BY p.doi
           ORDER BY CASE p.match_status
                        WHEN 'matched'   THEN 0
                        WHEN 'imported'  THEN 0
                        WHEN 'unmatched' THEN 1
                        ELSE 2 END,
                    p.rowid
           ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
       ) AS survivor_id
FROM reference_papers p
WHERE p.doi IS NOT NULL;
DELETE FROM doi_paper_merge WHERE dupe_id = survivor_id;

-- Remap links to survivors. OR IGNORE absorbs (parent, paper, type)
-- collisions with links that already point at the survivor.
UPDATE OR IGNORE article_reference_links
SET reference_paper_id = (SELECT m.survivor_id FROM doi_paper_merge m
                           WHERE m.dupe_id = article_reference_links.reference_paper_id)
WHERE reference_paper_id IN (SELECT dupe_id FROM doi_paper_merge);

-- Collision-absorbed links remain on dupes; drop them, then the dupes.
DELETE FROM article_reference_links
WHERE reference_paper_id IN (SELECT dupe_id FROM doi_paper_merge);
DELETE FROM reference_papers
WHERE id IN (SELECT dupe_id FROM doi_paper_merge);
DROP TABLE doi_paper_merge;

-- (4) Recount link-derived counters (type 0 = citation, 1 = reference).
UPDATE reference_papers
SET citation_count = (SELECT COUNT(*) FROM article_reference_links l
                       WHERE l.reference_paper_id = reference_papers.id
                         AND l.type = 0),
    reference_count = (SELECT COUNT(*) FROM article_reference_links l
                        WHERE l.reference_paper_id = reference_papers.id
                         AND l.type = 1);

-- (5) Case-insensitive uniqueness, created last.
CREATE UNIQUE INDEX uq_ref_papers_doi
    ON reference_papers(LOWER(doi)) WHERE doi IS NOT NULL;
";
