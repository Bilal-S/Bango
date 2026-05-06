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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
