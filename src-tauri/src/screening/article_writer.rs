use crate::db::biblio_repo;
use crate::error::AppError;
use crate::models::article::Article;
use crate::models::biblio::{TermSource, TermType};
use crate::screening::decision::ArticleDecision;
use crate::screening::tags_labels::{create_or_match_label, create_or_match_tag};
use rusqlite::Connection;

/// Mark a single article as a screening error: set `screening_error = 1`,
/// `screened_at = now`, and insert an `ai_screen` audit entry with the error
/// details.
pub fn set_screening_error(
    conn: &Connection,
    article_id: &str,
    error_message: &str,
    raw_response: Option<&str>,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET screening_error = 1, screened_at = datetime('now'), changed_at = datetime('now') WHERE id = ?1",
        rusqlite::params![article_id],
    )?;

    let audit_id = uuid::Uuid::new_v4().to_string();
    let details = match raw_response {
        Some(raw) => {
            let truncated = &raw[..raw.len().min(300)];
            format!("Screening error: {error_message}\n\nRaw LLM response (first 300 chars): {truncated}")
        }
        None => format!("Screening error: {error_message}"),
    };
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'ai_screen', ?3, 'ai')",
        rusqlite::params![audit_id, article_id, details],
    )?;

    Ok(())
}

/// Mark every article in `batch` as a screening error with the same reason.
///
/// Replaces the verbatim `for article in &batch { set_screening_error(...) }`
/// loops that appeared at 3 sites in `run_sync` (non-transient error,
/// count-mismatch, parse-error).
pub fn mark_batch_screening_error(
    conn: &Connection,
    batch: &[Article],
    reason: &str,
    raw_response: Option<&str>,
) -> Result<(), AppError> {
    for article in batch {
        set_screening_error(conn, &article.id, reason, raw_response)?;
    }
    Ok(())
}

/// Write one article's screening decision to the DB: update the article row
/// (status, ai_decision, ai_reasoning, confidence, matched criteria, tokens),
/// create/match suggested tags, apply auto-labels from matched criteria, and
/// optionally save extracted terms.
///
/// Covers both the stage-1 and stage-2 per-article write blocks.
/// `save_terms` is `true` for stage-1 (which extracts terms) and `false` for
/// stage-2 (which does not).
///
/// Takes `&Connection` (caller manages lock scope; `MutexGuard` is `!Send`).
#[allow(clippy::too_many_arguments)]
pub fn write_article_screening_result(
    conn: &Connection,
    article_id: &str,
    decision: &ArticleDecision,
    confidence: f64,
    actual_tokens: Option<usize>,
    suggested_tags: &[String],
    save_terms: bool,
    extracted_terms: &[String],
) -> Result<(), AppError> {
    update_article_after_screening(
        conn,
        ScreeningUpdate {
            article_id,
            decision: &decision.final_decision,
            reasoning: &decision.reasoning,
            confidence,
            matched_inc: &decision.augmented_inc,
            matched_exc: &decision.augmented_exc,
            actual_tokens,
            evidence_sections: decision.evidence_sections.as_deref(),
        },
    )?;

    for tag_name in suggested_tags {
        let _ = create_or_match_tag(conn, tag_name, article_id);
    }

    for (prefix, text) in &decision.auto_label_criteria {
        let label_name = format!("{}: {}", prefix, text);
        let _ = create_or_match_label(conn, &label_name, article_id);
    }

    if save_terms && !extracted_terms.is_empty() {
        let terms: Vec<(String, TermType, TermSource)> = extracted_terms
            .iter()
            .map(|t| (t.clone(), TermType::NounPhrase, TermSource::AiExtracted))
            .collect();
        let _ = biblio_repo::save_article_terms(conn, article_id, &terms);
    }

    Ok(())
}

pub struct ScreeningUpdate<'a> {
    pub article_id: &'a str,
    pub decision: &'a str,
    pub reasoning: &'a str,
    pub confidence: f64,
    pub matched_inc: &'a [String],
    pub matched_exc: &'a [String],
    pub actual_tokens: Option<usize>,
    /// Tier 3: when `Some`, the audit detail line names the evidence sections
    /// used (e.g. `"§Methods, §Results"`), producing an `ai_screen_enhanced`
    /// audit action. When `None`, the audit action is the legacy `ai_screen`.
    pub evidence_sections: Option<&'a str>,
}

/// Write the screening decision to the DB: update the article row (status,
/// ai_decision, ai_reasoning, confidence, matched criteria, tokens) and insert
/// the audit entry (`ai_screen` or `ai_screen_enhanced`).
///
/// Takes `&Connection` (caller manages lock scope; `MutexGuard` is `!Send`).
pub fn update_article_after_screening(
    conn: &Connection,
    update: ScreeningUpdate,
) -> Result<(), AppError> {
    let new_status = if update.decision == "include" { "included" } else { "rejected" };
    let matched_inc_json = serde_json::to_string(update.matched_inc)?;
    let matched_exc_json = serde_json::to_string(update.matched_exc)?;

    // Tier 3 Gap 6: two-stage screening calls this twice for borderline
    // articles (stage 1 then stage 2). The flat `actual_tokens = ?7` write
    // previously discarded the stage-1 token count. Accumulate atomically via
    // `COALESCE(actual_tokens, 0) + ?7` so the column reflects the full cost
    // (stage 1 starts from NULL -> `COALESCE(NULL,0)+t == t`, unchanged).
    conn.execute(
        "UPDATE articles SET status = ?1, ai_decision = ?2, ai_reasoning = ?3, ai_confidence = ?4, \
         matched_inclusion_criteria = ?5, matched_exclusion_criteria = ?6, screened_at = datetime('now'), changed_at = datetime('now'), \
         actual_tokens = COALESCE(actual_tokens, 0) + ?7 \
         WHERE id = ?8",
        rusqlite::params![
            new_status,
            update.decision,
            update.reasoning,
            update.confidence,
            matched_inc_json,
            matched_exc_json,
            update.actual_tokens,
            update.article_id
        ],
    )?;

    let audit_id = uuid::Uuid::new_v4().to_string();
    // Tier 3: enhanced / two-stage stage-2 entries use the `ai_screen_enhanced`
    // action and name the evidence sections in the details so decision flips
    // are visible in the audit trail. Abstract / stage-1 entries stay `ai_screen`.
    let (action, details) = match update.evidence_sections {
        Some(sections) => (
            "ai_screen_enhanced",
            format!(
                "AI screened (enhanced) with {} evidence: {} (confidence: {:.2})",
                sections, update.decision, update.confidence
            ),
        ),
        None => (
            "ai_screen",
            format!("AI screened: {} (confidence: {:.2})", update.decision, update.confidence),
        ),
    };
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, action, from_status, to_status, details, source) \
         VALUES (?1, ?2, ?3, 'working', ?4, ?5, 'ai')",
        rusqlite::params![audit_id, update.article_id, action, new_status, details],
    )?;

    Ok(())
}
