//! Performance benchmark for `article_repo::max_article_char_len`.
//!
//! Measures execution time of the query that finds the MAX character length
//! among unscreened working articles. This query is called during screening
//! readiness checks and token estimation (Section 9.6 of v3 spec).
//!
//! The v3 spec (Section 16.1) targets < 200ms for UI list operations.
//! Since `max_article_char_len` feeds into a readiness check that blocks the
//! screening UI, we use 200ms as the hard upper bound.

use std::time::Instant;

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;

/// Helper: seed `n` articles into the database as unscreened working articles.
/// Each article gets a `data_length` proportional to its index to simulate
/// realistic variance in abstract lengths.
fn seed_articles(conn: &rusqlite::Connection, n: usize) {
    let tx = conn.unchecked_transaction().expect("transaction start");

    // Get next sequence_id
    let base_seq: i64 = tx
        .query_row("SELECT COALESCE(MAX(sequence_id), 0) + 1 FROM articles", [], |row| row.get(0))
        .unwrap_or(1);

    for i in 0..n {
        // Simulate realistic data_length: 500–15000 chars
        let data_length = 500 + (i % 30) * 500;
        let id = format!("perf-art-{i}");
        let title = format!("Performance Test Article {i}");
        let abstract_text = "x".repeat(data_length - title.len());

        tx.execute(
            "INSERT INTO articles (
                id, sequence_id, status, title, abstract_text, authors,
                publication_year, keywords, data_length, token_estimate
            ) VALUES (?1, ?2, 'working', ?3, ?4, '[\"Author, A\"]', 2024, '[]', ?5, ?6)",
            rusqlite::params![
                id,
                base_seq + i as i64,
                title,
                abstract_text,
                data_length,
                data_length / 4,
            ],
        )
        .expect("insert article");
    }

    tx.commit().expect("commit");
}

/// Benchmark `max_article_char_len` at a given scale.
fn bench_at_scale(
    conn: &rusqlite::Connection,
    label: &str,
    count: usize,
    iterations: usize,
) -> Vec<f64> {
    let mut times = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        let result = article_repo::max_article_char_len(conn).expect("max_article_char_len");
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        assert!(result > 0, "max_article_char_len should return > 0 when articles exist");
        times.push(elapsed);
    }

    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg = times.iter().sum::<f64>() / times.len() as f64;
    let min = times[0];
    let max = times[times.len() - 1];
    let p50 = times[times.len() / 2];
    let p95 = times[(times.len() as f64 * 0.95) as usize];

    println!(
        "  {label:>12} | articles: {count:>5} | avg: {avg:>8.3}ms | min: {min:>8.3}ms | max: {max:>8.3}ms | p50: {p50:>8.3}ms | p95: {p95:>8.3}ms"
    );

    // v3 spec Section 16.1: UI operations < 200ms
    assert!(
        max < 200.0,
        "max_article_char_len took {max:.3}ms at {label} articles — exceeds 200ms target"
    );

    times
}

#[test]
fn test_max_article_char_len_performance_at_scale() {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("run migrations");

    println!("\n⏱  max_article_char_len Performance Benchmark");
    println!("{}", "─".repeat(90));

    let scales: Vec<(usize, &str)> =
        vec![(100, "100"), (1_000, "1K"), (5_000, "5K"), (10_000, "10K")];

    let iterations = 50;

    for (count, label) in &scales {
        // Clear previous articles
        conn.execute("DELETE FROM articles", []).expect("clear articles");

        // Seed articles
        seed_articles(&conn, *count);

        // Verify count
        let actual_count: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM articles WHERE status = 'working' AND screened_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(actual_count, *count, "article count mismatch");

        // Benchmark
        bench_at_scale(&conn, label, *count, iterations);
    }

    println!("{}", "─".repeat(90));
    println!("  ✅ All scales completed under 200ms target\n");
}

#[test]
fn test_max_article_char_len_returns_zero_when_empty() {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("run migrations");

    let result = article_repo::max_article_char_len(&conn).expect("max_article_char_len");
    assert_eq!(result, 0, "Should return 0 when no articles exist");
}

#[test]
fn test_max_article_char_len_excludes_non_working() {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("run migrations");

    // Insert articles in various statuses — only 'working' with screened_at NULL should count
    for (i, status) in ["duplicate", "included", "rejected", "working"].iter().enumerate() {
        let id = format!("status-art-{i}");
        let data_length = 10000 + i * 1000;
        conn.execute(
            "INSERT INTO articles (
                id, sequence_id, status, title, abstract_text, authors,
                publication_year, keywords, data_length, token_estimate, screened_at
            ) VALUES (?1, ?2, ?3, 'Title', 'Abstract', '[\"A\"]', 2024, '[]', ?4, ?5, CASE WHEN ?3 != 'working' THEN '2026-01-01T00:00:00Z' ELSE NULL END)",
            rusqlite::params![id, (i + 1) as i64, status, data_length, data_length / 4],
        )
        .expect("insert");
    }

    // Also insert a screened working article — should be excluded
    conn.execute(
        "INSERT INTO articles (
            id, sequence_id, status, title, abstract_text, authors,
            publication_year, keywords, data_length, token_estimate, screened_at
        ) VALUES ('screened-working', 10, 'working', 'Title', 'Abstract', '[\"A\"]', 2024, '[]', 99999, 24999, '2026-01-01T00:00:00Z')",
        [],
    ).expect("insert screened working");

    let result = article_repo::max_article_char_len(&conn).expect("max_article_char_len");

    // Only the unscreened working article (index 3, data_length = 13000) should be counted
    assert_eq!(result, 13000, "Should only count unscreened working articles, got {result}");
}
