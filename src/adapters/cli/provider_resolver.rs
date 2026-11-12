use crate::adapters::cli::args::CliArgs;
use crate::adapters::cli::interactive::InteractivePrompter;
use crate::domain::entities::{ProviderConfig, TranslationProviderKind};
use crate::domain::errors::AppError;
use std::sync::Arc;

pub use crate::adapters::translator::{
    AntigravityOptionResolver, GoogleCloudOptionResolver, GoogleFreeOptionResolver,
    ProviderMetadata, ProviderOptionResolver, ProviderOptions, ProviderRegistration,
    ProviderRegistry, TranslatorConstructor,
};

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
        let options = args.to_provider_options(is_headless);
        ProviderRegistry::global().resolve_config(provider_kind, &options)
    }

    pub async fn resolve_interactive(
        provider_kind: TranslationProviderKind,
        args: &CliArgs,
        is_headless: bool,
    ) -> Result<ProviderConfig, AppError> {
        let model = if is_headless || provider_kind != TranslationProviderKind::Antigravity {
            args.model.clone()
        } else {
            InteractivePrompter::prompt_model(args.model.clone()).await?
        };

        let mut api_key = args.api_key.clone();
        if !is_headless && provider_kind == TranslationProviderKind::GoogleCloud {
            let resolver = GoogleCloudOptionResolver;
            let options = ProviderOptions {
                model: None,
                block_concurrency: 1,
                api_key: args.api_key.clone(),
                cloud_key: args.cloud_key.clone(),
                is_headless: true,
            };
            let resolved_key = resolver.resolve_api_key(&options)?;
            if resolved_key.is_empty() {
                api_key = Some(InteractivePrompter::prompt_google_cloud_api_key()?);
            }
        }

        let options = ProviderOptions {
            model,
            block_concurrency: args.block_concurrency as usize,
            api_key,
            cloud_key: args.cloud_key.clone(),
            is_headless,
        };
        ProviderRegistry::global().resolve_config(provider_kind, &options)
    }
}
