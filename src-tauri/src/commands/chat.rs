use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::article::Article;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatArticleInfo {
    title: String,
    authors: Vec<String>,
    year: Option<i32>,
    keywords: Vec<String>,
    summary_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

fn get_summary_text(article: &Article) -> String {
    if let Some(ref ai_summary) = article.full_text_ai_summary {
        if !ai_summary.trim().is_empty() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(ai_summary) {
                if let Some(summary) = val.get("summary_150_250_words").and_then(|v| v.as_str()) {
                    return summary.to_string();
                }
            }
            return ai_summary.clone();
        }
    }
    if let Some(ref full_text) = article.full_text {
        if !full_text.trim().is_empty() {
            return full_text.clone();
        }
    }
    article.abstract_text.clone()
}

#[tauri::command]
pub async fn send_chat_message(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    article_ids: Vec<String>,
    history: Vec<ChatMessage>,
    new_message: String,
) -> Result<String, AppError> {
    let config = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?
    };

    let mut articles_info = Vec::new();
    {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        for id in &article_ids {
            let article = article_repo::get_article_by_id(&conn, id)?;
            let summary_text = get_summary_text(&article);
            articles_info.push(ChatArticleInfo {
                title: article.title.clone(),
                authors: article.authors.clone(),
                year: article.publication_year,
                keywords: article.keywords.clone(),
                summary_text,
            });
        }
    }

    let system_prompt = "You are a helpful academic research assistant. Answer the researcher's question using the provided article context. \
                         Do your best to provide a factual, accurate, and comprehensive answer based on the articles. \
                         Do not invent information. If the answer is based on one of the articles, cite the article (e.g., Author, Year) \
                         and reference the specific text when possible. Format your response in clean Markdown (using headings, lists, bold text, and tables where appropriate to present the information clearly).";

    let context_json = serde_json::to_string_pretty(&articles_info)
        .map_err(|e| AppError::Validation(format!("Failed to serialize article context: {}", e)))?;

    let mut user_prompt = String::new();
    user_prompt.push_str("Here is the JSON information about the selected articles for context:\n");
    user_prompt.push_str(&context_json);
    user_prompt.push_str("\n\n");

    if !history.is_empty() {
        user_prompt.push_str("Conversation history:\n");
        for msg in &history {
            let role_name = if msg.role == "user" { "User" } else { "Assistant" };
            user_prompt.push_str(&format!("{}: {}\n", role_name, msg.content));
        }
        user_prompt.push('\n');
    }

    user_prompt.push_str(&format!("User: {}\nAssistant:", new_message));

    let (response, _tokens) =
        orchestrator.send(&config, system_prompt, &user_prompt, LlmRequestType::Chat).await?;

    Ok(response)
}
