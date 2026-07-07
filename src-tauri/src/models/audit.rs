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
#[serde(rename_all = "camelCase")]
pub enum AuditAction {
    Import,
    DedupMerge,
    DedupFlag,
    StatusChange,
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
    WikiIngestError,
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
}

impl AuditAction {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::DedupMerge => "dedup_merge",
            Self::DedupFlag => "dedup_flag",
            Self::StatusChange => "status_change",
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
            Self::WikiIngestError => "wiki_ingest_error",
            Self::Translation => "translation",
            Self::TranslationError => "translation_error",
            Self::SearchStrategy => "search_strategy",
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
