pub const VERSION: i32 = 1;

pub const UP_SQL: &str = r#"
-- ── Core tables ──────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS research_aims (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    text TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS criteria (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    priority TEXT NOT NULL CHECK(priority IN ('critical', 'high', 'standard', 'low', 'optional')),
    text TEXT NOT NULL,
    type TEXT NOT NULL CHECK(type IN ('inclusion', 'exclusion'))
);

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    color TEXT,
    name TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL CHECK(source IN ('ai_suggested', 'user_created', 'ris_keyword'))
);

CREATE TABLE IF NOT EXISTS labels (
    id TEXT PRIMARY KEY,
    color TEXT,
    name TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL CHECK(source IN ('ai_generated', 'user_created'))
);

-- ── Articles ─────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS articles (
    -- Primary key
    id TEXT PRIMARY KEY,
    -- Columns (alphabetical)
    abstract_text TEXT NOT NULL,
    accession_number TEXT,
    actual_tokens INTEGER,
    ai_confidence REAL,
    ai_decision TEXT CHECK(ai_decision IS NULL OR ai_decision IN ('include', 'exclude')),
    ai_reasoning TEXT,
    affiliation TEXT,
    author_address TEXT,
    authors TEXT NOT NULL,
    changed_at TEXT NOT NULL DEFAULT '',
    custom_field3 TEXT,
    data_length INTEGER,
    date TEXT,
    doi TEXT,
    duplicate_of TEXT,
    eissn TEXT,
    end_page TEXT,
    full_text TEXT,
    full_text_ai_summary TEXT,
    full_text_file_name TEXT,
    has_citation_details INTEGER NOT NULL DEFAULT 0,
    has_full_text INTEGER NOT NULL DEFAULT 0,
    has_reference_details INTEGER NOT NULL DEFAULT 0,
    import_source TEXT,
    imported_at TEXT NOT NULL DEFAULT (datetime('now')),
    issn TEXT,
    issue TEXT,
    journal TEXT,
    journal_abbreviation TEXT,
    journal_index_id TEXT REFERENCES journal_index(id),
    journal_iso_abbreviation TEXT,
    keywords TEXT,
    language TEXT,
    manual_override INTEGER NOT NULL DEFAULT 0,
    matched_exclusion_criteria TEXT,
    matched_inclusion_criteria TEXT,
    notes TEXT,
    num_cited INTEGER,
    num_references INTEGER,
    publication_year INTEGER,
    publisher TEXT,
    publisher_address TEXT,
    publisher_city TEXT,
    reference_type TEXT,
    ris_extras TEXT,
    screened_at TEXT,
    screening_error INTEGER NOT NULL DEFAULT 0,
    sequence_id INTEGER NOT NULL DEFAULT 0,
    start_page TEXT,
    status TEXT NOT NULL DEFAULT 'duplicate' CHECK(status IN ('duplicate', 'working', 'included', 'rejected')),
    title TEXT NOT NULL,
    token_estimate INTEGER,
    url TEXT,
    user_notes TEXT,
    volume TEXT,
    web_of_science_db TEXT,
    -- Foreign key constraints
    FOREIGN KEY (duplicate_of) REFERENCES articles(id)
);

-- ── Article ↔ tags / labels ─────────────────────────────────

CREATE TABLE IF NOT EXISTS article_tags (
    article_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (article_id, tag_id),
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS article_labels (
    article_id TEXT NOT NULL,
    label_id TEXT NOT NULL,
    PRIMARY KEY (article_id, label_id),
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE,
    FOREIGN KEY (label_id) REFERENCES labels(id) ON DELETE CASCADE
);

-- ── Audit trail ─────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS audit_entries (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'tag_add', 'tag_remove', 'label_add', 'label_remove',
        'criteria_match', 'ai_screen', 'manual_override', 'ai_summary',
        'error', 'dedup_auto', 'reference_import', 'reference_match'
    )),
    article_id TEXT,
    details TEXT,
    from_status TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    to_status TEXT,
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE
);

-- ── LLM configuration (single-row via CHECK) ────────────────

CREATE TABLE IF NOT EXISTS llm_config (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    api_key_encrypted TEXT,
    context_window_tokens INTEGER NOT NULL DEFAULT 50000,
    endpoint_url TEXT NOT NULL,
    max_concurrent_requests INTEGER NOT NULL DEFAULT 3,
    model_name TEXT NOT NULL,
    provider TEXT NOT NULL CHECK(provider IN ('openai', 'anthropic', 'google', 'mistral_ai', 'z_ai', 'llama_cpp', 'ollama', 'lm_studio', 'custom')),
    request_delay_ms INTEGER NOT NULL DEFAULT 500,
    skip_temperature INTEGER NOT NULL DEFAULT 0,
    temperature REAL NOT NULL DEFAULT 0.2
);

-- ── Summary ──────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS summary (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    citation_style TEXT NOT NULL DEFAULT 'APA',
    generated_at TEXT NOT NULL,
    summary_text TEXT NOT NULL
);

-- ── App-level settings (key-value store) ─────────────────────

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT
);

INSERT OR IGNORE INTO app_settings (key, value) VALUES ('fulltext_storage_dir', NULL);

-- ── Journal Index (reference data) ────────────────────────────
-- IMPORTANT: This table stores system-distributed reference data.
-- Do NOT include in project backup export/import or reset operations.
-- Populated from CSV files via the import_journals script and
-- auto-loaded from the bundled portal DB on first startup.
-- Must be created BEFORE articles/reference_papers (FK references).

CREATE TABLE IF NOT EXISTS journal_index (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    eissn TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,
    issn TEXT,
    journal_title TEXT NOT NULL,
    languages TEXT,
    publisher_address TEXT,
    publisher_name TEXT,
    source_file TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    web_of_science_categories TEXT
);

-- Unique indexes for ISSN/eISSN (partial — only non-empty values)
CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_issn
    ON journal_index(issn) WHERE issn IS NOT NULL AND issn != '';
CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_eissn
    ON journal_index(eissn) WHERE eissn IS NOT NULL AND eissn != '';
-- Case-insensitive title lookup index
CREATE INDEX IF NOT EXISTS idx_journal_title_lower
    ON journal_index(LOWER(journal_title));

-- ── Reference papers (deduplicated) ──────────────────────────

CREATE TABLE IF NOT EXISTS reference_papers (
    id TEXT PRIMARY KEY,
    abstract_text TEXT DEFAULT '',
    authors TEXT DEFAULT '[]',                     -- JSON array of strings
    citation_count INTEGER NOT NULL DEFAULT 0,     -- how many articles cite this paper
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    date TEXT,
    doi TEXT,
    eissn TEXT,
    end_page TEXT,
    import_source TEXT,
    issn TEXT,
    issue TEXT,
    journal TEXT,
    journal_index_id TEXT REFERENCES journal_index(id),
    keywords TEXT DEFAULT '[]',                    -- JSON array of strings
    language TEXT,
    match_status TEXT NOT NULL DEFAULT 'unmatched'
        CHECK(match_status IN ('unmatched', 'matched', 'imported', 'not_in_library')),
    matched_article_id TEXT,                       -- FK → articles.id
    notes TEXT,
    publication_year INTEGER,
    publisher TEXT,
    publisher_address TEXT,
    publisher_city TEXT,
    reference_count INTEGER NOT NULL DEFAULT 0,    -- how many articles reference this paper
    reference_type TEXT,
    ris_extras TEXT,                               -- JSON object
    start_page TEXT,
    title TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    url TEXT,
    volume TEXT,
    FOREIGN KEY (matched_article_id) REFERENCES articles(id)
);

-- ── Article ↔ reference paper links (junction) ───────────────

CREATE TABLE IF NOT EXISTS article_reference_links (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    parent_article_id TEXT NOT NULL,
    reference_paper_id TEXT NOT NULL,
    type INTEGER NOT NULL CHECK(type IN (0, 1)),
        -- 0 = citation (another article citing the parent)
        -- 1 = reference (a work cited by the parent)
    FOREIGN KEY (parent_article_id) REFERENCES articles(id) ON DELETE CASCADE,
    FOREIGN KEY (reference_paper_id) REFERENCES reference_papers(id) ON DELETE CASCADE,
    UNIQUE(parent_article_id, reference_paper_id, type)
);

-- ── Indexes ──────────────────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_articles_status ON articles(status);
CREATE INDEX IF NOT EXISTS idx_articles_duplicate_of ON articles(duplicate_of);
CREATE INDEX IF NOT EXISTS idx_articles_screened_at ON articles(screened_at);
CREATE INDEX IF NOT EXISTS idx_articles_data_length ON articles(data_length);
CREATE INDEX IF NOT EXISTS idx_articles_sequence_id ON articles(sequence_id);
CREATE INDEX IF NOT EXISTS idx_articles_changed_at ON articles(changed_at);
CREATE INDEX IF NOT EXISTS idx_audit_entries_article_id ON audit_entries(article_id);
CREATE INDEX IF NOT EXISTS idx_criteria_type ON criteria(type);
-- Unique DOI (excluding NULLs — prevents duplicate papers with same DOI)
CREATE UNIQUE INDEX IF NOT EXISTS uq_ref_papers_doi
    ON reference_papers(doi) WHERE doi IS NOT NULL;
-- Unique title + authors + year combination
CREATE UNIQUE INDEX IF NOT EXISTS uq_ref_papers_title_authors_year
    ON reference_papers(LOWER(title), authors, publication_year)
    WHERE title IS NOT NULL AND title != '' AND publication_year IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ref_papers_match ON reference_papers(match_status);
CREATE INDEX IF NOT EXISTS idx_ref_papers_matched_article ON reference_papers(matched_article_id);
CREATE INDEX IF NOT EXISTS idx_ref_links_parent ON article_reference_links(parent_article_id);
CREATE INDEX IF NOT EXISTS idx_ref_links_paper ON article_reference_links(reference_paper_id);
CREATE INDEX IF NOT EXISTS idx_ref_links_parent_type ON article_reference_links(parent_article_id, type);

-- ── Bibliometrics: Normalized author entities ────────────────

CREATE TABLE IF NOT EXISTS biblio_authors (
    id TEXT PRIMARY KEY,
    article_count INTEGER NOT NULL DEFAULT 0,
    avg_year REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    display_name TEXT NOT NULL,
    estimated_h_index INTEGER,
    first_author_count INTEGER NOT NULL DEFAULT 0,
    normalized_name TEXT NOT NULL,
    total_citations INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_biblio_authors_norm ON biblio_authors(normalized_name);

-- ── Bibliometrics: Article ↔ Author mapping ──────────────────

CREATE TABLE IF NOT EXISTS biblio_article_authors (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES biblio_authors(id) ON DELETE CASCADE,
    author_order INTEGER NOT NULL DEFAULT 0,
    raw_affiliation TEXT,
    raw_name TEXT,
    UNIQUE(article_id, author_id, author_order)
);
CREATE INDEX IF NOT EXISTS idx_baa_article ON biblio_article_authors(article_id);
CREATE INDEX IF NOT EXISTS idx_baa_author ON biblio_article_authors(author_id);

-- ── Bibliometrics: Normalized institution entities ───────────

CREATE TABLE IF NOT EXISTS biblio_institutions (
    id TEXT PRIMARY KEY,
    city TEXT,
    country TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    normalized_name TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_biblio_inst_norm ON biblio_institutions(normalized_name);

-- ── Bibliometrics: Author ↔ Institution mapping (per article) 

CREATE TABLE IF NOT EXISTS biblio_author_affiliations (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES biblio_authors(id) ON DELETE CASCADE,
    institution_id TEXT NOT NULL REFERENCES biblio_institutions(id) ON DELETE CASCADE,
    UNIQUE(article_id, author_id, institution_id)
);

-- ── Bibliometrics: Normalized terms (keywords + noun phrases) 

CREATE TABLE IF NOT EXISTS biblio_terms (
    id TEXT PRIMARY KEY,
    article_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    normalized_term TEXT NOT NULL,
    raw_term TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'metadata' CHECK(source IN ('metadata', 'ai_extracted', 'user_added')),
    term_type TEXT NOT NULL DEFAULT 'keyword' CHECK(term_type IN ('keyword', 'noun_phrase'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_biblio_terms_norm ON biblio_terms(normalized_term, term_type);

-- ── Bibliometrics: Article ↔ Term mapping ────────────────────

CREATE TABLE IF NOT EXISTS biblio_article_terms (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    frequency INTEGER NOT NULL DEFAULT 1,
    term_id TEXT NOT NULL REFERENCES biblio_terms(id) ON DELETE CASCADE,
    UNIQUE(article_id, term_id)
);
CREATE INDEX IF NOT EXISTS idx_bat_article ON biblio_article_terms(article_id);
CREATE INDEX IF NOT EXISTS idx_bat_term ON biblio_article_terms(term_id);

-- ── Bibliometrics: Saved network graphs ──────────────────────

CREATE TABLE IF NOT EXISTS biblio_network_meta (
    id TEXT PRIMARY KEY,
    article_filter TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    edge_count INTEGER NOT NULL DEFAULT 0,
    label TEXT NOT NULL,
    network_type TEXT NOT NULL CHECK(network_type IN (
        'co_authorship', 'co_occurrence', 'citation', 'biblio_coupling', 'co_citation'
    )),
    node_count INTEGER NOT NULL DEFAULT 0,
    params_json TEXT
);

CREATE TABLE IF NOT EXISTS biblio_network_nodes (
    id TEXT PRIMARY KEY,
    cluster INTEGER,
    entity_id TEXT NOT NULL,
    label TEXT NOT NULL,
    network_id TEXT NOT NULL REFERENCES biblio_network_meta(id) ON DELETE CASCADE,
    weight REAL NOT NULL DEFAULT 1.0,
    x REAL,
    y REAL,
    UNIQUE(network_id, entity_id)
);
CREATE INDEX IF NOT EXISTS idx_bnn_network ON biblio_network_nodes(network_id);

CREATE TABLE IF NOT EXISTS biblio_network_edges (
    id TEXT PRIMARY KEY,
    network_id TEXT NOT NULL REFERENCES biblio_network_meta(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    UNIQUE(network_id, source_id, target_id)
);
CREATE INDEX IF NOT EXISTS idx_bne_network ON biblio_network_edges(network_id);

"#;
