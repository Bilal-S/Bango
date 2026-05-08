pub const VERSION: i32 = 3;

pub const UP_SQL: &str = r#"
-- Rename 'imported' status to 'duplicate' in existing data.
-- SQLite doesn't support ALTER TABLE ... ALTER CHECK constraints,
-- so we recreate the articles table with the updated constraint.

CREATE TABLE articles_new (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'duplicate' CHECK(status IN ('duplicate', 'working', 'included', 'rejected')),
    screening_error INTEGER NOT NULL DEFAULT 0,
    title TEXT NOT NULL,
    abstract_text TEXT NOT NULL,
    authors TEXT NOT NULL,
    publication_year INTEGER,
    doi TEXT,
    journal TEXT,
    volume TEXT,
    issue TEXT,
    start_page TEXT,
    end_page TEXT,
    keywords TEXT,
    url TEXT,
    language TEXT,
    publisher TEXT,
    publisher_city TEXT,
    publisher_address TEXT,
    issn TEXT,
    reference_type TEXT,
    date TEXT,
    author_address TEXT,
    accession_number TEXT,
    custom_field3 TEXT,
    journal_abbreviation TEXT,
    journal_iso_abbreviation TEXT,
    notes TEXT,
    web_of_science_db TEXT,
    user_notes TEXT,
    ris_extras TEXT,
    duplicate_of TEXT,
    ai_decision TEXT CHECK(ai_decision IS NULL OR ai_decision IN ('include', 'exclude')),
    ai_reasoning TEXT,
    ai_confidence REAL,
    matched_inclusion_criteria TEXT,
    matched_exclusion_criteria TEXT,
    manual_override INTEGER NOT NULL DEFAULT 0,
    import_source TEXT,
    imported_at TEXT NOT NULL DEFAULT (datetime('now')),
    screened_at TEXT,
    FOREIGN KEY (duplicate_of) REFERENCES articles_new(id)
);

INSERT INTO articles_new
    SELECT id,
           CASE WHEN status = 'imported' THEN 'duplicate' ELSE status END,
           screening_error, title, abstract_text, authors, publication_year, doi,
           journal, volume, issue, start_page, end_page, keywords, url,
           language, publisher, publisher_city, publisher_address, issn,
           reference_type, date, author_address, accession_number,
           custom_field3, journal_abbreviation, journal_iso_abbreviation,
           notes, web_of_science_db, user_notes, ris_extras, duplicate_of,
           ai_decision, ai_reasoning, ai_confidence,
           matched_inclusion_criteria, matched_exclusion_criteria,
           manual_override, import_source, imported_at, screened_at
    FROM articles;

DROP TABLE articles;
ALTER TABLE articles_new RENAME TO articles;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_articles_status ON articles(status);
CREATE INDEX IF NOT EXISTS idx_articles_duplicate_of ON articles(duplicate_of);
CREATE INDEX IF NOT EXISTS idx_articles_screened_at ON articles(screened_at);
"#;