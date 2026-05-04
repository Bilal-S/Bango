use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    pub source: LabelSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LabelSource {
    AiGenerated,
    UserCreated,
}

impl LabelSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AiGenerated => "ai_generated",
            Self::UserCreated => "user_created",
        }
    }
}
