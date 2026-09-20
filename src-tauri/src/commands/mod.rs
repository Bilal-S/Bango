pub mod app_settings;
pub mod articles;
pub mod bango_ai;
pub mod biblio_cmd;
pub mod chat;
pub mod citation_finder;
pub mod criteria;
pub mod dedup;
pub mod embedding;
pub mod export_cmd;
pub mod full_text;
pub mod import;
pub mod labels;
pub mod llm_config;
pub mod local_embeddings;
pub mod openalex;
pub mod prisma;
pub mod references;
pub mod scraping;
pub mod screening;
pub mod search_strategy;
pub mod startup;
pub mod summary;
pub mod tags;
pub mod translation;
pub mod trends;
pub mod wiki_cmd;
pub mod zotero;

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub status: String,
    pub article_count: usize,
}
