//! Coverage for the AppError enum + its Serialize impl.
use bango_lib::error::AppError;

#[test]
fn database_error_from_rusqlite() {
    let err = AppError::Database(rusqlite::Error::InvalidParameterName("x".to_string()));
    let s = err.to_string();
    assert!(s.contains("Database error"));
}

#[test]
fn validation_error_message() {
    let err = AppError::Validation("bad input".to_string());
    assert_eq!(err.to_string(), "Validation error: bad input");
}

#[test]
fn not_found_error_message() {
    let err = AppError::NotFound("missing".to_string());
    assert_eq!(err.to_string(), "Not found: missing");
}

#[test]
fn import_error_message() {
    let err = AppError::Import("parse failed".to_string());
    assert_eq!(err.to_string(), "Import error: parse failed");
}

#[test]
fn rendering_error_message() {
    let err = AppError::Rendering("svg bad".to_string());
    assert_eq!(err.to_string(), "Rendering error: svg bad");
}

#[test]
fn scraping_error_message() {
    let err = AppError::Scraping("chrome died".to_string());
    assert_eq!(err.to_string(), "Scraping error: chrome died");
}

#[test]
fn serialization_error_from_serde_json() {
    let bad: Result<serde_json::Value, _> = serde_json::from_str("{ invalid");
    let serde_err = bad.expect_err("should be parse error");
    let err: AppError = serde_err.into();
    assert!(err.to_string().contains("Serialization error"));
}

#[test]
fn io_error_from_std_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
    let err: AppError = io_err.into();
    assert!(err.to_string().contains("IO error"));
}

#[test]
fn serialize_produces_string() {
    // AppError serializes to its to_string() value (used by Tauri InvokeError).
    let err = AppError::Validation("oops".to_string());
    let json = serde_json::to_string(&err).expect("serialize");
    // The serialized form is a JSON string literal wrapping the message.
    assert!(json.contains("Validation error: oops"));
}
