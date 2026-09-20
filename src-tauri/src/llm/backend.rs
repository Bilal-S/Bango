//! The LLM generation backend selection type.
//!
//! Domain type owned by the llm module (the orchestrator's effective-config
//! provider consumes it); `db::app_settings_repo` owns only the
//! `llm_backend` key's persistence. Leaf module: imports nothing from the
//! crate.

/// Which backend serves generation calls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LlmBackend {
    /// The configured LLM provider (cloud or a user-run local server). The
    /// default and the pre-Bango-AI behavior.
    #[default]
    ConfiguredProvider,
    /// Bango AI: on-device inference via the downloaded llama.cpp runtime and
    /// Qwen3.5 model.
    BangoAi,
}

/// Serialized form of the `ConfiguredProvider` backend.
const CONFIGURED_PROVIDER_VALUE: &str = "configured_provider";

/// Serialized form of the `BangoAi` backend.
const BANGO_AI_VALUE: &str = "bango_ai";

impl LlmBackend {
    /// Canonical serialized form stored in `app_settings.llm_backend`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ConfiguredProvider => CONFIGURED_PROVIDER_VALUE,
            Self::BangoAi => BANGO_AI_VALUE,
        }
    }

    /// Parse the API string form exactly (`configured_provider` | `bango_ai`);
    /// `None` for anything else. Unlike [`Self::parse`] (the forgiving DB
    /// read), this is the strict boundary for command arguments so an invalid
    /// selection surfaces as an error.
    #[must_use]
    pub fn parse_exact(value: &str) -> Option<Self> {
        match value {
            CONFIGURED_PROVIDER_VALUE => Some(Self::ConfiguredProvider),
            BANGO_AI_VALUE => Some(Self::BangoAi),
            _ => None,
        }
    }

    /// Parse the stored form. Absent or unrecognized values fall back to the
    /// default so a corrupted row never silently selects the local backend.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some(v) if v == BANGO_AI_VALUE => Self::BangoAi,
            _ => Self::ConfiguredProvider,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exact_rejects_unknown_and_parse_falls_back() {
        assert_eq!(
            LlmBackend::parse_exact("configured_provider"),
            Some(LlmBackend::ConfiguredProvider)
        );
        assert_eq!(LlmBackend::parse_exact("bango_ai"), Some(LlmBackend::BangoAi));
        assert_eq!(LlmBackend::parse_exact("BANGO_AI"), None);
        assert_eq!(LlmBackend::parse_exact(""), None);
        assert_eq!(LlmBackend::parse_exact("garbage-value"), None);
        assert_eq!(LlmBackend::parse(None), LlmBackend::ConfiguredProvider);
        assert_eq!(LlmBackend::parse(Some("")), LlmBackend::ConfiguredProvider);
        assert_eq!(LlmBackend::parse(Some("garbage-value")), LlmBackend::ConfiguredProvider);
        assert_eq!(LlmBackend::parse(Some("bango_ai")), LlmBackend::BangoAi);
        assert_eq!(LlmBackend::default(), LlmBackend::ConfiguredProvider);
        for backend in [LlmBackend::ConfiguredProvider, LlmBackend::BangoAi] {
            assert_eq!(LlmBackend::parse_exact(backend.as_str()), Some(backend));
        }
    }
}
