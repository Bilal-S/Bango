//! Import journal index CSV files into the Bango portal SQLite database.
//!
//! Usage:
//!   cargo run -- [csv_directory] [db_path]
//!
//! If no directory is supplied, defaults to ~/Documents/Journals.
//! If no db_path is supplied, defaults to ../../src-tauri/resources/journal_index.db
//! (relative to this crate, for bundling with the Tauri app).
//!
//! Supported CSV formats (auto-detected by header):
//!   Standard: "Journal title","ISSN","eISSN","Publisher name",
//!             "Publisher address","Languages","Web of Science Categories"
//!   JCR:      Title20,Title,Country,SCIE,SSCI,AHCI,ESCI
//!
//! Match priority for existing records:
//!   1. ISSN (exact)
//!   2. eISSN (exact, always checked to avoid UNIQUE violations)
//!   3. Journal title (case-insensitive)
//!   4. No match → insert new record
//!
//! Blank CSV values never overwrite existing data.

use std::env;
use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use csv::ReaderBuilder;
use rusqlite::{params, Connection};
use uuid::Uuid;

// ── CSV format detection ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum CsvFormat {
    Standard,
    Jcr,
}

/// Detect CSV format from the header row.
fn detect_format(headers: &csv::StringRecord) -> Option<CsvFormat> {
    if headers.is_empty() {
        return None;
    }

    let h0 = headers.get(0).unwrap_or("").trim().to_lowercase();

    // JCR files start with "Title20"
    if h0 == "title20" {
        return Some(CsvFormat::Jcr);
    }

    // Standard files start with "Journal title" (or similar)
    if h0.contains("journal") && h0.contains("title") {
        return Some(CsvFormat::Standard);
    }

    // Fallback: check if we see standard columns anywhere
    for i in 0..headers.len() {
        let h = headers.get(i).unwrap_or("").trim().to_lowercase();
        if h == "issn" || h == "eissn" {
            return Some(CsvFormat::Standard);
        }
    }

    None
}

// ── CSV row schema ──────────────────────────────────────────

#[derive(Debug)]
struct JournalCsvRow {
    journal_title: String,
    issn: String,
    eissn: String,
    publisher_name: String,
    publisher_address: String,
    languages: String,
    web_of_science_categories: String,
}

// ── Helpers ─────────────────────────────────────────────────

/// Default output: `src-tauri/resources/journal_index.db` relative to this crate.
fn portal_db_path() -> PathBuf {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");
    PathBuf::from(manifest_dir)
        .join("../../src-tauri/resources/journal_index.db")
}

/// Create the journal_index table (idempotent).
fn ensure_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS journal_index (
            id TEXT PRIMARY KEY,
            journal_title TEXT NOT NULL,
            issn TEXT,
            eissn TEXT,
            publisher_name TEXT,
            publisher_address TEXT,
            languages TEXT,
            web_of_science_categories TEXT,
            is_system INTEGER NOT NULL DEFAULT 0,
            source_file TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_issn
            ON journal_index(issn) WHERE issn IS NOT NULL AND issn != '';
        CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_eissn
            ON journal_index(eissn) WHERE eissn IS NOT NULL AND eissn != '';
        CREATE INDEX IF NOT EXISTS idx_journal_title_lower
            ON journal_index(LOWER(journal_title));",
    )
    .expect("Failed to create journal_index schema");
}

/// Normalise an ISSN/eISSN string: strip hyphens, trim whitespace, uppercase.
fn normalise_issn(raw: &str) -> String {
    raw.chars()
        .filter(|c| *c != '-' && *c != ' ')
        .collect::<String>()
        .to_uppercase()
}

/// Try to find an existing journal record.  Returns the id if matched.
fn find_existing(
    conn: &Connection,
    issn: &str,
    eissn: &str,
    title: &str,
) -> Option<String> {
    // Priority 1: ISSN match
    if !issn.is_empty() {
        let result = conn
            .query_row(
                "SELECT id FROM journal_index WHERE issn = ?1 LIMIT 1",
                [issn],
                |row| row.get::<_, String>(0),
            )
            .ok();
        if result.is_some() {
            return result;
        }
    }

    // Priority 2: eISSN match (always check to avoid UNIQUE constraint violation)
    if !eissn.is_empty() {
        let result = conn
            .query_row(
                "SELECT id FROM journal_index WHERE eissn = ?1 LIMIT 1",
                [eissn],
                |row| row.get::<_, String>(0),
            )
            .ok();
        if result.is_some() {
            return result;
        }
    }

    // Priority 3: title match (case-insensitive)
    if !title.is_empty() {
        let result = conn
            .query_row(
                "SELECT id FROM journal_index WHERE LOWER(journal_title) = LOWER(?1) LIMIT 1",
                [title],
                |row| row.get::<_, String>(0),
            )
            .ok();
        if result.is_some() {
            return result;
        }
    }

    None
}

/// Insert a new journal record.
fn insert_journal(
    conn: &Connection,
    row: &JournalCsvRow,
    source_file: &str,
    is_system: bool,
) {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO journal_index
            (id, journal_title, issn, eissn, publisher_name,
             publisher_address, languages, web_of_science_categories,
             is_system, source_file, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            row.journal_title,
            row.issn,
            row.eissn,
            row.publisher_name,
            row.publisher_address,
            row.languages,
            row.web_of_science_categories,
            is_system as i32,
            source_file,
            now,
            now,
        ],
    )
    .expect("Failed to insert journal record");
}

/// Update an existing journal record, preserving existing data when CSV value is blank.
fn update_journal(
    conn: &Connection,
    existing_id: &str,
    row: &JournalCsvRow,
    source_file: &str,
) {
    let now = Utc::now().to_rfc3339();
    // Only overwrite fields that have non-empty values from the CSV.
    // Blank values preserve the existing database content.
    conn.execute(
        "UPDATE journal_index SET
            journal_title = CASE WHEN ?1 != '' THEN ?1 ELSE journal_title END,
            issn          = CASE WHEN ?2 != '' THEN ?2 ELSE issn END,
            eissn         = CASE WHEN ?3 != '' THEN ?3 ELSE eissn END,
            publisher_name = CASE WHEN ?4 != '' THEN ?4 ELSE publisher_name END,
            publisher_address = CASE WHEN ?5 != '' THEN ?5 ELSE publisher_address END,
            languages     = CASE WHEN ?6 != '' THEN ?6 ELSE languages END,
            web_of_science_categories = CASE WHEN ?7 != '' THEN ?7 ELSE web_of_science_categories END,
            source_file = ?8,
            updated_at  = ?9
         WHERE id = ?10",
        params![
            row.journal_title,
            row.issn,
            row.eissn,
            row.publisher_name,
            row.publisher_address,
            row.languages,
            row.web_of_science_categories,
            source_file,
            now,
            existing_id,
        ],
    )
    .expect("Failed to update journal record");
}

/// Parse a standard-format CSV row.
fn parse_standard_row(record: &csv::StringRecord) -> JournalCsvRow {
    JournalCsvRow {
        journal_title: record.get(0).unwrap_or("").trim().to_string(),
        issn: normalise_issn(record.get(1).unwrap_or("")),
        eissn: normalise_issn(record.get(2).unwrap_or("")),
        publisher_name: record.get(3).unwrap_or("").trim().to_string(),
        publisher_address: record.get(4).unwrap_or("").trim().to_string(),
        languages: record.get(5).unwrap_or("").trim().to_string(),
        web_of_science_categories: record.get(6).unwrap_or("").trim().to_string(),
    }
}

/// Parse a JCR-format CSV row.
/// Fields: Title20, Title, Country, SCIE, SSCI, AHCI, ESCI
fn parse_jcr_row(record: &csv::StringRecord) -> JournalCsvRow {
    let title = record.get(1).unwrap_or("").trim().to_string();
    let country = record.get(2).unwrap_or("").trim().to_string();

    // Combine SCIE/SSCI/AHCI/ESCI flags into WoS categories
    let mut categories: Vec<&str> = Vec::new();
    let flags = [
        ("SCIE", 3),
        ("SSCI", 4),
        ("AHCI", 5),
        ("ESCI", 6),
    ];
    for (label, idx) in &flags {
        let val = record.get(*idx).unwrap_or("").trim().to_uppercase();
        if val == "X" || val == "YES" || val == "1" || val == *label {
            categories.push(label);
        }
    }

    JournalCsvRow {
        journal_title: title,
        issn: String::new(),        // JCR has no ISSN
        eissn: String::new(),       // JCR has no eISSN
        publisher_name: String::new(),
        publisher_address: country,
        languages: String::new(),
        web_of_science_categories: categories.join(", "),
    }
}

/// Parse a single CSV file and return journal rows with detected format.
fn parse_csv(path: &PathBuf) -> Option<(CsvFormat, Vec<JournalCsvRow>)> {
    let mut reader = ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .unwrap_or_else(|e| panic!("Failed to open CSV {:?}: {}", path, e));

    // Detect format from headers
    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(e) => {
            eprintln!("  ❌ Cannot read headers from {:?}: {}", path, e);
            return None;
        }
    };

    let format = match detect_format(&headers) {
        Some(f) => f,
        None => {
            eprintln!(
                "  ⚠ Unknown CSV format in {:?}. Headers: {:?}",
                path.file_name().unwrap_or_default(),
                headers
            );
            return None;
        }
    };

    let fmt_label = match format {
        CsvFormat::Standard => "Standard",
        CsvFormat::Jcr => "JCR",
    };
    println!("   Format: {}", fmt_label);

    let mut rows = Vec::new();
    for result in reader.records() {
        match result {
            Ok(record) => {
                let min_cols = match format {
                    CsvFormat::Standard => 7,
                    CsvFormat::Jcr => 7,
                };
                if record.len() < min_cols {
                    eprintln!(
                        "  ⚠ Skipping malformed row ({} fields, expected {})",
                        record.len(),
                        min_cols
                    );
                    continue;
                }

                let row = match format {
                    CsvFormat::Standard => parse_standard_row(&record),
                    CsvFormat::Jcr => parse_jcr_row(&record),
                };

                rows.push(row);
            }
            Err(e) => {
                eprintln!("  ⚠ CSV parse error in {:?}: {}", path, e);
            }
        }
    }
    Some((format, rows))
}

/// Process a single CSV file: match → insert or update.
fn process_csv_file(conn: &Connection, path: &PathBuf, is_system: bool) -> (usize, usize, usize) {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    println!("📄 Processing: {}", file_name);

    let (format, rows) = match parse_csv(path) {
        Some(r) => r,
        None => {
            println!("   ⏭ Skipped (unrecognized format)");
            return (0, 0, 0);
        }
    };

    let mut inserted = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;

    for row in &rows {
        // Skip rows with empty title
        if row.journal_title.is_empty() {
            skipped += 1;
            continue;
        }

        match find_existing(conn, &row.issn, &row.eissn, &row.journal_title) {
            Some(existing_id) => {
                update_journal(conn, &existing_id, row, &file_name);
                updated += 1;
            }
            None => {
                insert_journal(conn, row, &file_name, is_system);
                inserted += 1;
            }
        }
    }

    let fmt_label = match format {
        CsvFormat::Standard => "Standard",
        CsvFormat::Jcr => "JCR",
    };
    println!(
        "   ✅ {} inserted, {} updated, {} skipped ({} rows, {} format)",
        inserted, updated, skipped, rows.len(), fmt_label
    );
    (inserted, updated, skipped)
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Determine CSV directory
    let csv_dir = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        dirs::document_dir()
            .expect("Cannot determine Documents directory")
            .join("Journals")
    };

    // Optional: explicit DB path (2nd argument), otherwise default portal DB
    let db_path_override = if args.len() > 2 {
        Some(PathBuf::from(&args[2]))
    } else {
        None
    };

    if !csv_dir.exists() {
        eprintln!("❌ CSV directory does not exist: {:?}", csv_dir);
        eprintln!("   Usage: import_journals [csv_directory]");
        std::process::exit(1);
    }

    // Open portal DB
    let db_path = db_path_override.unwrap_or_else(portal_db_path);
    println!("🗄  Portal DB: {:?}", db_path);

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create data directory");
    }

    let conn = Connection::open(&db_path).expect("Failed to open portal database");
    ensure_schema(&conn);

    // Collect CSV files, sorted alphabetically
    let mut csv_files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&csv_dir).expect("Failed to read CSV directory") {
        match entry {
            Ok(e) => {
                let path = e.path();
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
                if ext.as_deref() == Some("csv") {
                    csv_files.push(path);
                }
            }
            Err(e) => eprintln!("⚠ Error reading directory entry: {}", e),
        }
    }
    csv_files.sort();

    if csv_files.is_empty() {
        eprintln!("⚠ No CSV files found in {:?}", csv_dir);
        std::process::exit(0);
    }

    println!("📂 Found {} CSV file(s) in {:?}", csv_files.len(), csv_dir);
    println!();

    // Mark the first import as system data (bundled with the distribution)
    let is_system = true;

    let mut total_inserted = 0usize;
    let mut total_updated = 0usize;
    let mut total_skipped = 0usize;

    for csv_path in &csv_files {
        let (ins, upd, skip) = process_csv_file(&conn, csv_path, is_system);
        total_inserted += ins;
        total_updated += upd;
        total_skipped += skip;
    }

    println!();
    println!("═━═━═━═━═━═━═━═━═━═━═━═━═━═━═━═");
    println!("📊 Totals:");
    println!("   Inserted: {}", total_inserted);
    println!("   Updated:  {}", total_updated);
    println!("   Skipped:  {}", total_skipped);
    println!("   Total in DB: {}", {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))
            .unwrap_or(0);
        count
    });
    println!("═━═━═━═━═━═━═━═━═━═━═━═━═━═━═━═");
}