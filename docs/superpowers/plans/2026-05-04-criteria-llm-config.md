# Criteria & LLM Configuration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement CRUD operations for research aims and inclusion/exclusion criteria with priority levels, plus a complete LLM provider configuration system with encrypted API key storage and connection testing.

**Architecture:** Rust modules for criteria and LLM config database operations with AES-256-GCM encryption. Tauri commands expose CRUD and config operations. Vue views provide a three-section criteria editor and an LLM configuration form.

**Tech Stack:** Rust (rusqlite, aes-gcm, pbkdf2, sha2, reqwest), Tauri commands, Vue 3

**Depends on:** Plan 1 (Foundation & Database)

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/src/
├── crypto/
│   ├── mod.rs              (new: encryption module)
│   └── aes_gcm.rs          (new: AES-256-GCM encrypt/decrypt)
├── db/
│   ├── criteria_repo.rs    (new: aims + criteria DB operations)
│   ├── llm_config_repo.rs  (new: LLM config DB operations)
│   └── mod.rs              (modify: add new modules)
├── commands/
│   ├── criteria.rs         (new: criteria CRUD commands)
│   ├── llm_config.rs       (new: LLM config commands)
│   └── mod.rs              (modify: add modules)
├── tests/
│   ├── criteria_test.rs    (new: criteria tests)
│   ├── crypto_test.rs      (new: encryption tests)
│   └── priority_test.rs    (new: priority resolution tests)
```

### TypeScript/Vue (src/)

```
src/
├── views/
│   ├── criteria-editor.vue   (new: three-section editor)
│   └── llm-config.vue        (new: LLM configuration form)
├── composables/
│   ├── use-criteria.ts       (modify: add CRUD actions)
│   └── use-llm-config.ts     (new: LLM config composable)
├── router/
│   └── index.ts              (modify: update routes)
```

---

## Task 1: Encryption Module

**Files:**
- Create: `src-tauri/src/crypto/mod.rs`
- Create: `src-tauri/src/crypto/aes_gcm.rs`
- Create: `src-tauri/tests/crypto_test.rs`

- [ ] **Step 1: Add crypto dependencies to `src-tauri/Cargo.toml`**

```toml
aes-gcm = "0.10"
pbkdf2 = { version = "0.12", features = ["simple"] }
sha2 = "0.10"
rand = "0.8"
base64 = "0.22"
reqwest = { version = "0.12", features = ["json"] }
```

- [ ] **Step 2: Create `src-tauri/src/crypto/mod.rs`**

```rust
pub mod aes_gcm;
```

- [ ] **Step 3: Write failing tests in `src-tauri/tests/crypto_test.rs`**

```rust
use bango_lib::crypto::aes_gcm::{encrypt, decrypt, derive_key_from_machine};

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let key = [42u8; 32];
    let plaintext = "sk-test-api-key-12345";
    let encrypted = encrypt(plaintext.as_bytes(), &key).unwrap();
    let decrypted = decrypt(&encrypted, &key).unwrap();
    assert_eq!(String::from_utf8(decrypted).unwrap(), plaintext);
}

#[test]
fn test_different_keys_fail() {
    let key_a = [1u8; 32];
    let key_b = [2u8; 32];
    let plaintext = "secret";
    let encrypted = encrypt(plaintext.as_bytes(), &key_a).unwrap();
    assert!(decrypt(&encrypted, &key_b).is_err());
}

#[test]
fn test_encrypted_output_differs_from_input() {
    let key = [42u8; 32];
    let plaintext = "api-key-value";
    let encrypted = encrypt(plaintext.as_bytes(), &key).unwrap();
    // Base64 output should differ from plaintext
    assert_ne!(encrypted, plaintext);
}

#[test]
fn test_derive_key_deterministic() {
    let key_a = derive_key_from_machine();
    let key_b = derive_key_from_machine();
    assert_eq!(key_a, key_b);
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cd src-tauri && cargo test crypto_test --test crypto_test`
Expected: FAIL

- [ ] **Step 5: Implement `src-tauri/src/crypto/aes_gcm.rs`**

```rust
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, AeadCore, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use std::process::Command;

const SALT: &[u8; 16] = b"bango-app-salt16";
const ITERATIONS: u32 = 600_000;

/// Derives a 256-bit key from machine identity (hostname + username + app salt).
#[must_use]
pub fn derive_key_from_machine() -> [u8; 32] {
    let hostname = get_hostname();
    let username = get_username();
    let identity = format!("{}:{}", hostname, username);
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(identity.as_bytes(), SALT, ITERATIONS, &mut key);
    key
}

/// Derives a 256-bit key from a user-provided password.
#[must_use]
pub fn derive_key_from_password(password: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), SALT, ITERATIONS, &mut key);
    key
}

/// Encrypts plaintext using AES-256-GCM. Returns base64-encoded nonce+ciphertext.
pub fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Result<String, aes_gcm::Error> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .expect("Valid key length");
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext)?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

/// Decrypts base64-encoded nonce+ciphertext using AES-256-GCM.
pub fn decrypt(encoded: &str, key: &[u8; 32]) -> Result<Vec<u8>, aes_gcm::Error> {
    let combined = BASE64.decode(encoded).map_err(|_| aes_gcm::Error)?;
    if combined.len() < 12 {
        return Err(aes_gcm::Error);
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new_from_slice(key)
        .expect("Valid key length");
    cipher.decrypt(nonce, ciphertext)
}

fn get_hostname() -> String {
    Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown-host".to_string())
}

fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown-user".to_string())
}
```

- [ ] **Step 6: Add `pub mod crypto;` to `src-tauri/src/lib.rs`**

- [ ] **Step 7: Run tests**

Run: `cd src-tauri && cargo test crypto_test --test crypto_test`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/crypto/ src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/tests/crypto_test.rs
git commit -m "feat(crypto): add AES-256-GCM encryption with PBKDF2 key derivation"
```

---

## Task 2: Criteria Repository

**Files:**
- Create: `src-tauri/src/db/criteria_repo.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/tests/criteria_test.rs`

- [ ] **Step 1: Create `src-tauri/src/db/criteria_repo.rs`**

```rust
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::criterion::{Criterion, CriterionType, Priority, ResearchAim};

// Research Aims

pub fn get_all_aims(conn: &Connection) -> Result<Vec<ResearchAim>, AppError> {
    let mut stmt = conn.prepare("SELECT id, text, created_at FROM research_aims ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| {
        Ok(ResearchAim {
            id: row.get(0)?,
            text: row.get(1)?,
            created_at: row.get(2)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_aim(conn: &Connection, text: &str) -> Result<ResearchAim, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO research_aims (id, text, created_at) VALUES (?1, ?2, ?3)",
        params![id, text, now],
    )?;
    Ok(ResearchAim {
        id,
        text: text.to_string(),
        created_at: now,
    })
}

pub fn delete_aim(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM research_aims WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn update_aim(conn: &Connection, id: &str, text: &str) -> Result<ResearchAim, AppError> {
    conn.execute("UPDATE research_aims SET text = ?1 WHERE id = ?2", params![text, id])?;
    get_all_aims(conn)?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Aim {} not found", id)))
}

// Criteria

pub fn get_all_criteria(conn: &Connection) -> Result<Vec<Criterion>, AppError> {
    let mut stmt = conn.prepare("SELECT id, type, text, priority, created_at FROM criteria ORDER BY created_at")?;
    let rows = stmt.query_map([], row_to_criterion)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_criteria_by_type(conn: &Connection, criterion_type: &str) -> Result<Vec<Criterion>, AppError> {
    let mut stmt = conn.prepare("SELECT id, type, text, priority, created_at FROM criteria WHERE type = ?1 ORDER BY created_at")?;
    let rows = stmt.query_map([criterion_type], row_to_criterion)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_criterion(conn: &Connection, criterion_type: &str, text: &str, priority: &str) -> Result<Criterion, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO criteria (id, type, text, priority, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, criterion_type, text, priority, now],
    )?;
    Ok(Criterion {
        id,
        criterion_type: parse_criterion_type(criterion_type),
        text: text.to_string(),
        priority: parse_priority(priority),
        created_at: now,
    })
}

pub fn update_criterion(conn: &Connection, id: &str, text: &str, priority: &str) -> Result<Criterion, AppError> {
    conn.execute(
        "UPDATE criteria SET text = ?1, priority = ?2 WHERE id = ?3",
        params![text, priority, id],
    )?;
    get_all_criteria(conn)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Criterion {} not found", id)))
}

pub fn delete_criterion(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM criteria WHERE id = ?1", params![id])?;
    Ok(())
}

fn row_to_criterion(row: &rusqlite::Row<'_>) -> rusqlite::Result<Criterion> {
    let type_str: String = row.get(1)?;
    let priority_str: String = row.get(3)?;
    Ok(Criterion {
        id: row.get(0)?,
        criterion_type: parse_criterion_type(&type_str),
        text: row.get(2)?,
        priority: parse_priority(&priority_str),
        created_at: row.get(4)?,
    })
}

fn parse_criterion_type(s: &str) -> CriterionType {
    match s {
        "inclusion" => CriterionType::Inclusion,
        _ => CriterionType::Exclusion,
    }
}

fn parse_priority(s: &str) -> Priority {
    match s {
        "critical" => Priority::Critical,
        "high" => Priority::High,
        "low" => Priority::Low,
        "optional" => Priority::Optional,
        _ => Priority::Standard,
    }
}
```

- [ ] **Step 2: Update `src-tauri/src/db/mod.rs` — add `pub mod criteria_repo;` and `pub mod llm_config_repo;`**

- [ ] **Step 3: Write tests in `src-tauri/tests/criteria_test.rs`**

```rust
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::criteria_repo;

#[test]
fn test_create_and_get_aims() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let aim = criteria_repo::create_aim(&conn, "Study AI in healthcare").unwrap();
    assert_eq!(aim.text, "Study AI in healthcare");

    let aims = criteria_repo::get_all_aims(&conn).unwrap();
    assert_eq!(aims.len(), 1);
}

#[test]
fn test_delete_aim() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let aim = criteria_repo::create_aim(&conn, "Test aim").unwrap();
    criteria_repo::delete_aim(&conn, &aim.id).unwrap();
    assert!(criteria_repo::get_all_aims(&conn).unwrap().is_empty());
}

#[test]
fn test_create_criterion_with_priority() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let criterion = criteria_repo::create_criterion(&conn, "inclusion", "Must be about ML", "critical").unwrap();
    assert_eq!(criterion.text, "Must be about ML");
    assert!(matches!(criterion.criterion_type, crate::models::criterion::CriterionType::Inclusion));
    assert!(matches!(criterion.priority, crate::models::criterion::Priority::Critical));
}

#[test]
fn test_get_criteria_by_type() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    criteria_repo::create_criterion(&conn, "inclusion", "Include ML", "standard").unwrap();
    criteria_repo::create_criterion(&conn, "exclusion", "Exclude non-English", "high").unwrap();

    let inc = criteria_repo::get_criteria_by_type(&conn, "inclusion").unwrap();
    let exc = criteria_repo::get_criteria_by_type(&conn, "exclusion").unwrap();
    assert_eq!(inc.len(), 1);
    assert_eq!(exc.len(), 1);
}

#[test]
fn test_update_criterion() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let c = criteria_repo::create_criterion(&conn, "inclusion", "Original", "low").unwrap();
    let updated = criteria_repo::update_criterion(&conn, &c.id, "Updated", "critical").unwrap();
    assert_eq!(updated.text, "Updated");
    assert!(matches!(updated.priority, crate::models::criterion::Priority::Critical));
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test criteria_test --test criteria_test`
Expected: PASS

- [ ] **Step 5: Add `chrono` to Cargo.toml if not already present**

Check if `chrono` is already a dependency. If not, add:

```toml
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/db/criteria_repo.rs src-tauri/src/db/mod.rs src-tauri/tests/criteria_test.rs src-tauri/Cargo.toml
git commit -m "feat(criteria): add research aims and criteria repository with CRUD"
```

---

## Task 2.5: Priority Resolution Tests

**Files:**
- Create: `src-tauri/tests/priority_test.rs`

- [ ] **Step 1: Write priority edge case tests**

Create `src-tauri/tests/priority_test.rs`:

```rust
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::criteria_repo;
use bango_lib::models::criterion::Priority;

#[test]
fn test_priority_ordering() {
    assert!(Priority::Critical > Priority::High);
    assert!(Priority::High > Priority::Standard);
    assert!(Priority::Standard > Priority::Low);
    assert!(Priority::Low > Priority::Optional);
}

#[test]
fn test_criteria_priority_in_database() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    criteria_repo::create_criterion(&conn, "inclusion", "Critical item", "critical").unwrap();
    criteria_repo::create_criterion(&conn, "inclusion", "Standard item", "standard").unwrap();
    criteria_repo::create_criterion(&conn, "exclusion", "High item", "high").unwrap();

    let all = criteria_repo::get_all_criteria(&conn).unwrap();
    assert_eq!(all.len(), 3);

    let critical = all.iter().find(|c| c.text == "Critical item").unwrap();
    assert!(matches!(critical.priority, Priority::Critical));
}

#[test]
fn test_criteria_type_filtering() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    criteria_repo::create_criterion(&conn, "inclusion", "Include ML", "standard").unwrap();
    criteria_repo::create_criterion(&conn, "inclusion", "Include AI", "high").unwrap();
    criteria_repo::create_criterion(&conn, "exclusion", "Exclude non-English", "standard").unwrap();

    let inc = criteria_repo::get_criteria_by_type(&conn, "inclusion").unwrap();
    let exc = criteria_repo::get_criteria_by_type(&conn, "exclusion").unwrap();
    assert_eq!(inc.len(), 2);
    assert_eq!(exc.len(), 1);
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test priority_test --test priority_test`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/priority_test.rs
git commit -m "test(criteria): add priority ordering and type filtering tests"
```

---

## Task 3: LLM Config Repository

**Files:**
- Create: `src-tauri/src/db/llm_config_repo.rs`

- [ ] **Step 1: Create `src-tauri/src/db/llm_config_repo.rs`**

```rust
use rusqlite::{params, Connection};

use crate::crypto::aes_gcm;
use crate::error::AppError;
use crate::models::llm_config::{LlmConfig, LlmProvider};

pub fn get_config(conn: &Connection) -> Result<Option<LlmConfig>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT provider, endpoint_url, api_key_encrypted, model_name, temperature, \
         max_concurrent_requests, request_delay_ms, context_window_tokens FROM llm_config WHERE id = 1"
    )?;

    let result = stmt.query_row([], |row| {
        let provider_str: String = row.get(0)?;
        let provider = parse_provider(&provider_str);
        let api_key_encrypted: Option<String> = row.get(2)?;

        // Decrypt API key using machine-derived key
        let api_key_decrypted = api_key_encrypted.and_then(|enc| {
            let key = aes_gcm::derive_key_from_machine();
            aes_gcm::decrypt(&enc, &key).ok().and_then(|bytes| String::from_utf8(bytes).ok())
        });

        Ok(LlmConfig {
            provider,
            endpoint_url: row.get(1)?,
            api_key_encrypted: api_key_decrypted,
            model_name: row.get(3)?,
            temperature: row.get(4)?,
            max_concurrent_requests: row.get(5)?,
            request_delay_ms: row.get(6)?,
            context_window_tokens: row.get(7)?,
        })
    });

    match result {
        Ok(config) => Ok(Some(config)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

pub fn save_config(conn: &Connection, config: &LlmConfig) -> Result<(), AppError> {
    let key = aes_gcm::derive_key_from_machine();
    let encrypted_api_key = config.api_key_encrypted.as_ref()
        .map(|k| aes_gcm::encrypt(k.as_bytes(), &key))
        .transpose()
        .map_err(|_| AppError::Validation("Failed to encrypt API key".to_string()))?;

    conn.execute(
        "DELETE FROM llm_config WHERE id = 1",
        [],
    )?;

    conn.execute(
        "INSERT INTO llm_config (id, provider, endpoint_url, api_key_encrypted, model_name, \
         temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            config.provider.as_str(),
            config.endpoint_url,
            encrypted_api_key,
            config.model_name,
            config.temperature,
            config.max_concurrent_requests,
            config.request_delay_ms,
            config.context_window_tokens,
        ],
    )?;

    Ok(())
}

fn parse_provider(s: &str) -> LlmProvider {
    match s {
        "openai" => LlmProvider::Openai,
        "google" => LlmProvider::Google,
        "z_ai" => LlmProvider::ZAi,
        "llama_cpp" => LlmProvider::LlamaCpp,
        "ollama" => LlmProvider::Ollama,
        "lm_studio" => LlmProvider::Lmstudio,
        _ => LlmProvider::Custom,
    }
}
```

- [ ] **Step 2: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/llm_config_repo.rs
git commit -m "feat(llm): add LLM config repository with encrypted API key storage"
```

---

## Task 4: Criteria & LLM Config Tauri Commands

**Files:**
- Create: `src-tauri/src/commands/criteria.rs`
- Create: `src-tauri/src/commands/llm_config.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `src-tauri/src/commands/criteria.rs`**

```rust
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::error::AppError;
use crate::models::criterion::{Criterion, ResearchAim};

#[tauri::command]
pub fn get_research_aims(db_state: State<'_, DbState>) -> Result<Vec<ResearchAim>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::get_all_aims(&conn)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAimRequest { pub text: String; }

#[tauri::command]
pub fn create_research_aim(db_state: State<'_, DbState>, request: CreateAimRequest) -> Result<ResearchAim, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::create_aim(&conn, &request.text)
}

#[tauri::command]
pub fn delete_research_aim(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::delete_aim(&conn, &id)
}

#[tauri::command]
pub fn get_criteria(db_state: State<'_, DbState>) -> Result<Vec<Criterion>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::get_all_criteria(&conn)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCriterionRequest {
    pub criterion_type: String,
    pub text: String,
    pub priority: String,
}

#[tauri::command]
pub fn create_criterion(db_state: State<'_, DbState>, request: CreateCriterionRequest) -> Result<Criterion, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::create_criterion(&conn, &request.criterion_type, &request.text, &request.priority)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCriterionRequest {
    pub id: String,
    pub text: String,
    pub priority: String,
}

#[tauri::command]
pub fn update_criterion(db_state: State<'_, DbState>, request: UpdateCriterionRequest) -> Result<Criterion, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::update_criterion(&conn, &request.id, &request.text, &request.priority)
}

#[tauri::command]
pub fn delete_criterion(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    criteria_repo::delete_criterion(&conn, &id)
}
```

- [ ] **Step 2: Create `src-tauri/src/commands/llm_config.rs`**

```rust
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::client;
use crate::models::llm_config::LlmConfig;

#[tauri::command]
pub fn get_llm_config(db_state: State<'_, DbState>) -> Result<Option<LlmConfig>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    llm_config_repo::get_config(&conn)
}

#[tauri::command]
pub fn save_llm_config(db_state: State<'_, DbState>, config: LlmConfig) -> Result<(), AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    llm_config_repo::save_config(&conn, &config)
}

#[derive(Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn test_llm_connection(db_state: State<'_, DbState>) -> Result<TestConnectionResult, AppError> {
    let config = {
        let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
        llm_config_repo::get_config(&conn)?.ok_or_else(|| AppError::Validation("No LLM config found".to_string()))?
    };

    match client::send_chat_completion(&config, "You are a test.", "Say hello.").await {
        Ok(_) => Ok(TestConnectionResult {
            success: true,
            message: "Connection successful!".to_string(),
        }),
        Err(e) => Ok(TestConnectionResult {
            success: false,
            message: format!("Connection failed: {}", e),
        }),
    }
}

#[tauri::command]
pub fn is_local_provider(provider: String) -> bool {
    matches!(provider.as_str(), "llama_cpp" | "ollama" | "lm_studio")
}
```

- [ ] **Step 3: Update `src-tauri/src/commands/mod.rs` — add `pub mod criteria;` and `pub mod llm_config;`**

- [ ] **Step 4: Update `src-tauri/src/lib.rs` invoke handler with all new commands**

```rust
commands::criteria::get_research_aims,
commands::criteria::create_research_aim,
commands::criteria::delete_research_aim,
commands::criteria::get_criteria,
commands::criteria::create_criterion,
commands::criteria::update_criterion,
commands::criteria::delete_criterion,
commands::llm_config::get_llm_config,
commands::llm_config::save_llm_config,
commands::llm_config::test_llm_connection,
commands::llm_config::is_local_provider,
```

Also add `pub mod llm;` if not already present.

- [ ] **Step 5: Create minimal `src-tauri/src/llm/mod.rs` and `src-tauri/src/llm/client.rs` stubs**

`src-tauri/src/llm/mod.rs`:
```rust
pub mod client;
```

`src-tauri/src/llm/client.rs`:
```rust
use crate::error::AppError;
use crate::models::llm_config::LlmConfig;

pub async fn send_chat_completion(
    _config: &LlmConfig,
    _system_prompt: &str,
    _user_prompt: &str,
) -> Result<String, AppError> {
    // Will be fully implemented in Plan 5/6
    Err(AppError::Validation("LLM client not yet implemented".to_string()))
}
```

- [ ] **Step 6: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 7: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/ src-tauri/src/lib.rs src-tauri/src/llm/
git commit -m "feat(criteria-llm): add Tauri commands for criteria CRUD and LLM configuration"
```

---

## Task 5: Frontend Criteria Editor

**Files:**
- Create: `src/views/criteria-editor.vue`
- Modify: `src/router/index.ts`

- [ ] **Step 1: Create `src/views/criteria-editor.vue`**

```vue
<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { ResearchAim, Criterion } from '@/types';

const aims = ref<ResearchAim[]>([]);
const criteria = ref<Criterion[]>([]);
const newAimText = ref('');
const newInclusionText = ref('');
const newExclusionText = ref('');
const newInclusionPriority = ref<string>('standard');
const newExclusionPriority = ref<string>('standard');

const inclusionCriteria = computed(() => criteria.value.filter((c) => c.criterionType === 'inclusion'));
const exclusionCriteria = computed(() => criteria.value.filter((c) => c.criterionType === 'exclusion'));

onMounted(fetchAll);

async function fetchAll(): Promise<void> {
  const [aimsResult, criteriaResult] = await Promise.all([
    tauriCommand<ResearchAim[]>('get_research_aims'),
    tauriCommand<Criterion[]>('get_criteria'),
  ]);
  aims.value = aimsResult;
  criteria.value = criteriaResult;
}

async function addAim(): Promise<void> {
  if (!newAimText.value.trim()) return;
  await tauriCommand('create_research_aim', { request: { text: newAimText.value.trim() } });
  newAimText.value = '';
  await fetchAll();
}

async function deleteAim(id: string): Promise<void> {
  await tauriCommand('delete_research_aim', { id });
  await fetchAll();
}

async function addInclusion(): Promise<void> {
  if (!newInclusionText.value.trim()) return;
  await tauriCommand('create_criterion', {
    request: { criterionType: 'inclusion', text: newInclusionText.value.trim(), priority: newInclusionPriority.value },
  });
  newInclusionText.value = '';
  newInclusionPriority.value = 'standard';
  await fetchAll();
}

async function addExclusion(): Promise<void> {
  if (!newExclusionText.value.trim()) return;
  await tauriCommand('create_criterion', {
    request: { criterionType: 'exclusion', text: newExclusionText.value.trim(), priority: newExclusionPriority.value },
  });
  newExclusionText.value = '';
  newExclusionPriority.value = 'standard';
  await fetchAll();
}

async function deleteCriterion(id: string): Promise<void> {
  await tauriCommand('delete_criterion', { id });
  await fetchAll();
}

function priorityClass(priority: string): string {
  return `priority--${priority}`;
}
</script>

<template>
  <div class="criteria-editor">
    <h1>Criteria Editor</h1>

    <div class="criteria-editor__sections">
      <!-- Research Aims -->
      <section class="section">
        <h2>Research Aims</h2>
        <div class="section__input-row">
          <input v-model="newAimText" type="text" placeholder="Describe a research aim..." class="input" @keyup.enter="addAim" />
          <button class="btn btn--primary" @click="addAim">Add</button>
        </div>
        <ul class="section__list">
          <li v-for="aim in aims" :key="aim.id" class="section__item">
            <span>{{ aim.text }}</span>
            <button class="btn-icon" @click="deleteAim(aim.id)">×</button>
          </li>
        </ul>
      </section>

      <!-- Inclusion Criteria -->
      <section class="section">
        <h2>Inclusion Criteria</h2>
        <div class="section__input-row">
          <input v-model="newInclusionText" type="text" placeholder="Define an inclusion criterion..." class="input" @keyup.enter="addInclusion" />
          <select v-model="newInclusionPriority" class="select">
            <option value="critical">Critical</option>
            <option value="high">High</option>
            <option value="standard">Standard</option>
            <option value="low">Low</option>
            <option value="optional">Optional</option>
          </select>
          <button class="btn btn--primary" @click="addInclusion">Add</button>
        </div>
        <ul class="section__list">
          <li v-for="c in inclusionCriteria" :key="c.id" class="section__item" :class="priorityClass(c.priority)">
            <span class="section__priority" :class="priorityClass(c.priority)">{{ c.priority }}</span>
            <span>{{ c.text }}</span>
            <button class="btn-icon" @click="deleteCriterion(c.id)">×</button>
          </li>
        </ul>
      </section>

      <!-- Exclusion Criteria -->
      <section class="section">
        <h2>Exclusion Criteria</h2>
        <div class="section__input-row">
          <input v-model="newExclusionText" type="text" placeholder="Define an exclusion criterion..." class="input" @keyup.enter="addExclusion" />
          <select v-model="newExclusionPriority" class="select">
            <option value="critical">Critical</option>
            <option value="high">High</option>
            <option value="standard">Standard</option>
            <option value="low">Low</option>
            <option value="optional">Optional</option>
          </select>
          <button class="btn btn--primary" @click="addExclusion">Add</button>
        </div>
        <ul class="section__list">
          <li v-for="c in exclusionCriteria" :key="c.id" class="section__item" :class="priorityClass(c.priority)">
            <span class="section__priority" :class="priorityClass(c.priority)">{{ c.priority }}</span>
            <span>{{ c.text }}</span>
            <button class="btn-icon" @click="deleteCriterion(c.id)">×</button>
          </li>
        </ul>
      </section>
    </div>
  </div>
</template>

<style scoped>
.criteria-editor {
  padding: var(--space-6);
  max-width: 900px;
}

.criteria-editor h1 {
  font-size: var(--font-size-display);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-display);
  margin-bottom: var(--space-6);
}

.criteria-editor__sections {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.section {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  padding: var(--space-4);
}

.section h2 {
  font-size: var(--font-size-h2);
  margin-bottom: var(--space-3);
}

.section__input-row {
  display: flex;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
}

.input {
  flex: 1;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  outline: none;
}

.input:focus { border-color: var(--color-primary); border-width: 2px; }

.select {
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  outline: none;
  background-color: var(--color-surface);
}

.section__list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.section__item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  border-left: 3px solid var(--color-outline);
  font-size: var(--font-size-caption);
}

.section__item.priority--critical { border-left-color: var(--color-priority-critical); }
.section__item.priority--high { border-left-color: var(--color-priority-high); }
.section__item.priority--standard { border-left-color: var(--color-priority-standard); }
.section__item.priority--low { border-left-color: var(--color-priority-low); }
.section__item.priority--optional { border-left-color: var(--color-priority-optional); }

.section__priority {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
}

.section__priority.priority--critical { color: var(--color-priority-critical); }
.section__priority.priority--high { color: var(--color-priority-high); }
.section__priority.priority--standard { color: var(--color-priority-standard); }
.section__priority.priority--low { color: var(--color-priority-low); }
.section__priority.priority--optional { color: var(--color-priority-optional); }

.btn {
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
}

.btn--primary { background-color: var(--color-primary); color: var(--color-on-primary); }

.btn-icon {
  margin-left: auto;
  width: 24px; height: 24px;
  display: flex; align-items: center; justify-content: center;
  border-radius: 50%;
  font-size: 14px;
  color: var(--color-on-surface-variant);
  cursor: pointer;
}

.btn-icon:hover { background-color: var(--color-surface-container-high); }
</style>
```

- [ ] **Step 2: Update router**

In `src/router/index.ts`, add:

```typescript
const CriteriaEditor = () => import('@/views/criteria-editor.vue');
```

Change criteria route:

```typescript
{ path: '/criteria', name: 'criteria', component: CriteriaEditor },
```

- [ ] **Step 3: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/views/criteria-editor.vue src/router/index.ts
git commit -m "feat(criteria): add three-section criteria editor UI"
```

---

## Task 6: LLM Configuration UI

**Files:**
- Create: `src/composables/use-llm-config.ts`
- Create: `src/views/llm-config.vue`
- Modify: `src/router/index.ts`

- [ ] **Step 1: Create `src/composables/use-llm-config.ts`**

```typescript
import { ref, onMounted } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { LlmConfig } from '@/types';

const DEFAULT_CONFIG: LlmConfig = {
  provider: 'openai',
  endpointUrl: '',
  apiKeyEncrypted: null,
  modelName: '',
  temperature: 0.2,
  maxConcurrentRequests: 3,
  requestDelayMs: 500,
  contextWindowTokens: 50000,
};

export function useLlmConfig() {
  const config = ref<LlmConfig>({ ...DEFAULT_CONFIG });
  const loading = ref(false);
  const saving = ref(false);
  const testing = ref(false);
  const testResult = ref<{ success: boolean; message: string } | null>(null);
  const isLocal = ref(false);

  onMounted(loadConfig);

  async function loadConfig(): Promise<void> {
    loading.value = true;
    try {
      const saved = await tauriCommand<LlmConfig | null>('get_llm_config');
      if (saved) config.value = saved;
      checkLocal();
    } finally {
      loading.value = false;
    }
  }

  async function save(): Promise<void> {
    saving.value = true;
    try {
      await tauriCommand('save_llm_config', { config: config.value });
    } finally {
      saving.value = false;
    }
  }

  async function testConnection(): Promise<void> {
    testing.value = true;
    testResult.value = null;
    try {
      await save();
      testResult.value = await tauriCommand<{ success: boolean; message: string }>('test_llm_connection');
    } catch (e) {
      testResult.value = { success: false, message: e instanceof Error ? e.message : String(e) };
    } finally {
      testing.value = false;
    }
  }

  function checkLocal(): void {
    isLocal.value = ['llama_cpp', 'ollama', 'lm_studio'].includes(config.value.provider);
  }

  return { config, loading, saving, testing, testResult, isLocal, loadConfig, save, testConnection, checkLocal };
}
```

- [ ] **Step 2: Create `src/views/llm-config.vue`**

```vue
<script setup lang="ts">
import { useLlmConfig } from '@/composables/use-llm-config';

const { config, saving, testing, testResult, isLocal, save, testConnection, checkLocal } = useLlmConfig();
</script>

<template>
  <div class="llm-config">
    <h1>LLM Configuration</h1>

    <div v-if="isLocal" class="llm-config__warning">
      Local LLM providers typically require 16 GB or more of VRAM for models supporting 50K+ token context windows. Performance may be limited on systems with less VRAM.
    </div>

    <form class="llm-config__form" @submit.prevent="save">
      <div class="field">
        <label class="field__label">Provider</label>
        <select v-model="config.provider" class="input" @change="checkLocal">
          <option value="openai">OpenAI</option>
          <option value="google">Google</option>
          <option value="z_ai">z.ai</option>
          <option value="llama_cpp">llama.cpp</option>
          <option value="ollama">Ollama</option>
          <option value="lm_studio">LM Studio</option>
          <option value="custom">Custom</option>
        </select>
      </div>

      <div class="field">
        <label class="field__label">Endpoint URL</label>
        <input v-model="config.endpointUrl" type="url" class="input" placeholder="https://api.openai.com/v1/chat/completions" />
      </div>

      <div class="field">
        <label class="field__label">Model Name</label>
        <input v-model="config.modelName" type="text" class="input" placeholder="gpt-4o" />
      </div>

      <div class="field">
        <label class="field__label">API Key</label>
        <input v-model="config.apiKeyEncrypted" type="password" class="input" placeholder="sk-..." />
      </div>

      <div class="field">
        <label class="field__label">Temperature ({{ config.temperature }})</label>
        <input v-model.number="config.temperature" type="range" min="0" max="1" step="0.1" />
      </div>

      <div class="field-row">
        <div class="field">
          <label class="field__label">Concurrent Requests</label>
          <input v-model.number="config.maxConcurrentRequests" type="number" min="1" max="10" class="input" />
        </div>
        <div class="field">
          <label class="field__label">Request Delay (ms)</label>
          <input v-model.number="config.requestDelayMs" type="number" min="0" max="5000" step="100" class="input" />
        </div>
        <div class="field">
          <label class="field__label">Context Window (tokens)</label>
          <input v-model.number="config.contextWindowTokens" type="number" min="1000" class="input" />
        </div>
      </div>

      <div class="llm-config__actions">
        <button type="submit" class="btn btn--primary" :disabled="saving">
          {{ saving ? 'Saving...' : 'Save Configuration' }}
        </button>
        <button type="button" class="btn btn--secondary" :disabled="testing" @click="testConnection">
          {{ testing ? 'Testing...' : 'Test Connection' }}
        </button>
      </div>
    </form>

    <div v-if="testResult" class="llm-config__test-result" :class="{ 'llm-config__test-result--success': testResult.success }">
      {{ testResult.message }}
    </div>
  </div>
</template>

<style scoped>
.llm-config {
  padding: var(--space-6);
  max-width: 700px;
}

.llm-config h1 {
  font-size: var(--font-size-display);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-display);
  margin-bottom: var(--space-6);
}

.llm-config__warning {
  padding: var(--space-3);
  background-color: var(--color-surface-container);
  border: 1px solid var(--color-priority-high);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  color: var(--color-priority-high);
  margin-bottom: var(--space-4);
}

.llm-config__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.field__label {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
  color: var(--color-on-surface-variant);
}

.input {
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  outline: none;
  background-color: var(--color-surface);
}

.input:focus { border-color: var(--color-primary); border-width: 2px; }

.field-row {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: var(--space-3);
}

.llm-config__actions {
  display: flex;
  gap: var(--space-3);
  margin-top: var(--space-4);
}

.btn {
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
}

.btn--primary { background-color: var(--color-primary); color: var(--color-on-primary); }
.btn--secondary { background-color: var(--color-surface-container-high); color: var(--color-on-surface); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }

.llm-config__test-result {
  margin-top: var(--space-3);
  padding: var(--space-3);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
}

.llm-config__test-result--success {
  background-color: #dcfce7;
  color: #166534;
}
</style>
```

- [ ] **Step 3: Update router**

In `src/router/index.ts`, add:

```typescript
const LlmConfigView = () => import('@/views/llm-config.vue');
```

Change settings route:

```typescript
{ path: '/settings', name: 'settings', component: LlmConfigView },
```

- [ ] **Step 4: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/composables/use-llm-config.ts src/views/llm-config.vue src/router/index.ts
git commit -m "feat(llm): add LLM configuration form with provider selection and test connection"
```

---

## Task 7: Final Verification

- [ ] **Step 1: Run `npm run check:all`**

Run: `npm run check:all`
Expected: PASS

- [ ] **Step 2: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 3: Verify UI flows**

Run: `cd src-tauri && cargo tauri dev`
Expected: Criteria editor shows three sections (Aims, Inclusion, Exclusion). LLM config form shows provider dropdown and test connection button.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "chore: fix any issues from criteria and LLM config implementation"
```
