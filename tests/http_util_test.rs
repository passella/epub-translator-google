use epub_translator_google::adapters::translator::http_util::{
    HttpRequestSnapshot, extract_headers, extract_headers_redacted, extract_request_body,
};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

#[test]
fn test_extract_headers_when_empty_headers_returns_empty_vec() {
    let headers = HeaderMap::new();
    let extracted = extract_headers(&headers);
    assert!(extracted.is_empty());
}

#[test]
fn test_extract_headers_redacted_when_non_utf8_value_returns_fallback() {
    let mut headers = HeaderMap::new();
    let non_utf8_val = HeaderValue::from_bytes(&[0xFF, 0xFE, 0xFD]).unwrap();
    headers.insert(HeaderName::from_static("x-raw-bytes"), non_utf8_val);

    let extracted = extract_headers(&headers);
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].0, "x-raw-bytes");
    assert_eq!(extracted[0].1, "[non-utf8 header]");
}

#[test]
fn test_extract_headers_redacted_case_insensitive_matching() {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-goog-api-key"),
        HeaderValue::from_static("secret-key-123"),
    );
    headers.insert(
        HeaderName::from_static("authorization"),
        HeaderValue::from_static("Bearer token-456"),
    );
    headers.insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("application/json"),
    );

    let extracted = extract_headers_redacted(&headers, &["X-GOOG-API-KEY", "authorization"]);

    let mut found_api_key = false;
    let mut found_auth = false;
    let mut found_content_type = false;

    for (name, val) in extracted {
        if name.eq_ignore_ascii_case("x-goog-api-key") {
            assert_eq!(val, "[REDACTED]");
            found_api_key = true;
        } else if name.eq_ignore_ascii_case("authorization") {
            assert_eq!(val, "[REDACTED]");
            found_auth = true;
        } else if name.eq_ignore_ascii_case("content-type") {
            assert_eq!(val, "application/json");
            found_content_type = true;
        }
    }

    assert!(found_api_key);
    assert!(found_auth);
    assert!(found_content_type);
}

#[test]
fn test_extract_request_body_empty_and_non_empty() {
    let client = Client::new();

    let request_no_body = client.get("https://example.com/test").build().unwrap();
    assert_eq!(extract_request_body(&request_no_body), "");

    let request_with_body = client
        .post("https://example.com/api")
        .body("hello request body")
        .build()
        .unwrap();
    assert_eq!(
        extract_request_body(&request_with_body),
        "hello request body"
    );
}

#[test]
fn test_http_request_snapshot_to_network_error_details() {
    let client = Client::new();
    let request = client
        .post("https://example.com/network-test")
        .header("x-secret", "sensitive-123")
        .header("accept", "application/json")
        .body("sample payload")
        .build()
        .unwrap();

    let snapshot = HttpRequestSnapshot::from_request(&request, &["x-secret"]);
    let network_err = snapshot.to_network_error_details("Connection timed out");

    assert_eq!(network_err.url, "https://example.com/network-test");
    assert_eq!(network_err.method, "POST");
    assert_eq!(network_err.request_body, "sample payload");
    assert_eq!(network_err.status_code, 0);
    assert!(network_err.response_headers.is_empty());
    assert!(network_err.response_body.is_empty());
    assert_eq!(network_err.message, "Connection timed out");

    let secret_header = network_err
        .request_headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("x-secret"))
        .map(|(_, v)| v.as_str());
    assert_eq!(secret_header, Some("[REDACTED]"));
}

#[test]
fn test_http_request_snapshot_to_http_error_details() {
    let client = Client::new();
    let request = client
        .post("https://example.com/error-test")
        .header("content-type", "application/json")
        .body("{\"query\":\"value\"}")
        .build()
        .unwrap();

    let snapshot = HttpRequestSnapshot::from_request(&request, &[]);
    let response_headers = vec![("server".to_string(), "nginx".to_string())];
    let response_body = "{\"error\":\"Forbidden\"}".to_string();

    let http_err = snapshot.to_http_error_details(
        403,
        response_headers.clone(),
        response_body.clone(),
        "HTTP 403 Forbidden",
    );

    assert_eq!(http_err.url, "https://example.com/error-test");
    assert_eq!(http_err.method, "POST");
    assert_eq!(http_err.request_body, "{\"query\":\"value\"}");
    assert_eq!(http_err.status_code, 403);
    assert_eq!(http_err.response_headers, response_headers);
    assert_eq!(http_err.response_body, response_body);
    assert_eq!(http_err.message, "HTTP 403 Forbidden");
}
