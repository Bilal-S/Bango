//! The embedding backend selection type.
//!
//! Domain type owned by the embedding module (the service router consumes
//! it); `db::app_settings_repo` owns only the `embedding_backend` key's
//! persistence. Leaf module: imports nothing from the crate.

/// Which backend generates embeddings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EmbeddingBackend {
    /// The configured LLM provider's embedding API (OpenAI, Mistral, Google,
    /// Ollama, LM Studio, llama.cpp, Custom). The default and the
    /// pre-local-embeddings behavior.
    #[default]
    ConfiguredProvider,
    /// Bango Local: on-device inference via the downloaded local model.
    BangoLocal,
}

/// Serialized form of the `ConfiguredProvider` backend.
const CONFIGURED_PROVIDER_VALUE: &str = "configured_provider";

/// Serialized form of the `BangoLocal` backend.
const BANGO_LOCAL_VALUE: &str = "bango_local";

impl EmbeddingBackend {
    /// Canonical serialized form stored in `app_settings.embedding_backend`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ConfiguredProvider => CONFIGURED_PROVIDER_VALUE,
            Self::BangoLocal => BANGO_LOCAL_VALUE,
        }
    }

    /// Parse the API string form exactly (`configured_provider` |
    /// `bango_local`); `None` for anything else. Unlike [`Self::parse`]
    /// (the forgiving DB read), this is the strict boundary for command
    /// arguments so an invalid selection surfaces as an error.
    #[must_use]
    pub fn parse_exact(value: &str) -> Option<Self> {
        match value {
            CONFIGURED_PROVIDER_VALUE => Some(Self::ConfiguredProvider),
            BANGO_LOCAL_VALUE => Some(Self::BangoLocal),
            _ => None,
        }
    }

    /// Parse the stored form. Absent or unrecognized values fall back to the
    /// default so a corrupted row never silently selects the local backend.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some(v) if v == BANGO_LOCAL_VALUE => Self::BangoLocal,
            _ => Self::ConfiguredProvider,
        }
    }
}
