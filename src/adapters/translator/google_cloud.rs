use crate::domain::entities::{Language, TranslationResult};
use crate::domain::errors::{HttpErrorDetails, TranslationError};
use crate::domain::ports::TranslatorPort;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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
    api_key: String,
}

impl GoogleCloudTranslatorAdapter {
    pub fn new(api_key: impl Into<String>) -> Result<Self, TranslationError> {
        let key = api_key.into();
        let final_key = if key.is_empty() {
            std::env::var("GOOGLE_CLOUD_TRANSLATION_API_KEY")
                .or_else(|_| std::env::var("GOOGLE_API_KEY"))
                .map_err(|_| {
                    TranslationError::Authentication(
                        "Google Cloud Translation API key not provided and not found in environment variables GOOGLE_CLOUD_TRANSLATION_API_KEY or GOOGLE_API_KEY".to_string(),
                    )
                })?
        } else {
            key
        };

        Ok(Self {
            client: Client::builder().build().map_err(|e| {
                TranslationError::Http(format!("Failed to build reqwest client: {e}"))
            })?,
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

        let request = self
            .client
            .post(url)
            .header("X-goog-api-key", &self.api_key)
            .json(&request_body)
            .build()
            .map_err(|e| TranslationError::Http(format!("Failed to build HTTP request: {e}")))?;

        let request_method = request.method().to_string();
        let request_url = url.to_string();
        let request_headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|(name, val)| {
                let name_str = name.as_str().to_string();
                let val_str = if name_str.eq_ignore_ascii_case("x-goog-api-key") {
                    "[REDACTED]".to_string()
                } else {
                    val.to_str().unwrap_or("[non-utf8 header]").to_string()
                };
                (name_str, val_str)
            })
            .collect();
        let request_body_str = request
            .body()
            .and_then(|b| b.as_bytes())
            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
            .unwrap_or_default();

        let response = self.client.execute(request).await.map_err(|e| {
            TranslationError::HttpDetailed(Box::new(HttpErrorDetails {
                url: request_url.clone(),
                method: request_method.clone(),
                request_headers: request_headers.clone(),
                request_body: request_body_str.clone(),
                status_code: 0,
                response_headers: Vec::new(),
                response_body: String::new(),
                message: format!("Google Cloud HTTP network failure: {e}"),
            }))
        })?;

        let status = response.status();
        let status_code = status.as_u16();

        let response_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(name, val)| {
                (
                    name.as_str().to_string(),
                    val.to_str().unwrap_or("[non-utf8 header]").to_string(),
                )
            })
            .collect();

        let bytes = response
            .bytes()
            .await
            .map_err(|e| TranslationError::Http(format!("Failed to read response bytes: {e}")))?;

        if !status.is_success() {
            let raw_text = String::from_utf8_lossy(&bytes).to_string();
            let parsed_message = serde_json::from_slice::<GoogleCloudErrorResponse>(&bytes)
                .map(|err_resp| err_resp.error.message)
                .unwrap_or_else(|_| raw_text.clone());

            let error_details = Box::new(HttpErrorDetails {
                url: request_url,
                method: request_method,
                request_headers,
                request_body: request_body_str,
                status_code,
                response_headers,
                response_body: raw_text,
                message: format!("Google Cloud API error {status}: {parsed_message}"),
            });

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
