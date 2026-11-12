use crate::domain::errors::HttpErrorDetails;
use reqwest::Request;
use reqwest::header::HeaderMap;

pub fn extract_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    extract_headers_redacted(headers, &[])
}

pub fn extract_headers_redacted(
    headers: &HeaderMap,
    redacted_keys: &[&str],
) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, val)| {
            let name_str = name.as_str().to_string();
            let val_str = if redacted_keys
                .iter()
                .any(|k| k.eq_ignore_ascii_case(&name_str))
            {
                "[REDACTED]".to_string()
            } else {
                val.to_str().unwrap_or("[non-utf8 header]").to_string()
            };
            (name_str, val_str)
        })
        .collect()
}

pub fn extract_request_body(request: &Request) -> String {
    request
        .body()
        .and_then(|b| b.as_bytes())
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_default()
}

pub struct HttpRequestSnapshot {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpRequestSnapshot {
    pub fn from_request(request: &Request, redacted_headers: &[&str]) -> Self {
        Self {
            url: request.url().to_string(),
            method: request.method().to_string(),
            headers: extract_headers_redacted(request.headers(), redacted_headers),
            body: extract_request_body(request),
        }
    }

    pub fn to_network_error_details(&self, message: impl Into<String>) -> HttpErrorDetails {
        HttpErrorDetails {
            url: self.url.clone(),
            method: self.method.clone(),
            request_headers: self.headers.clone(),
            request_body: self.body.clone(),
            status_code: 0,
            response_headers: Vec::new(),
            response_body: String::new(),
            message: message.into(),
        }
    }

    pub fn to_http_error_details(
        self,
        status_code: u16,
        response_headers: Vec<(String, String)>,
        response_body: String,
        message: impl Into<String>,
    ) -> HttpErrorDetails {
        HttpErrorDetails {
            url: self.url,
            method: self.method,
            request_headers: self.headers,
            request_body: self.body,
            status_code,
            response_headers,
            response_body,
            message: message.into(),
        }
    }
}
