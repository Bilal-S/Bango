use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAim {
    pub id: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Criterion {
    pub id: String,
    pub criterion_type: CriterionType,
    pub text: String,
    pub priority: Priority,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CriterionType {
    Inclusion,
    Exclusion,
}

impl CriterionType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inclusion => "inclusion",
            Self::Exclusion => "exclusion",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    Critical = 5,
    High = 4,
    Standard = 3,
    Low = 2,
    Optional = 1,
}

impl Priority {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Standard => "standard",
            Self::Low => "low",
            Self::Optional => "optional",
        }
    }
}
