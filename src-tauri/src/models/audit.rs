use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    pub article_id: String,
    pub timestamp: String,
    pub action: AuditAction,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub details: Option<String>,
    pub source: AuditSource,
    /// First 40 chars of the article title (for dashboard context)
    pub article_title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Import,
    DedupMerge,
    DedupFlag,
    StatusChange,
    NoteAdd,
    TagAdd,
    TagRemove,
    LabelAdd,
    LabelRemove,
    CriteriaMatch,
    AiScreen,
    ManualOverride,
    AiSummary,
    Error,
    DedupAuto,
    ReferenceImport,
    ReferenceMatch,
    /// Plan-A translation rewrote the working article text to English
    /// (`translation_status = 'succeeded'`).
    Translation,
    /// A translation job failed (`translation_status = 'failed'`); the error
    /// message is stored in `details`.
    TranslationError,
    /// Search Strategy Builder produced a database-ready Boolean search
    /// strategy from the research aims + criteria (spec §8.4). System-level
    /// audit row (`article_id = NULL`); the `details` field records a compact
    /// summary ("Generated 8-database search strategy for N aim(s)").
    SearchStrategy,
    /// A metadata field (Authors, Affiliation, Journal, Year, Lang, DOI,
    /// Keywords) was edited in-place via the Article Detail "Metadata" card.
    /// The `details` field records which field changed
    /// (e.g. "Metadata edited: DOI").
    MetadataEdit,
    /// The user cleared the AI reasoning text + confidence from an article
    /// via the trashcan icon in the AI Decision card's expanded header. Only
    /// `ai_reasoning` + `ai_confidence` are nulled; `ai_decision`, `status`,
    /// `screened_at`, and `manual_override` are preserved so the decision
    /// and screening history stay intact.
    AiScreenClear,
}

impl AuditAction {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::DedupMerge => "dedup_merge",
            Self::DedupFlag => "dedup_flag",
            Self::StatusChange => "status_change",
            Self::NoteAdd => "note_add",
            Self::TagAdd => "tag_add",
            Self::TagRemove => "tag_remove",
            Self::LabelAdd => "label_add",
            Self::LabelRemove => "label_remove",
            Self::CriteriaMatch => "criteria_match",
            Self::AiScreen => "ai_screen",
            Self::ManualOverride => "manual_override",
            Self::AiSummary => "ai_summary",
            Self::Error => "error",
            Self::DedupAuto => "dedup_auto",
            Self::ReferenceImport => "reference_import",
            Self::ReferenceMatch => "reference_match",
            Self::Translation => "translation",
            Self::TranslationError => "translation_error",
            Self::SearchStrategy => "search_strategy",
            Self::MetadataEdit => "metadata_edit",
            Self::AiScreenClear => "ai_screen_clear",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditSource {
    Ai,
    User,
    System,
}

impl AuditSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::User => "user",
            Self::System => "system",
        }
    }
}

/// A single import activity row - one per file import, with the correct article count.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportActivity {
    pub id: String,
    pub timestamp: String,
    pub filename: String,
    pub count: usize,
}

/// A unified activity-feed entry that merges individual audit rows and grouped
/// import rows into a single timestamp-ordered stream. The frontend receives a
/// flat, correctly paginated list - no client-side merge or re-sort needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityFeedEntry {
    pub id: String,
    pub timestamp: String,
    /// `"audit"` for individual entries, `"import"` for grouped import rows.
    pub kind: String,
    /// Audit-specific: the action label (e.g. `"ai_screen"`, `"status_change"`).
    pub action: Option<String>,
    /// Audit-specific: the article UUID, or `null` for system-level entries.
    pub article_id: Option<String>,
    /// Human-readable detail text (audit details or import filename).
    pub details: Option<String>,
    /// Audit-specific: `"ai"`, `"user"`, or `"system"`.
    pub source: Option<String>,
    /// Audit-specific: first 55 chars of the article title.
    pub article_title: Option<String>,
    /// Import-specific: the filename without the `Imported from ` prefix.
    pub filename: Option<String>,
    /// Import-specific: number of articles in this import batch.
    pub count: Option<usize>,
}
