use crate::adapters::cli::args::CliArgs;
use crate::adapters::cli::provider_resolver::{
    AntigravityOptionResolver, GoogleCloudOptionResolver, GoogleFreeOptionResolver,
    ProviderOptionResolver,
};
use crate::adapters::translator::{
    AntigravityTranslatorAdapter, GoogleCloudTranslatorAdapter, GoogleFreeTranslatorAdapter,
};
use crate::domain::entities::{ProviderConfig, TranslationProviderKind};
use crate::domain::errors::AppError;
use crate::domain::ports::{TranslationCachePort, TranslatorPort};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

pub type TranslatorConstructor =
    Arc<dyn Fn(&ProviderConfig) -> Result<Arc<dyn TranslatorPort>, AppError> + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub kind: TranslationProviderKind,
    pub name: String,
    pub description: String,
    pub requires_api_key: bool,
}

impl ProviderMetadata {
    pub fn new(
        kind: TranslationProviderKind,
        name: impl Into<String>,
        description: impl Into<String>,
        requires_api_key: bool,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            description: description.into(),
            requires_api_key,
        }
    }
}

#[derive(Clone)]
pub struct ProviderRegistration {
    pub metadata: ProviderMetadata,
    pub option_resolver: Arc<dyn ProviderOptionResolver>,
    pub factory: TranslatorConstructor,
}

impl ProviderRegistration {
    pub fn new(
        metadata: ProviderMetadata,
        option_resolver: Arc<dyn ProviderOptionResolver>,
        factory: TranslatorConstructor,
    ) -> Self {
        Self {
            metadata,
            option_resolver,
            factory,
        }
    }
}

pub struct ProviderRegistry {
    entries: RwLock<HashMap<TranslationProviderKind, ProviderRegistration>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new_default()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn new_default() -> Self {
        let registry = Self::new();
        registry.register_defaults();
        registry
    }

    pub fn global() -> &'static Self {
        static INSTANCE: LazyLock<ProviderRegistry> = LazyLock::new(ProviderRegistry::new_default);
        &INSTANCE
    }

    pub fn register(&self, registration: ProviderRegistration) -> Result<(), AppError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| AppError::Config("ProviderRegistry write lock poisoned".to_string()))?;
        entries.insert(registration.metadata.kind, registration);
        Ok(())
    }

    pub fn get(&self, kind: TranslationProviderKind) -> Result<ProviderRegistration, AppError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| AppError::Config("ProviderRegistry read lock poisoned".to_string()))?;
        entries.get(&kind).cloned().ok_or_else(|| {
            AppError::Config(format!("No translator provider registered for '{kind}'"))
        })
    }

    pub fn registered_kinds(&self) -> Result<Vec<TranslationProviderKind>, AppError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| AppError::Config("ProviderRegistry read lock poisoned".to_string()))?;
        Ok(entries.keys().copied().collect())
    }

    pub fn all_metadata(&self) -> Result<Vec<ProviderMetadata>, AppError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| AppError::Config("ProviderRegistry read lock poisoned".to_string()))?;
        Ok(entries.values().map(|e| e.metadata.clone()).collect())
    }

    pub fn resolve_config(
        &self,
        kind: TranslationProviderKind,
        args: &CliArgs,
        is_headless: bool,
    ) -> Result<ProviderConfig, AppError> {
        let registration = self.get(kind)?;
        registration.option_resolver.resolve(args, is_headless)
    }

    pub fn create_translator(
        &self,
        config: &ProviderConfig,
    ) -> Result<Arc<dyn TranslatorPort>, AppError> {
        self.create_translator_with_cache(config, None)
    }

    pub fn create_translator_with_cache(
        &self,
        config: &ProviderConfig,
        cache: Option<Arc<dyn TranslationCachePort>>,
    ) -> Result<Arc<dyn TranslatorPort>, AppError> {
        let registration = self.get(config.provider_kind())?;
        match config {
            ProviderConfig::Antigravity {
                model,
                block_concurrency,
            } => {
                let mut adapter = AntigravityTranslatorAdapter::new(model.clone());
                if let Some(bc) = block_concurrency {
                    adapter = adapter.with_block_concurrency(*bc);
                }
                if let Some(c) = cache {
                    adapter = adapter.with_cache(c);
                }
                Ok(Arc::new(adapter))
            }
            _ => (registration.factory)(config),
        }
    }

    pub fn create_from_parts(
        &self,
        provider_kind: TranslationProviderKind,
        model: Option<String>,
        api_key: Option<String>,
    ) -> Result<Arc<dyn TranslatorPort>, AppError> {
        let config = ProviderConfig::new(provider_kind, model, api_key);
        self.create_translator(&config)
    }

    fn register_defaults(&self) {
        let antigravity_entry = ProviderRegistration::new(
            ProviderMetadata::new(
                TranslationProviderKind::Antigravity,
                "Google Antigravity",
                "Modelos generativos do Google Antigravity (Gemini)",
                false,
            ),
            Arc::new(AntigravityOptionResolver),
            Arc::new(|config| match config {
                ProviderConfig::Antigravity {
                    model,
                    block_concurrency,
                } => {
                    let mut adapter = AntigravityTranslatorAdapter::new(model.clone());
                    if let Some(bc) = block_concurrency {
                        adapter = adapter.with_block_concurrency(*bc);
                    }
                    Ok(Arc::new(adapter))
                }
                _ => Err(AppError::Config(
                    "Invalid config for Antigravity provider".to_string(),
                )),
            }),
        );

        let google_free_entry = ProviderRegistration::new(
            ProviderMetadata::new(
                TranslationProviderKind::GoogleFree,
                "Google Free Endpoint",
                "Endpoint gratuito gtx do Google Translate sem chaves",
                false,
            ),
            Arc::new(GoogleFreeOptionResolver),
            Arc::new(|config| match config {
                ProviderConfig::GoogleFree => Ok(Arc::new(GoogleFreeTranslatorAdapter::new()?)),
                _ => Err(AppError::Config(
                    "Invalid config for Google Free provider".to_string(),
                )),
            }),
        );

        let google_cloud_entry = ProviderRegistration::new(
            ProviderMetadata::new(
                TranslationProviderKind::GoogleCloud,
                "Google Cloud Translation API",
                "API oficial do Google Cloud Translation v2",
                true,
            ),
            Arc::new(GoogleCloudOptionResolver),
            Arc::new(|config| match config {
                ProviderConfig::GoogleCloud { api_key } => Ok(Arc::new(
                    GoogleCloudTranslatorAdapter::new(api_key.clone())?,
                )),
                _ => Err(AppError::Config(
                    "Invalid config for Google Cloud provider".to_string(),
                )),
            }),
        );

        let _ = self.register(antigravity_entry);
        let _ = self.register(google_free_entry);
        let _ = self.register(google_cloud_entry);
    }
}

pub struct TranslatorFactory;

impl TranslatorFactory {
    pub fn create(config: &ProviderConfig) -> Result<Arc<dyn TranslatorPort>, AppError> {
        Self::create_with_cache(config, None)
    }

    pub fn create_with_cache(
        config: &ProviderConfig,
        cache: Option<Arc<dyn TranslationCachePort>>,
    ) -> Result<Arc<dyn TranslatorPort>, AppError> {
        ProviderRegistry::global().create_translator_with_cache(config, cache)
    }

    pub fn create_from_parts(
        provider_kind: TranslationProviderKind,
        model: Option<String>,
        api_key: Option<String>,
    ) -> Result<Arc<dyn TranslatorPort>, AppError> {
        ProviderRegistry::global().create_from_parts(provider_kind, model, api_key)
    }
}
