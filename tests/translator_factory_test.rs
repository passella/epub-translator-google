use async_trait::async_trait;
use epub_translator_google::adapters::translator::{
    ProviderMetadata, ProviderOptionResolver, ProviderOptions, ProviderRegistration,
    ProviderRegistry, TranslatorFactory,
};
use epub_translator_google::domain::entities::{
    Language, ProviderConfig, TranslationProviderKind, TranslationResult,
};
use epub_translator_google::domain::errors::{AppError, TranslationError};
use epub_translator_google::domain::ports::TranslatorPort;
use std::sync::Arc;

struct MockCustomTranslator;

#[async_trait]
impl TranslatorPort for MockCustomTranslator {
    async fn translate_html(
        &self,
        _html: &str,
        _source_lang: &Language,
        _target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        Ok(TranslationResult {
            translated_text: "custom translation".to_string(),
            detected_source: None,
        })
    }

    async fn translate_batch(
        &self,
        _texts: &[String],
        _source_lang: &Language,
        _target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        Ok(Vec::new())
    }

    fn provider_name(&self) -> &'static str {
        "custom-test-provider"
    }
}

struct MockOptionResolver;

impl ProviderOptionResolver for MockOptionResolver {
    fn resolve(&self, _options: &ProviderOptions) -> Result<ProviderConfig, AppError> {
        Ok(ProviderConfig::GoogleFree)
    }
}

#[test]
fn test_provider_registry_when_kind_not_registered_returns_error() {
    let registry = ProviderRegistry::new();
    let result = registry.get(TranslationProviderKind::Antigravity);
    assert!(result.is_err());
}

#[test]
fn test_provider_registry_create_translator_when_kind_not_registered_returns_error() {
    let registry = ProviderRegistry::new();
    let config = ProviderConfig::antigravity(None);
    let result = registry.create_translator(&config);
    assert!(result.is_err());
}

#[test]
fn test_provider_registry_resolve_config_when_kind_not_registered_returns_error() {
    let registry = ProviderRegistry::new();
    let options = ProviderOptions::default();
    let result = registry.resolve_config(TranslationProviderKind::Antigravity, &options);
    assert!(result.is_err());
}

#[test]
fn test_provider_registry_custom_registration_and_creation() {
    let registry = ProviderRegistry::new();
    let metadata = ProviderMetadata::new(
        TranslationProviderKind::GoogleFree,
        "Custom Name",
        "Custom Description",
        false,
    );
    let registration = ProviderRegistration::new(
        metadata.clone(),
        Arc::new(MockOptionResolver),
        Arc::new(|_config| Ok(Arc::new(MockCustomTranslator))),
    );

    registry.register(registration).unwrap();

    let fetched = registry.get(TranslationProviderKind::GoogleFree).unwrap();
    assert_eq!(fetched.metadata.name, "Custom Name");
    assert_eq!(fetched.metadata.description, "Custom Description");
    assert!(!fetched.metadata.requires_api_key);

    let config = ProviderConfig::google_free();
    let adapter = registry.create_translator(&config).unwrap();
    assert_eq!(adapter.provider_name(), "custom-test-provider");

    let kinds = registry.registered_kinds().unwrap();
    assert_eq!(kinds, vec![TranslationProviderKind::GoogleFree]);

    let all_meta = registry.all_metadata().unwrap();
    assert_eq!(all_meta.len(), 1);
    assert_eq!(all_meta[0], metadata);
}

#[test]
fn test_provider_registry_global_contains_defaults() {
    let registry = ProviderRegistry::global();
    let kinds = registry.registered_kinds().unwrap();
    assert!(kinds.contains(&TranslationProviderKind::Antigravity));
    assert!(kinds.contains(&TranslationProviderKind::GoogleFree));
    assert!(kinds.contains(&TranslationProviderKind::GoogleCloud));

    let all_meta = registry.all_metadata().unwrap();
    assert_eq!(all_meta.len(), 3);
}

#[test]
fn test_translator_factory_create_antigravity() {
    let config = ProviderConfig::antigravity(Some("gemini-4-argon-medium".to_string()));
    let adapter = TranslatorFactory::create(&config).unwrap();
    assert_eq!(adapter.provider_name(), "antigravity");
    assert_eq!(adapter.model_name(), Some("gemini-4-argon-medium"));
}

#[test]
fn test_translator_factory_create_antigravity_default_model() {
    let config = ProviderConfig::antigravity(None);
    let adapter = TranslatorFactory::create(&config).unwrap();
    assert_eq!(adapter.provider_name(), "antigravity");
    assert_eq!(adapter.model_name(), None);
}

#[test]
fn test_translator_factory_create_google_free() {
    let config = ProviderConfig::google_free();
    let adapter = TranslatorFactory::create(&config).unwrap();
    assert_eq!(adapter.provider_name(), "google-free");
}

#[test]
fn test_translator_factory_create_google_cloud() {
    let config = ProviderConfig::google_cloud("dummy_api_key");
    let adapter = TranslatorFactory::create(&config).unwrap();
    assert_eq!(adapter.provider_name(), "google-cloud");
}

#[test]
fn test_translator_factory_create_from_parts() {
    let adapter = TranslatorFactory::create_from_parts(
        TranslationProviderKind::Antigravity,
        Some("gemini-4-argon-medium".to_string()),
        None,
    )
    .unwrap();
    assert_eq!(adapter.provider_name(), "antigravity");
    assert_eq!(adapter.model_name(), Some("gemini-4-argon-medium"));
}
