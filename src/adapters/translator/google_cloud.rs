use crate::adapters::translator::http_util::{HttpRequestSnapshot, extract_headers};
use crate::domain::entities::{Language, TranslationResult};
use crate::domain::errors::TranslationError;
use crate::domain::ports::TranslatorPort;
use async_trait::async_trait;
use reqwest::Client;
use reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use zeroize::Zeroizing;

#[derive(Serialize)]
struct GoogleCloudRequestBody<'a> {
    q: &'a [String],
    target: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'a str>,
    format: &'a str,
}

#[derive(Deserialize)]
struct GoogleCloudTranslationItem {
    #[serde(rename = "translatedText")]
    translated_text: String,
    #[serde(rename = "detectedSourceLanguage")]
    detected_source_language: Option<String>,
}

#[derive(Deserialize)]
struct GoogleCloudData {
    translations: Vec<GoogleCloudTranslationItem>,
}

#[derive(Deserialize)]
struct GoogleCloudSuccessResponse {
    data: GoogleCloudData,
}

#[derive(Deserialize)]
struct GoogleCloudErrorDetail {
    message: String,
}

#[derive(Deserialize)]
struct GoogleCloudErrorResponse {
    error: GoogleCloudErrorDetail,
}

pub struct GoogleCloudTranslatorAdapter {
    client: Client,
    api_key: Zeroizing<String>,
}

impl GoogleCloudTranslatorAdapter {
    fn read_non_empty_env(var_name: &str) -> Option<Zeroizing<String>> {
        std::env::var(var_name)
            .ok()
            .map(Zeroizing::new)
            .map(|v| Zeroizing::new(v.trim().to_string()))
            .filter(|v| !v.is_empty())
    }

    pub fn new(api_key: impl Into<String>) -> Result<Self, TranslationError> {
        let key = Zeroizing::new(api_key.into());
        let trimmed = key.trim();
        let final_key = if trimmed.is_empty() {
            Self::read_non_empty_env("GOOGLE_CLOUD_TRANSLATION_API_KEY")
                .or_else(|| Self::read_non_empty_env("GOOGLE_TRANSLATE_API_KEY"))
                .or_else(|| Self::read_non_empty_env("GOOGLE_API_KEY"))
                .ok_or_else(|| {
                    TranslationError::Authentication(
                        "Google Cloud Translation API key not provided and not found in environment variables GOOGLE_CLOUD_TRANSLATION_API_KEY, GOOGLE_TRANSLATE_API_KEY or GOOGLE_API_KEY".to_string(),
                    )
                })?
        } else {
            Zeroizing::new(trimmed.to_string())
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|e| TranslationError::Http(format!("Failed to build reqwest client: {e}")))?;

        Ok(Self {
            client,
            api_key: final_key,
        })
    }
}

#[async_trait]
impl TranslatorPort for GoogleCloudTranslatorAdapter {
    async fn translate_html(
        &self,
        html: &str,
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        let batch = vec![html.to_string()];
        let mut results = self
            .translate_batch(&batch, source_lang, target_lang)
            .await?;
        results.pop().ok_or_else(|| {
            TranslationError::Provider("Empty translation response from Google Cloud".to_string())
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let source_opt = if source_lang.is_auto() {
            None
        } else {
            Some(source_lang.as_str())
        };

        let request_body = GoogleCloudRequestBody {
            q: texts,
            target: target_lang.as_str(),
            source: source_opt,
            format: "html",
        };

        let url = "https://translation.googleapis.com/language/translate/v2";

        let mut api_key_header = HeaderValue::from_str(&self.api_key).map_err(|e| {
            TranslationError::Authentication(format!("Invalid API key header value: {e}"))
        })?;
        api_key_header.set_sensitive(true);

        let request = self
            .client
            .post(url)
            .header("X-goog-api-key", api_key_header)
            .json(&request_body)
            .build()
            .map_err(|e| TranslationError::Http(format!("Failed to build HTTP request: {e}")))?;

        let snapshot = HttpRequestSnapshot::from_request(&request, &["x-goog-api-key"]);

        let response = self.client.execute(request).await.map_err(|e| {
            TranslationError::HttpDetailed(Box::new(
                snapshot
                    .to_network_error_details(format!("Google Cloud HTTP network failure: {e}")),
            ))
        })?;

        let status = response.status();
        let status_code = status.as_u16();
        let response_headers = extract_headers(response.headers());

        let bytes = response
            .bytes()
            .await
            .map_err(|e| TranslationError::Http(format!("Failed to read response bytes: {e}")))?;

        if !status.is_success() {
            let raw_text = String::from_utf8_lossy(&bytes).to_string();
            let parsed_message = serde_json::from_slice::<GoogleCloudErrorResponse>(&bytes)
                .map(|err_resp| err_resp.error.message)
                .unwrap_or_else(|_| raw_text.clone());

            let error_details = Box::new(snapshot.to_http_error_details(
                status_code,
                response_headers,
                raw_text,
                format!("Google Cloud API error {status}: {parsed_message}"),
            ));

            if status_code == 429 {
                return Err(TranslationError::RateLimited(format!(
                    "Google Cloud endpoint returned HTTP 429: {parsed_message}"
                )));
            }

            return Err(TranslationError::HttpDetailed(error_details));
        }

        let parsed: GoogleCloudSuccessResponse = serde_json::from_slice(&bytes).map_err(|e| {
            TranslationError::Serialization(format!(
                "Failed to deserialize Google Cloud response: {e}"
            ))
        })?;

        let results = parsed
            .data
            .translations
            .into_iter()
            .map(|item| TranslationResult {
                translated_text: item.translated_text,
                detected_source: item.detected_source_language.map(Language::new),
            })
            .collect();

        Ok(results)
    }

    fn provider_name(&self) -> &'static str {
        "google-cloud"
    }
}
