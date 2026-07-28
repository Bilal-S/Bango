//! Wiki chat delegate.
//!
//! Extracted from the pre-split `wiki_cmd.rs` (refactor v6). Body moved
//! VERBATIM; no behavioral change.

use crate::commands::chat::ChatMessage;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::chat as wiki_chat_mod;

/// Send a wiki-grounded chat message (FTS5 RAG). Returns the assistant response.
#[tauri::command]
pub async fn wiki_chat(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    question: String,
    history: Vec<ChatMessage>,
) -> Result<String, AppError> {
    wiki_chat_mod::wiki_chat(db_state, orchestrator, &question, &history).await
}
