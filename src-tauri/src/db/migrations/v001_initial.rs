pub const VERSION: i32 = 1;

pub const UP_SQL: &str = r#"
-- ── Core tables ──────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS research_aims (
    id TEXT PRIMARY KEY,
    text TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS criteria (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL CHECK(type IN ('inclusion', 'exclusion')),
    text TEXT NOT NULL,
    priority TEXT NOT NULL CHECK(priority IN ('critical', 'high', 'standard', 'low', 'optional')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL CHECK(source IN ('ai_suggested', 'user_created', 'ris_keyword')),
    color TEXT
);

CREATE TABLE IF NOT EXISTS labels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL CHECK(source IN ('ai_generated', 'user_created')),
    color TEXT
);

-- ── Articles ─────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS articles (
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
    data_length INTEGER,
    token_estimate INTEGER,
    actual_tokens INTEGER,
    sequence_id INTEGER NOT NULL DEFAULT 0,
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
    article_id TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'tag_add', 'tag_remove', 'label_add', 'label_remove',
        'criteria_match', 'ai_screen', 'manual_override', 'ai_summary', 'error', 'dedup_auto'
    )),
    from_status TEXT,
    to_status TEXT,
    details TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE
);

-- ── LLM configuration (single-row via CHECK) ────────────────

CREATE TABLE IF NOT EXISTS llm_config (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    provider TEXT NOT NULL CHECK(provider IN ('openai', 'anthropic', 'google', 'mistral_ai', 'z_ai', 'llama_cpp', 'ollama', 'lm_studio', 'custom')),
    endpoint_url TEXT NOT NULL,
    api_key_encrypted TEXT,
    model_name TEXT NOT NULL,
    temperature REAL NOT NULL DEFAULT 0.2,
    max_concurrent_requests INTEGER NOT NULL DEFAULT 3,
    request_delay_ms INTEGER NOT NULL DEFAULT 500,
    context_window_tokens INTEGER NOT NULL DEFAULT 50000,
    skip_temperature INTEGER NOT NULL DEFAULT 0
);

-- ── Indexes ──────────────────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_articles_status ON articles(status);
CREATE INDEX IF NOT EXISTS idx_articles_duplicate_of ON articles(duplicate_of);
CREATE INDEX IF NOT EXISTS idx_articles_screened_at ON articles(screened_at);
CREATE INDEX IF NOT EXISTS idx_articles_data_length ON articles(data_length);
CREATE INDEX IF NOT EXISTS idx_articles_sequence_id ON articles(sequence_id);
CREATE INDEX IF NOT EXISTS idx_audit_entries_article_id ON audit_entries(article_id);
CREATE INDEX IF NOT EXISTS idx_criteria_type ON criteria(type);
"#;
