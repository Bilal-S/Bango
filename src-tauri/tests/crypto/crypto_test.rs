use bango_lib::crypto::aes_gcm::{decrypt, derive_key_from_machine, encrypt};

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
    assert_ne!(encrypted, plaintext);
}

#[test]
fn test_derive_key_deterministic() {
    let key_a = derive_key_from_machine();
    let key_b = derive_key_from_machine();
    assert_eq!(key_a, key_b);
}
