use async_trait::async_trait;
use epub_translator_google::adapters::translator::translate_batch_sequential;
use epub_translator_google::domain::entities::{Language, TranslationResult};
use epub_translator_google::domain::errors::TranslationError;
use epub_translator_google::domain::ports::TranslatorPort;

struct MockTranslator {
    fail_on: Option<String>,
}

#[async_trait]
impl TranslatorPort for MockTranslator {
    async fn translate_html(
        &self,
        html: &str,
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        if self.fail_on.as_deref() == Some(html) {
            return Err(TranslationError::Process(format!("Failed on {html}")));
        }
        Ok(TranslationResult {
            translated_text: format!("{html}-{}-{}", source_lang.as_str(), target_lang.as_str()),
            detected_source: Some(source_lang.clone()),
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        translate_batch_sequential(self, texts, source_lang, target_lang).await
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

#[tokio::test]
async fn test_translate_batch_sequential_when_empty_slice_returns_empty_vec() {
    let translator = MockTranslator { fail_on: None };
    let source = Language::new("en");
    let target = Language::new("pt");

    let results = translate_batch_sequential(&translator, &[], &source, &target)
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[tokio::test]
async fn test_translate_batch_sequential_when_translator_fails_propagates_error() {
    let translator = MockTranslator {
        fail_on: Some("error_chunk".to_string()),
    };
    let source = Language::new("en");
    let target = Language::new("pt");
    let texts = vec![
        "ok_chunk_1".to_string(),
        "error_chunk".to_string(),
        "ok_chunk_2".to_string(),
    ];

    let result = translate_batch_sequential(&translator, &texts, &source, &target).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        TranslationError::Process(msg) => {
            assert!(msg.contains("Failed on error_chunk"));
        }
        other => panic!("Unexpected error variant: {other:?}"),
    }
}

#[tokio::test]
async fn test_translate_batch_sequential_single_and_multiple_elements_in_order() {
    let translator = MockTranslator { fail_on: None };
    let source = Language::new("en");
    let target = Language::new("pt");

    let single = vec!["one".to_string()];
    let res_single = translate_batch_sequential(&translator, &single, &source, &target)
        .await
        .unwrap();
    assert_eq!(res_single.len(), 1);
    assert_eq!(res_single[0].translated_text, "one-en-pt");

    let multiple = vec![
        "first".to_string(),
        "second".to_string(),
        "third".to_string(),
    ];
    let res_multiple = translate_batch_sequential(&translator, &multiple, &source, &target)
        .await
        .unwrap();
    assert_eq!(res_multiple.len(), 3);
    assert_eq!(res_multiple[0].translated_text, "first-en-pt");
    assert_eq!(res_multiple[1].translated_text, "second-en-pt");
    assert_eq!(res_multiple[2].translated_text, "third-en-pt");
}
