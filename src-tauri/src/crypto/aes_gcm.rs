use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::Nonce;
use aes_gcm::{Aes256Gcm, Error as AesGcmError};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

const SALT: &[u8; 16] = b"bango-app-salt16";
const ITERATIONS: u32 = 600_000;

/// Derives a 256-bit key from machine identity (hostname + username + app salt).
#[must_use]
pub fn derive_key_from_machine() -> [u8; 32] {
    let hostname = get_hostname();
    let username = get_username();
    let identity = format!("{hostname}:{username}");
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
pub fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Result<String, AesGcmError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| AesGcmError)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext)?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

/// Decrypts base64-encoded nonce+ciphertext using AES-256-GCM.
pub fn decrypt(encoded: &str, key: &[u8; 32]) -> Result<Vec<u8>, AesGcmError> {
    let combined = BASE64.decode(encoded).map_err(|_| AesGcmError)?;
    if combined.len() < 12 {
        return Err(AesGcmError);
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| AesGcmError)?;
    cipher.decrypt(nonce, ciphertext)
}

fn get_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown-host".to_string())
}

fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown-user".to_string())
}
