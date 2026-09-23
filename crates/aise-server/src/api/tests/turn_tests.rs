use super::*;
use axum::http::HeaderValue;

#[test]
fn missing_idempotency_key_is_distinguished_from_invalid_header() {
    let headers = HeaderMap::new();
    assert_eq!(idempotency_key_header(&headers).unwrap(), None);

    let mut headers = HeaderMap::new();
    headers.insert("Idempotency-Key", HeaderValue::from_static("key"));
    assert_eq!(idempotency_key_header(&headers).unwrap().as_deref(), Some("key"));

    let mut headers = HeaderMap::new();
    headers.insert("Idempotency-Key", HeaderValue::from_bytes(&[0xff]).unwrap());
    assert!(idempotency_key_header(&headers).is_err());
}
