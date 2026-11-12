use crate::adapters::cli::args::CliArgs;
use crate::adapters::cli::interactive::InteractivePrompter;
use crate::domain::entities::{ProviderConfig, TranslationProviderKind};
use crate::domain::errors::AppError;
use std::sync::Arc;

pub use crate::adapters::translator::{
    ProviderMetadata, ProviderRegistration, ProviderRegistry, TranslatorConstructor,
};

pub trait ProviderOptionResolver: Send + Sync {
    fn resolve(&self, args: &CliArgs, is_headless: bool) -> Result<ProviderConfig, AppError>;
}

pub struct AntigravityOptionResolver;

impl ProviderOptionResolver for AntigravityOptionResolver {
    fn resolve(&self, args: &CliArgs, is_headless: bool) -> Result<ProviderConfig, AppError> {
        let model = if is_headless {
            args.model.clone()
        } else {
            InteractivePrompter::prompt_model(args.model.clone())?
        };
        Ok(ProviderConfig::Antigravity {
            model,
            block_concurrency: Some(args.block_concurrency as usize),
        })
    }
}

pub struct GoogleFreeOptionResolver;

impl ProviderOptionResolver for GoogleFreeOptionResolver {
    fn resolve(&self, _args: &CliArgs, _is_headless: bool) -> Result<ProviderConfig, AppError> {
        Ok(ProviderConfig::GoogleFree)
    }
}

pub struct GoogleCloudOptionResolver;

impl GoogleCloudOptionResolver {
    pub fn resolve_api_key(&self, args: &CliArgs, is_headless: bool) -> Result<String, AppError> {
        self.resolve_api_key_with_env(
            args,
            is_headless,
            || std::env::var("GOOGLE_TRANSLATE_API_KEY").ok(),
            || std::env::var("GOOGLE_API_KEY").ok(),
        )
    }

    pub fn resolve_api_key_with_env<F1, F2>(
        &self,
        args: &CliArgs,
        is_headless: bool,
        translate_api_key_fn: F1,
        google_api_key_fn: F2,
    ) -> Result<String, AppError>
    where
        F1: FnOnce() -> Option<String>,
        F2: FnOnce() -> Option<String>,
    {
        if let Some(key) = args
            .cloud_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(key.to_string());
        }

        if let Some(key) = args
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(key.to_string());
        }

        if let Some(key) = translate_api_key_fn() {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }

        if let Some(key) = google_api_key_fn() {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }

        if is_headless {
            return Ok(String::new());
        }

        InteractivePrompter::prompt_google_cloud_api_key()
    }
}

impl ProviderOptionResolver for GoogleCloudOptionResolver {
    fn resolve(&self, args: &CliArgs, is_headless: bool) -> Result<ProviderConfig, AppError> {
        let api_key = self.resolve_api_key(args, is_headless)?;
        Ok(ProviderConfig::GoogleCloud { api_key })
    }
}

pub struct ProviderConfigResolver;

impl ProviderConfigResolver {
    pub fn resolver_for(
        provider_kind: TranslationProviderKind,
    ) -> Result<Arc<dyn ProviderOptionResolver>, AppError> {
        let registration = ProviderRegistry::global().get(provider_kind)?;
        Ok(registration.option_resolver)
    }

    pub fn resolve(
        provider_kind: TranslationProviderKind,
        args: &CliArgs,
        is_headless: bool,
    ) -> Result<ProviderConfig, AppError> {
        ProviderRegistry::global().resolve_config(provider_kind, args, is_headless)
    }
}
