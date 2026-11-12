use epub_translator_google::adapters::translator::GoogleCloudTranslatorAdapter;
use epub_translator_google::domain::entities::Language;
use epub_translator_google::domain::errors::TranslationError;
use epub_translator_google::domain::ports::TranslatorPort;

#[tokio::test]
async fn test_google_cloud_empty_key_without_env_returns_error() {
    unsafe {
        std::env::remove_var("GOOGLE_CLOUD_TRANSLATION_API_KEY");
        std::env::remove_var("GOOGLE_API_KEY");
    }

    let adapter = GoogleCloudTranslatorAdapter::new("");
    assert!(adapter.is_err());
    match adapter.err().unwrap() {
        TranslationError::Authentication(msg) => {
            assert!(msg.contains("API key not provided"));
        }
        other => panic!("Unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn test_google_cloud_constructor_with_valid_key() {
    let adapter = GoogleCloudTranslatorAdapter::new("test-secret-key-123").unwrap();
    assert_eq!(adapter.provider_name(), "google-cloud");
}

#[tokio::test]
async fn test_google_cloud_translate_batch_empty_slice_returns_empty_vec() {
    let adapter = GoogleCloudTranslatorAdapter::new("test-key").unwrap();
    let source = Language::new("en");
    let target = Language::new("pt-BR");
    let results = adapter
        .translate_batch(&[], &source, &target)
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_google_cloud_error_details_redacts_api_key_and_url_has_no_query_param() {
    let adapter = GoogleCloudTranslatorAdapter::new("super-secret-api-key-999").unwrap();
    let source = Language::new("en");
    let target = Language::new("pt-BR");
    let texts = vec!["Hello world".to_string()];

    let err = adapter
        .translate_batch(&texts, &source, &target)
        .await
        .err();
    assert!(err.is_some());

    match err.unwrap() {
        TranslationError::HttpDetailed(details) => {
            assert_eq!(
                details.url,
                "https://translation.googleapis.com/language/translate/v2"
            );
            assert!(!details.url.contains("super-secret-api-key-999"));
            assert!(!details.url.contains("?key="));

            for (header_name, header_val) in &details.request_headers {
                if header_name.eq_ignore_ascii_case("x-goog-api-key") {
                    assert_eq!(header_val, "[REDACTED]");
                }
                assert!(!header_val.contains("super-secret-api-key-999"));
            }
        }
        TranslationError::Http(msg) => {
            assert!(!msg.contains("super-secret-api-key-999"));
        }
        other => panic!("Unexpected error type: {other:?}"),
    }
}
