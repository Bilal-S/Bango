use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub source: TagSource,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TagSource {
    AiSuggested,
    UserCreated,
    RisKeyword,
}

impl TagSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AiSuggested => "ai_suggested",
            Self::UserCreated => "user_created",
            Self::RisKeyword => "ris_keyword",
        }
    }
}
